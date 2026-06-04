//! Minimal JavaScript execution and DOM bindings for Webby.
//!
//! `webby_js` owns JavaScript engine integration and JavaScript-facing DOM
//! bindings. Tree mutation helpers live in `webby_dom`; layout and rendering
//! continue to consume the normal styled/layout/display-list pipeline.

use boa_engine::{Context, Source, vm::RuntimeLimits};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use webby_core::{WebbyError, WebbyResult};
use webby_dom::{Document, Node, NodeId, NodeKind};

const PRELUDE: &str = r#"
var __webby_console = [];
var console = {
  log: function() {
    var parts = [];
    for (var index = 0; index < arguments.length; index = index + 1) {
      parts.push(String(arguments[index]));
    }
    __webby_console.push(parts.join(" "));
  }
};
var window = {};
"#;

const CONSOLE_SNAPSHOT: &str = "JSON.stringify(__webby_console)";
const MUTATION_SNAPSHOT: &str = "JSON.stringify(__webby_mutations)";
const EVENT_HANDLER_SNAPSHOT: &str = "JSON.stringify(__webby_event_handlers)";
const BROWSER_ACTION_SNAPSHOT: &str = "JSON.stringify(__webby_browser_actions)";
const STORAGE_ACTION_SNAPSHOT: &str = "JSON.stringify(__webby_storage_actions)";
const CUSTOM_ELEMENT_DIAGNOSTIC_SNAPSHOT: &str =
    "JSON.stringify(__webby_custom_element_diagnostics)";

/// One script supplied to the JavaScript executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptSource {
    /// Deterministic human-facing label for diagnostics.
    pub label: String,
    /// JavaScript source code.
    pub code: String,
}

impl ScriptSource {
    /// Creates a script source with a deterministic label.
    pub fn new(label: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            code: code.into(),
        }
    }
}

/// JavaScript execution options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionOptions {
    /// Whether script execution is enabled.
    pub enabled: bool,
    /// Maximum inline scripts to execute. Extra scripts are skipped with a
    /// deterministic diagnostic.
    pub max_scripts: usize,
    /// Maximum bytes per script. Larger scripts are skipped with a
    /// deterministic diagnostic.
    pub max_script_bytes: usize,
    /// Maximum JavaScript loop iterations before Boa raises a runtime error.
    pub max_loop_iterations: u64,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            max_scripts: 64,
            max_script_bytes: 256 * 1024,
            max_loop_iterations: 100_000,
        }
    }
}

/// Result of JavaScript execution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionReport {
    /// Captured console and error diagnostics in deterministic order.
    pub diagnostics: Vec<String>,
    /// Registered DOM event handlers in deterministic registration order.
    pub event_handlers: Vec<EventHandler>,
    /// Browser API requests made by JavaScript.
    pub browser_actions: Vec<BrowserAction>,
    /// Web Storage mutations requested by JavaScript.
    pub storage_actions: Vec<StorageAction>,
    /// Pipeline stages dirtied by DOM mutations during execution.
    pub dirty: DirtyState,
}

/// Pipeline invalidation state caused by JavaScript DOM mutations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirtyState {
    /// DOM tree content or attributes changed.
    pub dom: bool,
    /// Style matching or computed style may have changed.
    pub style: bool,
    /// Layout geometry may have changed.
    pub layout: bool,
    /// Rendered pixels may have changed.
    pub render: bool,
}

impl DirtyState {
    fn dom_layout_render() -> Self {
        Self {
            dom: true,
            style: false,
            layout: true,
            render: true,
        }
    }

    fn all() -> Self {
        Self {
            dom: true,
            style: true,
            layout: true,
            render: true,
        }
    }

    fn merge(&mut self, other: Self) {
        self.dom |= other.dom;
        self.style |= other.style;
        self.layout |= other.layout;
        self.render |= other.render;
    }
}

/// JavaScript event handler registered on a DOM node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventHandler {
    /// DOM node id that owns the handler.
    pub id: String,
    /// Event type, such as `click`, `input`, or `submit`.
    pub event_type: String,
    /// JavaScript function source captured at registration time.
    pub handler: String,
}

/// JavaScript browser API request for the app shell to apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BrowserAction {
    /// `location.href = ...` or `location.assign(...)`.
    #[serde(rename = "navigate")]
    Navigate { url: String, replace: bool },
    /// `history.back()`.
    #[serde(rename = "historyBack")]
    HistoryBack,
    /// `history.forward()`.
    #[serde(rename = "historyForward")]
    HistoryForward,
    /// `setTimeout(...)`.
    #[serde(rename = "setTimeout")]
    SetTimeout {
        id: u32,
        callback: String,
        delay_ms: u32,
    },
    /// `clearTimeout(...)`.
    #[serde(rename = "clearTimeout")]
    ClearTimeout { id: u32 },
}

/// Browser API values exposed to JavaScript during execution.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserApiContext {
    /// Current page URL used by `location.href`.
    pub current_url: String,
    /// Security-checked resources available to the JavaScript fetch/XHR shims.
    pub fetch_resources: Vec<FetchResource>,
    /// Current localStorage entries for this origin.
    pub local_storage: Vec<StorageEntry>,
    /// Current sessionStorage entries for this tab/origin.
    pub session_storage: Vec<StorageEntry>,
    /// Whether Web Storage APIs are enabled.
    pub storage_enabled: bool,
    /// Per-storage-area quota in UTF-8 bytes.
    pub storage_quota_bytes: usize,
    /// JavaScript-visible viewport width.
    pub viewport_width: u32,
    /// JavaScript-visible viewport height.
    pub viewport_height: u32,
    /// Layout-backed element rectangles keyed by stable DOM node id.
    pub geometry: Vec<DomGeometry>,
}

/// Layout-backed geometry exposed to Webby's DOM APIs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomGeometry {
    /// Stable DOM node id.
    pub node_id: String,
    /// Border-box x coordinate in page coordinates.
    pub x: f32,
    /// Border-box y coordinate in page coordinates.
    pub y: f32,
    /// Border-box width.
    pub width: f32,
    /// Border-box height.
    pub height: f32,
}

/// Preloaded resource exposed to JavaScript fetch/XHR shims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchResource {
    /// JavaScript request string that should match this resource.
    pub request_url: String,
    /// Absolute final URL.
    pub url: String,
    /// HTTP status if available.
    pub status: u16,
    /// Decoded text response body.
    pub body: String,
    /// Controlled error to throw when security checks reject the request.
    pub error: Option<String>,
}

/// One Web Storage key/value pair exposed to JavaScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageEntry {
    /// Storage key.
    pub key: String,
    /// Storage value.
    pub value: String,
}

/// Storage area mutated by JavaScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageArea {
    /// Persistent localStorage.
    Local,
    /// Per-tab sessionStorage.
    Session,
}

/// One JavaScript Web Storage mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StorageAction {
    /// `setItem(key, value)`.
    #[serde(rename = "setItem")]
    SetItem {
        /// Storage area.
        area: StorageArea,
        /// Storage key.
        key: String,
        /// Storage value.
        value: String,
    },
    /// `removeItem(key)`.
    #[serde(rename = "removeItem")]
    RemoveItem {
        /// Storage area.
        area: StorageArea,
        /// Storage key.
        key: String,
    },
    /// `clear()`.
    #[serde(rename = "clear")]
    Clear {
        /// Storage area.
        area: StorageArea,
    },
}

impl Default for BrowserApiContext {
    fn default() -> Self {
        Self {
            current_url: String::new(),
            fetch_resources: Vec::new(),
            local_storage: Vec::new(),
            session_storage: Vec::new(),
            storage_enabled: true,
            storage_quota_bytes: 4096,
            viewport_width: 0,
            viewport_height: 0,
            geometry: Vec::new(),
        }
    }
}

/// Extracts static string URLs used by Webby's v0.1 fetch/XHR shims.
///
/// This is intentionally small and deterministic: dynamic request expressions
/// are unsupported until a fuller asynchronous Web API bridge exists.
pub fn collect_static_request_urls(scripts: &[ScriptSource]) -> Vec<String> {
    let mut urls = BTreeSet::new();
    for script in scripts {
        collect_call_string_arguments(&script.code, "fetch", 0, &mut urls);
        collect_call_string_arguments(&script.code, "open", 1, &mut urls);
    }
    urls.into_iter().collect()
}

fn collect_call_string_arguments(
    source: &str,
    function_name: &str,
    argument_index: usize,
    urls: &mut BTreeSet<String>,
) {
    let mut rest = source;
    let needle = format!("{function_name}(");
    while let Some(found) = rest.find(&needle) {
        let after_name = &rest[found + needle.len()..];
        if let Some(argument) = static_string_argument(after_name, argument_index) {
            urls.insert(argument);
        }
        rest = &after_name[after_name.len().min(1)..];
    }
}

fn static_string_argument(arguments: &str, argument_index: usize) -> Option<String> {
    let mut remaining = arguments.trim_start();
    for index in 0..=argument_index {
        if index > 0 {
            let comma = remaining.find(',')?;
            remaining = remaining[comma + 1..].trim_start();
        }
    }
    read_js_string_literal(remaining)
}

fn read_js_string_literal(input: &str) -> Option<String> {
    let mut chars = input.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut output = String::new();
    let mut escaped = false;
    for character in chars {
        if escaped {
            output.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return Some(output);
        } else {
            output.push(character);
        }
    }
    None
}

/// DOM event dispatch request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDispatch {
    /// Event type to dispatch.
    pub event_type: String,
    /// Target DOM node id.
    pub target_node_id: NodeId,
}

/// Result of dispatching one DOM event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventDispatchReport {
    /// Captured console/error diagnostics in deterministic order.
    pub diagnostics: Vec<String>,
    /// Whether a handler called `preventDefault()`.
    pub default_prevented: bool,
    /// Browser API requests made by event handlers.
    pub browser_actions: Vec<BrowserAction>,
    /// Web Storage mutations requested by event handlers.
    pub storage_actions: Vec<StorageAction>,
    /// Pipeline stages dirtied by event handler DOM mutations.
    pub dirty: DirtyState,
}

/// Executes inline scripts in source order.
pub fn execute_scripts(
    scripts: &[ScriptSource],
    options: ExecutionOptions,
) -> WebbyResult<ExecutionReport> {
    if !options.enabled {
        let diagnostics = if scripts.is_empty() {
            Vec::new()
        } else {
            vec!["JavaScript disabled by configuration".to_string()]
        };
        return Ok(ExecutionReport {
            diagnostics,
            event_handlers: Vec::new(),
            browser_actions: Vec::new(),
            storage_actions: Vec::new(),
            dirty: DirtyState::default(),
        });
    }

    let mut context = limited_context(options);
    eval_source(&mut context, "webby prelude", PRELUDE)?;
    eval_source(
        &mut context,
        "browser API prelude",
        &browser_api_prelude(&BrowserApiContext::default())?,
    )?;

    let mut diagnostics = run_script_sources(&mut context, scripts, options);
    diagnostics.extend(read_console_log(&mut context)?);
    Ok(ExecutionReport {
        diagnostics,
        event_handlers: Vec::new(),
        browser_actions: read_browser_actions(&mut context)?,
        storage_actions: read_storage_actions(&mut context)?,
        dirty: DirtyState::default(),
    })
}

/// Executes inline scripts with Webby's small DOM bindings and applies
/// deterministic DOM mutations before returning.
pub fn execute_scripts_with_dom(
    document: &mut Document,
    scripts: &[ScriptSource],
    options: ExecutionOptions,
) -> WebbyResult<ExecutionReport> {
    if scripts.is_empty() {
        return Ok(ExecutionReport::default());
    }

    if !options.enabled {
        return Ok(ExecutionReport {
            diagnostics: vec!["JavaScript disabled by configuration".to_string()],
            event_handlers: Vec::new(),
            browser_actions: Vec::new(),
            storage_actions: Vec::new(),
            dirty: DirtyState::default(),
        });
    }

    document.assign_stable_ids();
    let mut context = limited_context(options);
    eval_source(&mut context, "webby prelude", PRELUDE)?;
    eval_source(
        &mut context,
        "browser API prelude",
        &browser_api_prelude(&BrowserApiContext::default())?,
    )?;
    eval_source(&mut context, "DOM prelude", &dom_prelude(document)?)?;

    let mut diagnostics = run_script_sources(&mut context, scripts, options);
    let mutation_report = apply_dom_mutations(document, &mut context)?;
    diagnostics.extend(mutation_report.diagnostics);
    let event_handlers = read_event_handlers(&mut context)?;
    diagnostics.extend(read_custom_element_diagnostics(&mut context)?);
    diagnostics.extend(read_console_log(&mut context)?);
    Ok(ExecutionReport {
        diagnostics,
        event_handlers,
        browser_actions: read_browser_actions(&mut context)?,
        storage_actions: read_storage_actions(&mut context)?,
        dirty: mutation_report.dirty,
    })
}

/// Executes inline scripts with DOM bindings and browser API shims.
pub fn execute_scripts_with_dom_and_browser(
    document: &mut Document,
    scripts: &[ScriptSource],
    options: ExecutionOptions,
    browser: &BrowserApiContext,
) -> WebbyResult<ExecutionReport> {
    if scripts.is_empty() {
        return Ok(ExecutionReport::default());
    }

    if !options.enabled {
        return Ok(ExecutionReport {
            diagnostics: vec!["JavaScript disabled by configuration".to_string()],
            event_handlers: Vec::new(),
            browser_actions: Vec::new(),
            storage_actions: Vec::new(),
            dirty: DirtyState::default(),
        });
    }

    document.assign_stable_ids();
    let mut context = limited_context(options);
    eval_source(&mut context, "webby prelude", PRELUDE)?;
    eval_source(
        &mut context,
        "browser API prelude",
        &browser_api_prelude(browser)?,
    )?;
    eval_source(&mut context, "DOM prelude", &dom_prelude(document)?)?;

    let mut diagnostics = run_script_sources(&mut context, scripts, options);
    let mutation_report = apply_dom_mutations(document, &mut context)?;
    diagnostics.extend(mutation_report.diagnostics);
    let event_handlers = read_event_handlers(&mut context)?;
    let browser_actions = read_browser_actions(&mut context)?;
    let storage_actions = read_storage_actions(&mut context)?;
    diagnostics.extend(read_custom_element_diagnostics(&mut context)?);
    diagnostics.extend(read_console_log(&mut context)?);
    Ok(ExecutionReport {
        diagnostics,
        event_handlers,
        browser_actions,
        storage_actions,
        dirty: mutation_report.dirty,
    })
}

/// Dispatches one DOM event against previously registered handlers and applies
/// resulting DOM mutations.
pub fn dispatch_event_with_dom(
    document: &mut Document,
    handlers: &[EventHandler],
    event: EventDispatch,
    options: ExecutionOptions,
) -> WebbyResult<EventDispatchReport> {
    dispatch_event_with_dom_and_browser(
        document,
        handlers,
        event,
        options,
        &BrowserApiContext::default(),
    )
}

/// Dispatches one DOM event with browser API values exposed to handlers.
pub fn dispatch_event_with_dom_and_browser(
    document: &mut Document,
    handlers: &[EventHandler],
    event: EventDispatch,
    options: ExecutionOptions,
    browser: &BrowserApiContext,
) -> WebbyResult<EventDispatchReport> {
    if handlers.is_empty() || !options.enabled {
        return Ok(EventDispatchReport::default());
    }

    document.assign_stable_ids();
    let mut context = limited_context(options);
    eval_source(&mut context, "webby prelude", PRELUDE)?;
    eval_source(
        &mut context,
        "browser API prelude",
        &browser_api_prelude(browser)?,
    )?;
    eval_source(&mut context, "DOM prelude", &dom_prelude(document)?)?;
    let handler_json = serde_json::to_string(handlers).map_err(|error| WebbyError::Parse {
        message: format!("JavaScript event handler serialization failed: {error}"),
    })?;
    let source = event_dispatch_source(&handler_json, &event)?;
    let mut diagnostics = Vec::new();
    if let Err(error) = context.eval(Source::from_bytes(source.as_bytes())) {
        diagnostics.push(format!(
            "JavaScript event error in {}: {error}",
            event.event_type
        ));
    }
    let mutation_report = apply_dom_mutations(document, &mut context)?;
    diagnostics.extend(mutation_report.diagnostics);
    diagnostics.extend(read_custom_element_diagnostics(&mut context)?);
    diagnostics.extend(read_console_log(&mut context)?);
    let default_prevented = read_bool(&mut context, "__webby_default_prevented")?;
    let browser_actions = read_browser_actions(&mut context)?;
    let storage_actions = read_storage_actions(&mut context)?;
    Ok(EventDispatchReport {
        diagnostics,
        default_prevented,
        browser_actions,
        storage_actions,
        dirty: mutation_report.dirty,
    })
}

/// Runs one stored timer callback with DOM and browser API bindings.
pub fn execute_timer_callback_with_dom(
    document: &mut Document,
    callback: &str,
    options: ExecutionOptions,
    browser: &BrowserApiContext,
) -> WebbyResult<ExecutionReport> {
    if !options.enabled {
        return Ok(ExecutionReport {
            diagnostics: vec!["JavaScript disabled by configuration".to_string()],
            event_handlers: Vec::new(),
            browser_actions: Vec::new(),
            storage_actions: Vec::new(),
            dirty: DirtyState::default(),
        });
    }

    document.assign_stable_ids();
    let mut context = limited_context(options);
    eval_source(&mut context, "webby prelude", PRELUDE)?;
    eval_source(
        &mut context,
        "browser API prelude",
        &browser_api_prelude(browser)?,
    )?;
    eval_source(&mut context, "DOM prelude", &dom_prelude(document)?)?;
    let source = timer_callback_source(callback)?;
    let mut diagnostics = Vec::new();
    if let Err(error) = context.eval(Source::from_bytes(source.as_bytes())) {
        diagnostics.push(format!("JavaScript timer error: {error}"));
    }
    let mutation_report = apply_dom_mutations(document, &mut context)?;
    diagnostics.extend(mutation_report.diagnostics);
    let event_handlers = read_event_handlers(&mut context)?;
    let browser_actions = read_browser_actions(&mut context)?;
    let storage_actions = read_storage_actions(&mut context)?;
    diagnostics.extend(read_custom_element_diagnostics(&mut context)?);
    diagnostics.extend(read_console_log(&mut context)?);
    Ok(ExecutionReport {
        diagnostics,
        event_handlers,
        browser_actions,
        storage_actions,
        dirty: mutation_report.dirty,
    })
}

fn limited_context(options: ExecutionOptions) -> Context {
    let mut context = Context::default();
    let mut runtime_limits = RuntimeLimits::default();
    runtime_limits.set_loop_iteration_limit(options.max_loop_iterations);
    context.set_runtime_limits(runtime_limits);
    context
}

fn run_script_sources(
    context: &mut Context,
    scripts: &[ScriptSource],
    options: ExecutionOptions,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (index, script) in scripts.iter().enumerate() {
        if index >= options.max_scripts {
            diagnostics.push(format!(
                "JavaScript skipped {}: script count limit {} reached",
                script.label, options.max_scripts
            ));
            continue;
        }
        if script.code.len() > options.max_script_bytes {
            diagnostics.push(format!(
                "JavaScript skipped {}: script is {} bytes, limit is {} bytes",
                script.label,
                script.code.len(),
                options.max_script_bytes
            ));
            continue;
        }
        if let Err(error) = context.eval(Source::from_bytes(script.code.as_bytes())) {
            diagnostics.push(format!("JavaScript error in {}: {error}", script.label));
        }
    }
    diagnostics
}

fn eval_source(context: &mut Context, label: &str, source: &str) -> WebbyResult<()> {
    context
        .eval(Source::from_bytes(source.as_bytes()))
        .map(|_| ())
        .map_err(|error| WebbyError::unsupported(format!("JavaScript {label} failed: {error}")))
}

fn dom_prelude(document: &Document) -> WebbyResult<String> {
    let mirror = mirror_node(&document.root);
    let json = serde_json::to_string(&mirror).map_err(|error| WebbyError::Parse {
        message: format!("DOM mirror serialization failed: {error}"),
    })?;
    Ok(format!(
        "var __webby_dom_root = {json};\n{}",
        include_str!("dom_prelude.js")
    ))
}

fn browser_api_prelude(browser: &BrowserApiContext) -> WebbyResult<String> {
    let href = serde_json::to_string(&browser.current_url).map_err(|error| WebbyError::Parse {
        message: format!("JavaScript location serialization failed: {error}"),
    })?;
    let fetch_resources =
        serde_json::to_string(&browser.fetch_resources).map_err(|error| WebbyError::Parse {
            message: format!("JavaScript fetch resource serialization failed: {error}"),
        })?;
    let local_storage =
        serde_json::to_string(&browser.local_storage).map_err(|error| WebbyError::Parse {
            message: format!("JavaScript localStorage serialization failed: {error}"),
        })?;
    let session_storage =
        serde_json::to_string(&browser.session_storage).map_err(|error| WebbyError::Parse {
            message: format!("JavaScript sessionStorage serialization failed: {error}"),
        })?;
    let geometry = serde_json::to_string(&browser.geometry).map_err(|error| WebbyError::Parse {
        message: format!("JavaScript DOM geometry serialization failed: {error}"),
    })?;
    let storage_enabled = browser.storage_enabled;
    let storage_quota = browser.storage_quota_bytes;
    let viewport_width = browser.viewport_width;
    let viewport_height = browser.viewport_height;
    Ok(format!(
        r#"
var __webby_location_href = {href};
var __webby_fetch_resources = {fetch_resources};
var __webby_local_storage_entries = {local_storage};
var __webby_session_storage_entries = {session_storage};
var __webby_storage_enabled = {storage_enabled};
var __webby_storage_quota_bytes = {storage_quota};
var __webby_viewport_width = {viewport_width};
var __webby_viewport_height = {viewport_height};
var __webby_geometry_entries = {geometry};
var __webby_browser_actions = [];
var __webby_storage_actions = [];
var __webby_next_timer_id = 1;
function __webby_make_storage(area, entries) {{
  var store = {{}};
  var keys = [];
  function utf8Len(value) {{
    var string = String(value);
    var total = 0;
    for (var index = 0; index < string.length; index = index + 1) {{
      var code = string.charCodeAt(index);
      if (code >= 0xD800 && code <= 0xDBFF && index + 1 < string.length) {{
        var next = string.charCodeAt(index + 1);
        if (next >= 0xDC00 && next <= 0xDFFF) {{
          total = total + 4;
          index = index + 1;
          continue;
        }}
      }}
      if (code <= 0x7F) {{
        total = total + 1;
      }} else if (code <= 0x7FF) {{
        total = total + 2;
      }} else {{
        total = total + 3;
      }}
    }}
    return total;
  }}
  function normalizeKey(key) {{
    return String(key);
  }}
  function normalizeValue(value) {{
    return String(value);
  }}
  function sortKeys() {{
    keys.sort();
  }}
  function totalBytes(nextKey, nextValue, removing) {{
    var seen = {{}};
    var total = 0;
    for (var index = 0; index < keys.length; index = index + 1) {{
      var key = keys[index];
      if (removing === key) {{
        continue;
      }}
      var value = store[key];
      if (nextKey === key) {{
        value = nextValue;
      }}
      seen[key] = true;
      total = total + utf8Len(key) + utf8Len(value);
    }}
    if (nextKey !== null && !seen[nextKey]) {{
      total = total + utf8Len(nextKey) + utf8Len(nextValue);
    }}
    return total;
  }}
  for (var entryIndex = 0; entryIndex < entries.length; entryIndex = entryIndex + 1) {{
    var entry = entries[entryIndex];
    var key = normalizeKey(entry.key);
    if (!Object.prototype.hasOwnProperty.call(store, key)) {{
      keys.push(key);
    }}
    store[key] = normalizeValue(entry.value);
  }}
  sortKeys();
  return {{
    get length() {{
      return keys.length;
    }},
    key: function(index) {{
      var numeric = Number(index);
      if (!isFinite(numeric)) {{
        return null;
      }}
      var key = keys[Math.floor(numeric)];
      return key === undefined ? null : key;
    }},
    getItem: function(key) {{
      var normalized = normalizeKey(key);
      if (Object.prototype.hasOwnProperty.call(store, normalized)) {{
        return store[normalized];
      }}
      return null;
    }},
    setItem: function(key, value) {{
      if (!__webby_storage_enabled) {{
        throw new Error("Web Storage disabled by configuration");
      }}
      var normalizedKey = normalizeKey(key);
      var normalizedValue = normalizeValue(value);
      var byteLen = totalBytes(normalizedKey, normalizedValue, null);
      if (byteLen > __webby_storage_quota_bytes) {{
        throw new Error("Web Storage quota exceeded");
      }}
      if (!Object.prototype.hasOwnProperty.call(store, normalizedKey)) {{
        keys.push(normalizedKey);
        sortKeys();
      }}
      store[normalizedKey] = normalizedValue;
      __webby_storage_actions.push({{ type: "setItem", area: area, key: normalizedKey, value: normalizedValue }});
    }},
    removeItem: function(key) {{
      if (!__webby_storage_enabled) {{
        throw new Error("Web Storage disabled by configuration");
      }}
      var normalized = normalizeKey(key);
      if (Object.prototype.hasOwnProperty.call(store, normalized)) {{
        delete store[normalized];
        var nextKeys = [];
        for (var keyIndex = 0; keyIndex < keys.length; keyIndex = keyIndex + 1) {{
          if (keys[keyIndex] !== normalized) {{
            nextKeys.push(keys[keyIndex]);
          }}
        }}
        keys = nextKeys;
      }}
      __webby_storage_actions.push({{ type: "removeItem", area: area, key: normalized }});
    }},
    clear: function() {{
      if (!__webby_storage_enabled) {{
        throw new Error("Web Storage disabled by configuration");
      }}
      store = {{}};
      keys = [];
      __webby_storage_actions.push({{ type: "clear", area: area }});
    }}
  }};
}}
window.localStorage = __webby_make_storage("local", __webby_local_storage_entries);
window.sessionStorage = __webby_make_storage("session", __webby_session_storage_entries);
function __webby_fetch_entry(input) {{
  var key = String(input);
  for (var index = 0; index < __webby_fetch_resources.length; index = index + 1) {{
    if (__webby_fetch_resources[index].request_url === key || __webby_fetch_resources[index].url === key) {{
      if (__webby_fetch_resources[index].error) {{
        throw new Error(__webby_fetch_resources[index].error);
      }}
      return __webby_fetch_resources[index];
    }}
  }}
  throw new Error("fetch resource unavailable: " + key);
}}
function __webby_response(entry) {{
  return {{
    status: entry.status,
    ok: entry.status >= 200 && entry.status < 300,
    url: entry.url,
    text: function() {{
      return entry.body;
    }},
    then: function(resolve) {{
      if (typeof resolve === "function") {{
        resolve(this);
      }}
      return this;
    }}
  }};
}}
window.location = {{}};
Object.defineProperty(window.location, "href", {{
  get: function() {{
    return __webby_location_href;
  }},
  set: function(value) {{
    __webby_location_href = String(value);
    __webby_browser_actions.push({{ type: "navigate", url: String(value), replace: false }});
  }}
}});
window.location.assign = function(value) {{
  window.location.href = value;
}};
window.history = {{
  back: function() {{
    __webby_browser_actions.push({{ type: "historyBack" }});
  }},
  forward: function() {{
    __webby_browser_actions.push({{ type: "historyForward" }});
  }}
}};
window.setTimeout = function(callback, delay) {{
  if (typeof callback !== "function") {{
    throw new Error("setTimeout callback must be a function");
  }}
  var id = __webby_next_timer_id;
  __webby_next_timer_id = __webby_next_timer_id + 1;
  var delayNumber = Number(delay);
  if (!isFinite(delayNumber) || delayNumber < 0) {{
    delayNumber = 0;
  }}
  if (delayNumber > 2147483647) {{
    delayNumber = 2147483647;
  }}
  __webby_browser_actions.push({{
    type: "setTimeout",
    id: id,
    callback: String(callback),
    delay_ms: Math.floor(delayNumber)
  }});
  return id;
}};
window.clearTimeout = function(id) {{
  __webby_browser_actions.push({{ type: "clearTimeout", id: Number(id) || 0 }});
}};
window.fetch = function(input) {{
  return __webby_response(__webby_fetch_entry(input));
}};
window.XMLHttpRequest = function() {{
  this.method = "";
  this.requestUrl = "";
  this.status = 0;
  this.responseText = "";
  this.onload = null;
}};
window.XMLHttpRequest.prototype.open = function(method, input) {{
  this.method = String(method).toUpperCase();
  this.requestUrl = String(input);
}};
window.XMLHttpRequest.prototype.send = function() {{
  if (this.method !== "GET") {{
    throw new Error("XMLHttpRequest only supports GET");
  }}
  var entry = __webby_fetch_entry(this.requestUrl);
  this.status = entry.status;
  this.responseText = entry.body;
  this.responseURL = entry.url;
  if (typeof this.onload === "function") {{
    this.onload();
  }}
}};
var location = window.location;
var history = window.history;
var setTimeout = window.setTimeout;
var clearTimeout = window.clearTimeout;
var fetch = window.fetch;
var XMLHttpRequest = window.XMLHttpRequest;
var localStorage = window.localStorage;
var sessionStorage = window.sessionStorage;
"#
    ))
}

fn timer_callback_source(callback: &str) -> WebbyResult<String> {
    let callback = serde_json::to_string(callback).map_err(|error| WebbyError::Parse {
        message: format!("JavaScript timer callback serialization failed: {error}"),
    })?;
    Ok(format!(
        r#"
var __webby_timer_callback = eval("(" + {callback} + ")");
if (typeof __webby_timer_callback !== "function") {{
  throw new Error("timer callback is not a function");
}}
__webby_timer_callback.call(window);
"#
    ))
}

fn event_dispatch_source(handler_json: &str, event: &EventDispatch) -> WebbyResult<String> {
    let event_type =
        serde_json::to_string(&event.event_type).map_err(|error| WebbyError::Parse {
            message: format!("JavaScript event type serialization failed: {error}"),
        })?;
    let target = event.target_node_id.to_string();
    Ok(format!(
        r#"
var __webby_registered_event_handlers = {handler_json};
var __webby_default_prevented = false;
function __webby_event_path(targetId) {{
  var path = [];
  var node = __webby_nodes[String(targetId)];
  while (node) {{
    path.push(node.id);
    node = __webby_nodes[node.parent];
  }}
  return path;
}}
function __webby_dispatch_registered_event(eventType, targetId) {{
  var path = __webby_event_path(targetId);
  for (var pathIndex = 0; pathIndex < path.length; pathIndex = pathIndex + 1) {{
    var currentId = path[pathIndex];
    for (var handlerIndex = 0; handlerIndex < __webby_registered_event_handlers.length; handlerIndex = handlerIndex + 1) {{
      var registration = __webby_registered_event_handlers[handlerIndex];
      if (registration.id === currentId && registration.event_type === eventType) {{
        var event = {{
          type: eventType,
          target: __webby_ref(String(targetId)),
          currentTarget: __webby_ref(currentId),
          defaultPrevented: false,
          preventDefault: function() {{
            this.defaultPrevented = true;
            __webby_default_prevented = true;
          }}
        }};
        var handler = eval("(" + registration.handler + ")");
        handler.call(__webby_ref(currentId), event);
      }}
    }}
  }}
}}
__webby_dispatch_registered_event({event_type}, "{target}");
"#
    ))
}

#[derive(Debug, Clone, Serialize)]
struct MirrorNode {
    id: String,
    kind: &'static str,
    tag_name: String,
    text: String,
    attributes: BTreeMap<String, String>,
    children: Vec<MirrorNode>,
    shadow_children: Vec<MirrorNode>,
}

fn mirror_node(node: &Node) -> MirrorNode {
    match &node.kind {
        NodeKind::Document => MirrorNode {
            id: node.id.to_string(),
            kind: "document",
            tag_name: String::new(),
            text: String::new(),
            attributes: BTreeMap::new(),
            children: node.children.iter().map(mirror_node).collect(),
            shadow_children: node.shadow_children.iter().map(mirror_node).collect(),
        },
        NodeKind::Element(element) => MirrorNode {
            id: node.id.to_string(),
            kind: "element",
            tag_name: element.tag_name.clone(),
            text: String::new(),
            attributes: element.attributes.clone(),
            children: node.children.iter().map(mirror_node).collect(),
            shadow_children: node.shadow_children.iter().map(mirror_node).collect(),
        },
        NodeKind::Text(text) => MirrorNode {
            id: node.id.to_string(),
            kind: "text",
            tag_name: String::new(),
            text: text.clone(),
            attributes: BTreeMap::new(),
            children: Vec::new(),
            shadow_children: Vec::new(),
        },
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op")]
enum DomMutation {
    #[serde(rename = "createElement")]
    CreateElement { id: String, tag_name: String },
    #[serde(rename = "createTextNode")]
    CreateTextNode { id: String, text: String },
    #[serde(rename = "setTextContent")]
    SetTextContent { id: String, text: String },
    #[serde(rename = "appendChild")]
    AppendChild { parent: String, child: String },
    #[serde(rename = "attachShadow")]
    AttachShadow { host: String, mode: String },
    #[serde(rename = "appendShadowChild")]
    AppendShadowChild { host: String, child: String },
    #[serde(rename = "remove")]
    Remove { id: String },
    #[serde(rename = "setAttribute")]
    SetAttribute {
        id: String,
        name: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct DomMutationReport {
    diagnostics: Vec<String>,
    dirty: DirtyState,
}

fn apply_dom_mutations(
    document: &mut Document,
    context: &mut Context,
) -> WebbyResult<DomMutationReport> {
    let value = context
        .eval(Source::from_bytes(MUTATION_SNAPSHOT.as_bytes()))
        .map_err(|error| WebbyError::unsupported(format!("JavaScript DOM read failed: {error}")))?;
    let Some(json) = value.as_string() else {
        return Ok(DomMutationReport::default());
    };
    let operations = serde_json::from_str::<Vec<DomMutation>>(&json.to_std_string_lossy())
        .map_err(|error| WebbyError::Parse {
            message: format!("JavaScript DOM mutation JSON parse failed: {error}"),
        })?;
    let mut created = HashMap::new();
    let mut aliases = HashMap::new();
    let mut diagnostics = Vec::new();
    let mut dirty = DirtyState::default();
    let mut next_id = document.next_node_id();
    for operation in operations {
        match apply_dom_mutation(
            document,
            &mut created,
            &mut aliases,
            &mut next_id,
            operation,
        ) {
            Ok(operation_dirty) => dirty.merge(operation_dirty),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    Ok(DomMutationReport { diagnostics, dirty })
}

fn apply_dom_mutation(
    document: &mut Document,
    created: &mut HashMap<String, Node>,
    aliases: &mut HashMap<String, NodeId>,
    next_id: &mut NodeId,
    operation: DomMutation,
) -> Result<DirtyState, String> {
    match operation {
        DomMutation::CreateElement { id, tag_name } => {
            let mut node = Node::element(tag_name, BTreeMap::new());
            node.id = take_next_id(next_id);
            created.insert(id, node);
            Ok(DirtyState::default())
        }
        DomMutation::CreateTextNode { id, text } => {
            let mut node = Node::text(text);
            node.id = take_next_id(next_id);
            created.insert(id, node);
            Ok(DirtyState::default())
        }
        DomMutation::SetTextContent { id, text } => {
            if let Some(node) = created.get_mut(&id) {
                set_created_text_content(node, text, next_id);
                return Ok(DirtyState::default());
            }
            let Some(node_id) = resolve_mutation_id(&id, aliases) else {
                return Err(format!("JavaScript DOM mutation skipped invalid node {id}"));
            };
            if document.set_text_content(node_id, text) {
                Ok(DirtyState::dom_layout_render())
            } else {
                Err(format!("JavaScript DOM mutation skipped missing node {id}"))
            }
        }
        DomMutation::AppendChild { parent, child } => {
            let child_node = if let Some(node) = created.remove(&child) {
                Some(node)
            } else {
                resolve_mutation_id(&child, aliases)
                    .and_then(|node_id| document.remove_node(node_id))
            };
            let Some(child_node) = child_node else {
                return Err(format!(
                    "JavaScript DOM mutation skipped missing child {child}"
                ));
            };
            let child_node_id = child_node.id;
            if let Some(parent_node) = created.get_mut(&parent) {
                parent_node.children.push(child_node);
                return Ok(DirtyState::default());
            }
            let Some(parent_id) = resolve_mutation_id(&parent, aliases) else {
                return Err(format!(
                    "JavaScript DOM mutation skipped invalid parent {parent}"
                ));
            };
            if document.append_child(parent_id, child_node) {
                aliases.insert(child, child_node_id);
                Ok(DirtyState::all())
            } else {
                Err(format!(
                    "JavaScript DOM mutation skipped missing parent {parent}"
                ))
            }
        }
        DomMutation::AttachShadow { host, mode } => {
            if mode != "open" {
                return Err(format!(
                    "JavaScript Shadow DOM attachShadow skipped unsupported mode {mode:?}"
                ));
            }
            if let Some(host_node) = created.get_mut(&host) {
                host_node.shadow_children.clear();
                return Ok(DirtyState::default());
            }
            let Some(host_id) = resolve_mutation_id(&host, aliases) else {
                return Err(format!(
                    "JavaScript Shadow DOM attachShadow skipped invalid host {host}"
                ));
            };
            if document.attach_shadow_root(host_id) {
                Ok(DirtyState::all())
            } else {
                Err(format!(
                    "JavaScript Shadow DOM attachShadow skipped missing host {host}"
                ))
            }
        }
        DomMutation::AppendShadowChild { host, child } => {
            let child_node = if let Some(node) = created.remove(&child) {
                Some(node)
            } else {
                resolve_mutation_id(&child, aliases)
                    .and_then(|node_id| document.remove_node(node_id))
            };
            let Some(child_node) = child_node else {
                return Err(format!(
                    "JavaScript Shadow DOM mutation skipped missing child {child}"
                ));
            };
            let child_node_id = child_node.id;
            if let Some(host_node) = created.get_mut(&host) {
                host_node.shadow_children.push(child_node);
                return Ok(DirtyState::default());
            }
            let Some(host_id) = resolve_mutation_id(&host, aliases) else {
                return Err(format!(
                    "JavaScript Shadow DOM mutation skipped invalid host {host}"
                ));
            };
            if document.append_shadow_child(host_id, child_node) {
                aliases.insert(child, child_node_id);
                Ok(DirtyState::all())
            } else {
                Err(format!(
                    "JavaScript Shadow DOM mutation skipped missing host {host}"
                ))
            }
        }
        DomMutation::Remove { id } => {
            let removed = created.remove(&id).is_some()
                || resolve_mutation_id(&id, aliases)
                    .and_then(|node_id| document.remove_node(node_id))
                    .is_some();
            if removed {
                Ok(DirtyState::all())
            } else {
                Err(format!("JavaScript DOM mutation skipped missing node {id}"))
            }
        }
        DomMutation::SetAttribute { id, name, value } => {
            if let Some(node) = created.get_mut(&id) {
                if let NodeKind::Element(element) = &mut node.kind {
                    element.attributes.insert(name.to_ascii_lowercase(), value);
                    return Ok(DirtyState::default());
                }
                return Err(format!("JavaScript DOM mutation skipped non-element {id}"));
            }
            if resolve_mutation_id(&id, aliases)
                .is_some_and(|node_id| document.set_attribute(node_id, &name, value))
            {
                Ok(dirty_for_attribute(&name))
            } else {
                Err(format!(
                    "JavaScript DOM mutation skipped missing element {id}"
                ))
            }
        }
    }
}

fn read_custom_element_diagnostics(context: &mut Context) -> WebbyResult<Vec<String>> {
    let value = context
        .eval(Source::from_bytes(
            CUSTOM_ELEMENT_DIAGNOSTIC_SNAPSHOT.as_bytes(),
        ))
        .map_err(|error| {
            WebbyError::unsupported(format!(
                "JavaScript custom element diagnostic read failed: {error}"
            ))
        })?;
    let Some(json) = value.as_string() else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<String>>(&json.to_std_string_lossy()).map_err(|error| {
        WebbyError::Parse {
            message: format!("JavaScript custom element diagnostic JSON parse failed: {error}"),
        }
    })
}

fn dirty_for_attribute(name: &str) -> DirtyState {
    match name.to_ascii_lowercase().as_str() {
        "class" | "id" | "style" => DirtyState::all(),
        "href" | "action" | "method" | "name" | "value" | "type" | "placeholder" => {
            DirtyState::dom_layout_render()
        }
        _ => DirtyState {
            dom: true,
            style: false,
            layout: false,
            render: false,
        },
    }
}

fn set_created_text_content(node: &mut Node, text: String, next_id: &mut NodeId) {
    match &mut node.kind {
        NodeKind::Text(value) => *value = text,
        NodeKind::Document | NodeKind::Element(_) => {
            let mut child = Node::text(text);
            child.id = take_next_id(next_id);
            node.children.clear();
            node.children.push(child);
        }
    }
}

fn take_next_id(next_id: &mut NodeId) -> NodeId {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);
    id
}

fn parse_existing_id(id: &str) -> Option<NodeId> {
    id.parse::<NodeId>().ok()
}

fn resolve_mutation_id(id: &str, aliases: &HashMap<String, NodeId>) -> Option<NodeId> {
    aliases.get(id).copied().or_else(|| parse_existing_id(id))
}

fn read_console_log(context: &mut Context) -> WebbyResult<Vec<String>> {
    let value = context
        .eval(Source::from_bytes(CONSOLE_SNAPSHOT.as_bytes()))
        .map_err(|error| {
            WebbyError::unsupported(format!("JavaScript console read failed: {error}"))
        })?;
    let Some(json) = value.as_string() else {
        return Ok(Vec::new());
    };
    let lines =
        serde_json::from_str::<Vec<String>>(&json.to_std_string_lossy()).map_err(|error| {
            WebbyError::Parse {
                message: format!("JavaScript console JSON parse failed: {error}"),
            }
        })?;
    Ok(lines
        .into_iter()
        .map(|line| format!("JavaScript console: {line}"))
        .collect())
}

fn read_event_handlers(context: &mut Context) -> WebbyResult<Vec<EventHandler>> {
    let value = context
        .eval(Source::from_bytes(EVENT_HANDLER_SNAPSHOT.as_bytes()))
        .map_err(|error| {
            WebbyError::unsupported(format!("JavaScript event handler read failed: {error}"))
        })?;
    let Some(json) = value.as_string() else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<EventHandler>>(&json.to_std_string_lossy()).map_err(|error| {
        WebbyError::Parse {
            message: format!("JavaScript event handler JSON parse failed: {error}"),
        }
    })
}

fn read_browser_actions(context: &mut Context) -> WebbyResult<Vec<BrowserAction>> {
    let value = context
        .eval(Source::from_bytes(BROWSER_ACTION_SNAPSHOT.as_bytes()))
        .map_err(|error| {
            WebbyError::unsupported(format!("JavaScript browser action read failed: {error}"))
        })?;
    let Some(json) = value.as_string() else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<BrowserAction>>(&json.to_std_string_lossy()).map_err(|error| {
        WebbyError::Parse {
            message: format!("JavaScript browser action JSON parse failed: {error}"),
        }
    })
}

fn read_storage_actions(context: &mut Context) -> WebbyResult<Vec<StorageAction>> {
    let value = context
        .eval(Source::from_bytes(STORAGE_ACTION_SNAPSHOT.as_bytes()))
        .map_err(|error| {
            WebbyError::unsupported(format!("JavaScript storage action read failed: {error}"))
        })?;
    let Some(json) = value.as_string() else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<StorageAction>>(&json.to_std_string_lossy()).map_err(|error| {
        WebbyError::Parse {
            message: format!("JavaScript storage action JSON parse failed: {error}"),
        }
    })
}

fn read_bool(context: &mut Context, name: &str) -> WebbyResult<bool> {
    let value = context
        .eval(Source::from_bytes(name.as_bytes()))
        .map_err(|error| {
            WebbyError::unsupported(format!("JavaScript boolean read failed: {error}"))
        })?;
    Ok(value.as_boolean().unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::{
        BrowserAction, BrowserApiContext, DirtyState, DomGeometry, EventDispatch, ExecutionOptions,
        FetchResource, ScriptSource, StorageAction, StorageArea, StorageEntry,
        collect_static_request_urls, dispatch_event_with_dom, execute_scripts,
        execute_scripts_with_dom, execute_scripts_with_dom_and_browser,
    };
    use webby_dom::NodeKind;

    #[test]
    fn console_log_is_captured_deterministically() -> webby_core::WebbyResult<()> {
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "console.log('hello', 42); console.log('again');",
        )];

        let first = execute_scripts(&scripts, ExecutionOptions::default())?;
        let second = execute_scripts(&scripts, ExecutionOptions::default())?;

        assert_eq!(first, second);
        assert_eq!(
            first.diagnostics,
            vec![
                "JavaScript console: hello 42".to_string(),
                "JavaScript console: again".to_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn syntax_and_runtime_errors_become_diagnostics() -> webby_core::WebbyResult<()> {
        let scripts = vec![
            ScriptSource::new("inline script 1", "var = ;"),
            ScriptSource::new("inline script 2", "document.querySelector('p');"),
        ];

        let report = execute_scripts(&scripts, ExecutionOptions::default())?;

        assert_eq!(report.diagnostics.len(), 2);
        assert!(report.diagnostics[0].contains("JavaScript error in inline script 1"));
        assert!(report.diagnostics[1].contains("JavaScript error in inline script 2"));
        Ok(())
    }

    #[test]
    fn disabled_javascript_skips_execution() -> webby_core::WebbyResult<()> {
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "console.log('hidden');",
        )];
        let report = execute_scripts(
            &scripts,
            ExecutionOptions {
                enabled: false,
                ..ExecutionOptions::default()
            },
        )?;

        assert_eq!(
            report.diagnostics,
            vec!["JavaScript disabled by configuration".to_string()]
        );
        Ok(())
    }

    #[test]
    fn script_limits_are_diagnostic_not_panic() -> webby_core::WebbyResult<()> {
        let scripts = vec![
            ScriptSource::new("inline script 1", "console.log('first');"),
            ScriptSource::new("inline script 2", "console.log('second');"),
        ];
        let report = execute_scripts(
            &scripts,
            ExecutionOptions {
                max_scripts: 1,
                max_script_bytes: 8,
                ..ExecutionOptions::default()
            },
        )?;

        assert!(report.diagnostics[0].contains("JavaScript skipped inline script 1"));
        assert!(report.diagnostics[1].contains("JavaScript skipped inline script 2"));
        Ok(())
    }

    #[test]
    fn loop_runtime_limit_becomes_diagnostic() -> webby_core::WebbyResult<()> {
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "while (true) { var keepGoing = true; }",
        )];
        let report = execute_scripts(
            &scripts,
            ExecutionOptions {
                max_loop_iterations: 8,
                ..ExecutionOptions::default()
            },
        )?;

        assert_eq!(report.diagnostics.len(), 1);
        assert!(report.diagnostics[0].contains("JavaScript error in inline script 1"));
        Ok(())
    }

    #[test]
    fn dom_text_content_mutation_updates_document() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p id=\"intro\">Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.getElementById('intro').textContent = 'New text';",
        )];

        execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert_eq!(webby_html::extract_visible_text(&document), "New text");
        Ok(())
    }

    #[test]
    fn dom_create_append_and_attribute_mutations_update_document() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><main id=\"app\"></main></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var item = document.createElement('p'); item.className = 'note'; item.appendChild(document.createTextNode('Added')); document.getElementById('app').appendChild(item); console.log(item.getAttribute('class'));",
        )];

        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        let Some(main) = document.find_node(2) else {
            return Err(webby_core::WebbyError::invalid_input("missing main"));
        };
        assert_eq!(main.children.len(), 1);
        assert!(matches!(
            &main.children[0].kind,
            NodeKind::Element(element)
                if element.tag_name == "p"
                    && element.attributes.get("class").map(String::as_str) == Some("note")
        ));
        assert!(matches!(
            &main.children[0].children[0].kind,
            NodeKind::Text(text) if text == "Added"
        ));
        assert!(
            report
                .diagnostics
                .contains(&"JavaScript console: note".to_string())
        );
        Ok(())
    }

    #[test]
    fn query_selector_can_target_supported_simple_selector() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<body><p class=\"note\" type=\"text\">Old</p><p>Other</p></body>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.querySelector('p.note').textContent = 'Hit'; document.querySelector('p[type=\"text\"]').setAttribute('name', 'q');",
        )];

        execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert_eq!(webby_html::extract_visible_text(&document), "Hit\n\nOther");
        let Some(first_paragraph) = document.find_node(2) else {
            return Err(webby_core::WebbyError::invalid_input("missing paragraph"));
        };
        assert!(matches!(
            &first_paragraph.kind,
            NodeKind::Element(element)
                if element.attributes.get("name").map(String::as_str) == Some("q")
        ));
        Ok(())
    }

    #[test]
    fn attach_shadow_renders_shadow_content_instead_of_light_dom() -> webby_core::WebbyResult<()> {
        let mut document =
            webby_html::parse_document("<body><x-card id=\"card\">Light fallback</x-card></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var root = document.getElementById('card').attachShadow({ mode: 'open' });\
             var p = document.createElement('p');\
             p.textContent = 'Shadow content';\
             root.appendChild(p);",
        )];

        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert!(report.diagnostics.is_empty());
        assert_eq!(
            webby_html::extract_visible_text(&document),
            "Shadow content"
        );
        assert!(webby_html::dump_dom(&document).contains("#shadow-root"));
        Ok(())
    }

    #[test]
    fn custom_element_connected_callback_can_attach_shadow() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><x-card></x-card></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "function Card() {}\
             Card.prototype.connectedCallback = function() {\
               var root = this.attachShadow({ mode: 'open' });\
               var span = document.createElement('span');\
               span.textContent = 'Upgraded';\
               root.appendChild(span);\
             };\
             customElements.define('x-card', Card);",
        )];

        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert!(report.diagnostics.is_empty());
        assert_eq!(webby_html::extract_visible_text(&document), "Upgraded");
        Ok(())
    }

    #[test]
    fn unsupported_shadow_mode_is_diagnostic_not_panic() -> webby_core::WebbyResult<()> {
        let mut document =
            webby_html::parse_document("<body><x-card id=\"card\"></x-card></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.getElementById('card').attachShadow({ mode: 'closed' });",
        )];

        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("JavaScript Shadow DOM unsupported attachShadow mode")
        }));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("JavaScript error in inline script 1"))
        );
        Ok(())
    }

    #[test]
    fn moving_shadow_child_detaches_from_shadow_root() -> webby_core::WebbyResult<()> {
        let mut document =
            webby_html::parse_document("<body><x-card id=\"card\"></x-card></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var root = document.getElementById('card').attachShadow({ mode: 'open' });\
             var p = document.createElement('p');\
             p.textContent = 'Moved';\
             root.appendChild(p);\
             document.body.appendChild(p);",
        )];

        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert!(report.diagnostics.is_empty());
        assert_eq!(webby_html::extract_visible_text(&document), "Moved");
        let body = &document.root.children[0];
        assert_eq!(body.children.len(), 2);
        assert!(body.children[0].shadow_children.is_empty());
        Ok(())
    }

    #[test]
    fn query_selector_can_target_supported_complex_selectors() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<body><section class=\"card\"><div><p class=\"note highlighted\">Old</p></div><button name=\"q\">Button</button><h2>Title</h2></section></body>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.querySelector('.card p.note.highlighted').textContent = 'Descendant';\
             document.querySelector('.card > button[name=\"q\"]').textContent = 'Child';\
             document.querySelector('h1, h2').setAttribute('id', 'heading');",
        )];

        execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert_eq!(
            webby_html::extract_visible_text(&document),
            "Descendant\n\nChild\n\nTitle"
        );
        assert!(
            document
                .find_node(8)
                .and_then(|node| match &node.kind {
                    NodeKind::Element(element) => element.attributes.get("id"),
                    NodeKind::Document | NodeKind::Text(_) => None,
                })
                .is_some_and(|id| id == "heading")
        );
        Ok(())
    }

    #[test]
    fn unsupported_query_selector_becomes_script_diagnostic() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><div><p>Text</p></div></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.querySelector('div + p').textContent = 'Nope';",
        )];

        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert_eq!(report.diagnostics.len(), 1);
        assert!(report.diagnostics[0].contains("JavaScript error in inline script 1"));
        Ok(())
    }

    #[test]
    fn class_list_dataset_and_style_mutations_update_attributes() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<body><p id=\"item\" class=\"old\" data-count=\"1\">Text</p></body>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var item = document.getElementById('item');\
             item.classList.add('new');\
             item.classList.remove('old');\
             item.dataset.count = '2';\
             item.style.color = 'red';\
             console.log(item.classList.contains('new'), item.dataset.count, item.style.color);",
        )];

        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        let Some(paragraph) = document.find_node(2) else {
            return Err(webby_core::WebbyError::invalid_input("missing paragraph"));
        };
        assert!(matches!(
            &paragraph.kind,
            NodeKind::Element(element)
                if element.attributes.get("class").map(String::as_str) == Some("new")
                    && element.attributes.get("data-count").map(String::as_str) == Some("2")
                    && element.attributes.get("style").is_some_and(|style| style.contains("color: red"))
        ));
        assert!(report.dirty.style);
        assert!(report.dirty.layout);
        assert!(
            report
                .diagnostics
                .contains(&"JavaScript console: true 2 red".to_string())
        );
        Ok(())
    }

    #[test]
    fn traversal_properties_and_query_selector_all_are_deterministic() -> webby_core::WebbyResult<()>
    {
        let mut document = webby_html::parse_document(
            "<body><main id=\"root\"><p class=\"note\">One</p><span>Two</span><p class=\"note\">Three</p></main></body>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var root = document.getElementById('root');\
             var notes = root.querySelectorAll('p.note');\
             console.log(root.children.length, root.childNodes.length, root.firstChild.textContent, root.lastChild.textContent, notes.length);\
             notes[1].previousSibling.textContent = 'Middle';\
             console.log(notes[0].parentNode.id, notes[1].matches('.note'), notes[1].closest('main').id);",
        )];

        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert_eq!(
            webby_html::extract_visible_text(&document),
            "One\n\nMiddle\n\nThree"
        );
        assert!(
            report
                .diagnostics
                .contains(&"JavaScript console: 3 3 One Three 2".to_string())
        );
        assert!(
            report
                .diagnostics
                .contains(&"JavaScript console: root true root".to_string())
        );
        Ok(())
    }

    #[test]
    fn document_window_and_geometry_apis_use_browser_context() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<html><head><title>Demo</title></head><body><p id=\"box\">Box</p></body></html>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var rect = document.getElementById('box').getBoundingClientRect();\
             console.log(document.title, document.body.nodeName, document.documentElement.nodeName, window.innerWidth, window.innerHeight, rect.left, rect.top, rect.width, rect.height, document.body.clientWidth);",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                viewport_width: 320,
                viewport_height: 200,
                geometry: vec![DomGeometry {
                    node_id: "6".to_string(),
                    x: 10.0,
                    y: 20.0,
                    width: 80.0,
                    height: 30.0,
                }],
                ..BrowserApiContext::default()
            },
        )?;

        assert_eq!(
            report.diagnostics,
            vec!["JavaScript console: Demo BODY HTML 320 200 10 20 80 30 320".to_string()]
        );
        Ok(())
    }

    #[test]
    fn click_event_bubbles_and_can_mutate_dom() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<body><div id=\"outer\"><button id=\"target\">Old</button></div></body>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.getElementById('outer').addEventListener('click', function(event) { console.log(event.type, event.target.id, event.currentTarget.id); });\
             document.getElementById('target').onclick = function(event) { this.textContent = 'Clicked'; event.preventDefault(); };",
        )];
        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        assert_eq!(report.event_handlers.len(), 2);
        let event_report = dispatch_event_with_dom(
            &mut document,
            &report.event_handlers,
            EventDispatch {
                event_type: "click".to_string(),
                target_node_id: 3,
            },
            ExecutionOptions::default(),
        )?;

        assert!(event_report.default_prevented);
        assert_eq!(webby_html::extract_visible_text(&document), "Clicked");
        assert!(
            event_report
                .diagnostics
                .contains(&"JavaScript console: click target outer".to_string())
        );
        Ok(())
    }

    #[test]
    fn location_and_history_apis_report_browser_actions() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p>Home</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "console.log(location.href); location.assign('/next'); history.back(); history.forward();",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                current_url: "https://example.test/home".to_string(),
                fetch_resources: Vec::new(),
                ..BrowserApiContext::default()
            },
        )?;

        assert_eq!(
            report.diagnostics,
            vec!["JavaScript console: https://example.test/home".to_string()]
        );
        assert_eq!(
            report.browser_actions,
            vec![
                BrowserAction::Navigate {
                    url: "/next".to_string(),
                    replace: false
                },
                BrowserAction::HistoryBack,
                BrowserAction::HistoryForward
            ]
        );
        Ok(())
    }

    #[test]
    fn timeout_apis_report_deterministic_actions() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p>Home</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var timer = setTimeout(function() { document.querySelector('p').textContent = 'Timer'; }, 25); clearTimeout(timer);",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                current_url: "https://example.test/home".to_string(),
                fetch_resources: Vec::new(),
                ..BrowserApiContext::default()
            },
        )?;

        assert_eq!(report.browser_actions.len(), 2);
        assert!(matches!(
            report.browser_actions.first(),
            Some(BrowserAction::SetTimeout {
                id: 1,
                delay_ms: 25,
                ..
            })
        ));
        assert_eq!(
            report.browser_actions.get(1),
            Some(&BrowserAction::ClearTimeout { id: 1 })
        );
        Ok(())
    }

    #[test]
    fn fetch_shim_exposes_text_status_ok_and_url() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p id=\"out\">Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var response = fetch('/data.txt'); console.log(response.status, response.ok, response.url); document.getElementById('out').textContent = response.text();",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                current_url: "https://example.test/home".to_string(),
                fetch_resources: vec![FetchResource {
                    request_url: "/data.txt".to_string(),
                    url: "https://example.test/data.txt".to_string(),
                    status: 200,
                    body: "Fetched text".to_string(),
                    error: None,
                }],
                ..BrowserApiContext::default()
            },
        )?;

        assert_eq!(
            report.diagnostics,
            vec!["JavaScript console: 200 true https://example.test/data.txt".to_string()]
        );
        assert_eq!(webby_html::extract_visible_text(&document), "Fetched text");
        Ok(())
    }

    #[test]
    fn xhr_shim_loads_text_and_runs_onload() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p id=\"out\">Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var xhr = new XMLHttpRequest(); xhr.onload = function() { document.getElementById('out').textContent = xhr.status + ':' + xhr.responseText; }; xhr.open('GET', '/data.txt'); xhr.send();",
        )];

        execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                current_url: "https://example.test/home".to_string(),
                fetch_resources: vec![FetchResource {
                    request_url: "/data.txt".to_string(),
                    url: "https://example.test/data.txt".to_string(),
                    status: 200,
                    body: "XHR text".to_string(),
                    error: None,
                }],
                ..BrowserApiContext::default()
            },
        )?;

        assert_eq!(webby_html::extract_visible_text(&document), "200:XHR text");
        Ok(())
    }

    #[test]
    fn xhr_rejects_unsupported_methods_with_diagnostic() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p>Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var xhr = new XMLHttpRequest(); xhr.open('POST', '/data.txt'); xhr.send();",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                current_url: "https://example.test/home".to_string(),
                fetch_resources: vec![FetchResource {
                    request_url: "/data.txt".to_string(),
                    url: "https://example.test/data.txt".to_string(),
                    status: 200,
                    body: "XHR text".to_string(),
                    error: None,
                }],
                ..BrowserApiContext::default()
            },
        )?;

        assert!(
            report
                .diagnostics
                .iter()
                .any(|item| item.contains("XMLHttpRequest only supports GET"))
        );
        Ok(())
    }

    #[test]
    fn failed_fetch_is_a_controlled_javascript_diagnostic() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p>Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "fetch('/missing.txt').text();",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                current_url: "https://example.test/home".to_string(),
                fetch_resources: Vec::new(),
                ..BrowserApiContext::default()
            },
        )?;

        assert!(
            report
                .diagnostics
                .iter()
                .any(|item| item.contains("fetch resource unavailable"))
        );
        Ok(())
    }

    #[test]
    fn web_storage_set_get_remove_clear_key_and_length_work() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p id=\"out\">Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "localStorage.setItem('b', 2);
             localStorage.setItem('a', 'one');
             var before = localStorage.length + ':' + localStorage.key(0) + ':' + localStorage.getItem('a');
             localStorage.removeItem('b');
             sessionStorage.setItem('temp', 'yes');
             var session = sessionStorage.getItem('temp');
             localStorage.clear();
             document.getElementById('out').textContent = before + ':' + localStorage.length + ':' + session;",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext::default(),
        )?;

        assert_eq!(webby_html::extract_visible_text(&document), "2:a:one:0:yes");
        assert_eq!(
            report.storage_actions,
            vec![
                StorageAction::SetItem {
                    area: StorageArea::Local,
                    key: "b".to_string(),
                    value: "2".to_string()
                },
                StorageAction::SetItem {
                    area: StorageArea::Local,
                    key: "a".to_string(),
                    value: "one".to_string()
                },
                StorageAction::RemoveItem {
                    area: StorageArea::Local,
                    key: "b".to_string()
                },
                StorageAction::SetItem {
                    area: StorageArea::Session,
                    key: "temp".to_string(),
                    value: "yes".to_string()
                },
                StorageAction::Clear {
                    area: StorageArea::Local
                }
            ]
        );
        Ok(())
    }

    #[test]
    fn web_storage_reads_initial_snapshots_and_drives_dom_updates() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p id=\"out\">Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.getElementById('out').textContent = localStorage.getItem('saved') + '/' + sessionStorage.getItem('tab');",
        )];

        execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                local_storage: vec![StorageEntry {
                    key: "saved".to_string(),
                    value: "local".to_string(),
                }],
                session_storage: vec![StorageEntry {
                    key: "tab".to_string(),
                    value: "session".to_string(),
                }],
                ..BrowserApiContext::default()
            },
        )?;

        assert_eq!(webby_html::extract_visible_text(&document), "local/session");
        Ok(())
    }

    #[test]
    fn web_storage_quota_overflow_is_controlled_diagnostic() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p>Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "localStorage.setItem('large', 'abcdef');",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                storage_quota_bytes: 4,
                ..BrowserApiContext::default()
            },
        )?;

        assert!(report.storage_actions.is_empty());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|item| item.contains("Web Storage quota exceeded"))
        );
        Ok(())
    }

    #[test]
    fn web_storage_quota_counts_utf8_bytes() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p>Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "localStorage.setItem('k', 'ååå');",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                storage_quota_bytes: 6,
                ..BrowserApiContext::default()
            },
        )?;

        assert!(report.storage_actions.is_empty());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|item| item.contains("Web Storage quota exceeded"))
        );
        Ok(())
    }

    #[test]
    fn disabled_web_storage_is_controlled_diagnostic() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document("<body><p>Old</p></body>")?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "sessionStorage.setItem('key', 'value');",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext {
                storage_enabled: false,
                ..BrowserApiContext::default()
            },
        )?;

        assert!(report.storage_actions.is_empty());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|item| item.contains("Web Storage disabled by configuration"))
        );
        Ok(())
    }

    #[test]
    fn canvas_2d_api_records_deterministic_dom_attribute() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<body><canvas id=\"c\" width=\"20\" height=\"10\"></canvas></body>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "var ctx = document.getElementById('c').getContext('2d');
             ctx.fillStyle = 'red';
             ctx.fillRect(1, 2, 3, 4);
             ctx.strokeStyle = '#00f';
             ctx.lineWidth = 2;
             ctx.strokeRect(5, 6, 7, 8);
             ctx.clearRect(1, 2, 1, 1);",
        )];

        execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext::default(),
        )?;

        let Some(canvas) = document.find_node(2) else {
            return Err(webby_core::WebbyError::invalid_input("missing canvas"));
        };
        assert!(matches!(
            &canvas.kind,
            NodeKind::Element(element)
                if element.attributes.get("data-webby-canvas").map(String::as_str)
                    == Some("fillRect,1,2,3,4,red;strokeRect,5,6,7,8,#00f,2;clearRect,1,2,1,1")
        ));
        Ok(())
    }

    #[test]
    fn canvas_context_preserves_existing_page_command_state() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<body><canvas id=\"c\" width=\"20\" height=\"10\"></canvas></body>",
        )?;
        let scripts = vec![
            ScriptSource::new(
                "inline script 1",
                "var first = document.getElementById('c').getContext('2d'); first.fillStyle = 'red'; first.fillRect(0, 0, 1, 1);",
            ),
            ScriptSource::new(
                "inline script 2",
                "var second = document.getElementById('c').getContext('2d'); second.fillStyle = 'blue'; second.fillRect(1, 0, 1, 1);",
            ),
        ];

        execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext::default(),
        )?;

        let Some(canvas) = document.find_node(2) else {
            return Err(webby_core::WebbyError::invalid_input("missing canvas"));
        };
        assert!(matches!(
            &canvas.kind,
            NodeKind::Element(element)
                if element.attributes.get("data-webby-canvas").map(String::as_str)
                    == Some("fillRect,0,0,1,1,red;fillRect,1,0,1,1,blue")
        ));
        Ok(())
    }

    #[test]
    fn canvas_invalid_operations_are_controlled_diagnostics() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<body><canvas id=\"c\" width=\"20\" height=\"10\"></canvas></body>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.getElementById('c').getContext('webgl');",
        )];

        let report = execute_scripts_with_dom_and_browser(
            &mut document,
            &scripts,
            ExecutionOptions::default(),
            &BrowserApiContext::default(),
        )?;

        assert!(
            report
                .diagnostics
                .iter()
                .any(|item| item.contains("canvas only supports 2d context"))
        );
        Ok(())
    }

    #[test]
    fn static_request_url_collection_is_deterministic() {
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "fetch('/b.txt'); fetch('/a.txt'); xhr.open('GET', '/c.txt');",
        )];

        assert_eq!(
            collect_static_request_urls(&scripts),
            vec![
                "/a.txt".to_string(),
                "/b.txt".to_string(),
                "/c.txt".to_string()
            ]
        );
    }

    #[test]
    fn text_and_attribute_mutations_report_dirty_states() -> webby_core::WebbyResult<()> {
        let mut text_document = webby_html::parse_document("<body><p id=\"out\">Old</p></body>")?;
        let text_report = execute_scripts_with_dom(
            &mut text_document,
            &[ScriptSource::new(
                "inline script 1",
                "document.getElementById('out').textContent = 'New';",
            )],
            ExecutionOptions::default(),
        )?;
        assert_eq!(
            text_report.dirty,
            DirtyState {
                dom: true,
                style: false,
                layout: true,
                render: true
            }
        );

        let mut style_document = webby_html::parse_document("<body><p id=\"out\">Old</p></body>")?;
        let style_report = execute_scripts_with_dom(
            &mut style_document,
            &[ScriptSource::new(
                "inline script 1",
                "document.getElementById('out').setAttribute('class', 'hot');",
            )],
            ExecutionOptions::default(),
        )?;
        assert_eq!(style_report.dirty, DirtyState::all());
        Ok(())
    }

    #[test]
    fn input_event_handlers_can_mutate_dom() -> webby_core::WebbyResult<()> {
        let mut document = webby_html::parse_document(
            "<body><p id=\"out\">Old</p><form id=\"f\"><input id=\"q\"></form></body>",
        )?;
        let scripts = vec![ScriptSource::new(
            "inline script 1",
            "document.getElementById('f').addEventListener('input', function(event) { document.getElementById('out').textContent = 'Input'; });",
        )];
        let report =
            execute_scripts_with_dom(&mut document, &scripts, ExecutionOptions::default())?;

        let event_report = dispatch_event_with_dom(
            &mut document,
            &report.event_handlers,
            EventDispatch {
                event_type: "input".to_string(),
                target_node_id: 4,
            },
            ExecutionOptions::default(),
        )?;

        assert!(!event_report.default_prevented);
        assert_eq!(webby_html::extract_visible_text(&document), "Input");
        Ok(())
    }
}
