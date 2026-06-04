//! Script resource coordination for Webby app and CLI pipelines.
//!
//! `webby_script` resolves and loads script resources through caller-provided
//! `webby_net` loaders, builds deterministic classic/module execution order,
//! and returns plain `webby_js::ScriptSource` values for the JavaScript engine.

use std::collections::{BTreeMap, BTreeSet};

use webby_dom::Document;
use webby_html::{ScriptContent, ScriptElement};
use webby_js::ScriptSource;
use webby_net::{ResourceLoader, decode_text_utf8};

/// Maximum JavaScript source bytes retained by script coordination.
pub const MAX_SCRIPT_SOURCE_BYTES: usize = 256 * 1024;

/// Ordered scripts and non-fatal diagnostics for a document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadedScripts {
    /// Script sources in deterministic execution order.
    pub scripts: Vec<ScriptSource>,
    /// Script loading/parsing diagnostics.
    pub diagnostics: Vec<String>,
}

/// Loads and orders document scripts.
///
/// Classic scripts run first with Webby's deterministic blocking/defer/async
/// model. Module scripts run after classic scripts. Static module imports are
/// resolved relative to the importing module URL and evaluated dependency-first.
pub fn load_document_scripts<L: ResourceLoader>(
    document: &Document,
    page_url: &url::Url,
    loader: Option<&L>,
) -> LoadedScripts {
    let mut loader_state = ScriptLoader::new(page_url, loader);
    loader_state.load_document(document);
    loader_state.finish()
}

struct ScriptLoader<'a, L> {
    page_url: &'a url::Url,
    loader: Option<&'a L>,
    blocking: Vec<ScriptSource>,
    deferred: Vec<ScriptSource>,
    async_scripts: Vec<ScriptSource>,
    modules: Vec<ScriptSource>,
    diagnostics: Vec<String>,
    module_cache: BTreeMap<String, Option<Vec<ScriptSource>>>,
    visiting_modules: BTreeSet<String>,
}

impl<'a, L: ResourceLoader> ScriptLoader<'a, L> {
    fn new(page_url: &'a url::Url, loader: Option<&'a L>) -> Self {
        Self {
            page_url,
            loader,
            blocking: Vec::new(),
            deferred: Vec::new(),
            async_scripts: Vec::new(),
            modules: Vec::new(),
            diagnostics: Vec::new(),
            module_cache: BTreeMap::new(),
            visiting_modules: BTreeSet::new(),
        }
    }

    fn load_document(&mut self, document: &Document) {
        for script in webby_html::collect_scripts(document) {
            self.load_element(script);
        }
    }

    fn finish(mut self) -> LoadedScripts {
        self.blocking.extend(self.deferred);
        self.blocking.extend(self.async_scripts);
        self.blocking.extend(self.modules);
        LoadedScripts {
            scripts: self.blocking,
            diagnostics: self.diagnostics,
        }
    }

    fn load_element(&mut self, script: ScriptElement) {
        match script_type(script.script_type.as_deref()) {
            ScriptKind::Classic => self.load_classic(script),
            ScriptKind::Module => self.load_module_element(script),
            ScriptKind::Unsupported(kind) => {
                self.diagnostics.push(format!(
                    "JavaScript skipped {}: unsupported script type {}",
                    script.label, kind
                ));
            }
        }
    }

    fn load_classic(&mut self, script: ScriptElement) {
        let Some(source) = self.classic_source(script) else {
            return;
        };
        if source.async_attr {
            self.async_scripts.push(source.source);
        } else if source.defer {
            self.deferred.push(source.source);
        } else {
            self.blocking.push(source.source);
        }
    }

    fn classic_source(&mut self, script: ScriptElement) -> Option<OrderedScriptSource> {
        match script.content {
            ScriptContent::Inline(code) => {
                let label = script.label;
                if !self.script_source_within_limit(&label, code.len()) {
                    return None;
                }
                Some(OrderedScriptSource {
                    source: ScriptSource::new(label, code),
                    defer: script.defer,
                    async_attr: script.async_attr,
                })
            }
            ScriptContent::External(src) => {
                self.load_external_classic(script.label, src, script.defer, script.async_attr)
            }
        }
    }

    fn load_external_classic(
        &mut self,
        label: String,
        src: String,
        defer: bool,
        async_attr: bool,
    ) -> Option<OrderedScriptSource> {
        let (script_url, code) = self.load_external_script(&label, &src)?;
        Some(OrderedScriptSource {
            source: ScriptSource::new(label_for_url(label, &script_url), code),
            defer,
            async_attr,
        })
    }

    fn load_module_element(&mut self, script: ScriptElement) {
        match script.content {
            ScriptContent::Inline(code) => {
                let label = script.label;
                if !self.script_source_within_limit(&label, code.len()) {
                    return;
                }
                let sources = self.module_sources(label, self.page_url, code);
                self.modules.extend(sources);
            }
            ScriptContent::External(src) => {
                let Some(script_url) = self.resolve_script_url(&script.label, &src) else {
                    return;
                };
                let sources = self.load_module_url(&script_url);
                self.modules.extend(sources);
            }
        }
    }

    fn module_sources(
        &mut self,
        label: String,
        base_url: &url::Url,
        code: String,
    ) -> Vec<ScriptSource> {
        let parsed = parse_module(&label, &code, &mut self.diagnostics);
        if !parsed.supported {
            return Vec::new();
        }
        let mut output = Vec::new();
        for import in parsed.imports {
            if let Ok(import_url) = base_url.join(&import) {
                output.extend(self.load_module_url(&import_url));
            } else {
                self.diagnostics.push(format!(
                    "JavaScript module {label} skipped invalid import {import}"
                ));
            }
        }
        output.push(ScriptSource::new(label, parsed.code));
        output
    }

    fn load_module_url(&mut self, module_url: &url::Url) -> Vec<ScriptSource> {
        let key = module_url.to_string();
        if let Some(cached) = self.module_cache.get(&key) {
            return cached.clone().unwrap_or_default();
        }
        if !self.visiting_modules.insert(key.clone()) {
            self.diagnostics
                .push(format!("JavaScript module cycle detected at {key}"));
            return Vec::new();
        }
        let Some(loader) = self.loader else {
            self.diagnostics.push(format!(
                "JavaScript module {key} skipped: external module loading requires a resource loader"
            ));
            self.visiting_modules.remove(&key);
            self.module_cache.insert(key, None);
            return Vec::new();
        };
        let result = loader.load(module_url);
        let sources = match result {
            Ok(response) => {
                if !self.script_source_within_limit(&key, response.bytes.len()) {
                    Vec::new()
                } else {
                    let code = decode_text_utf8(&response.bytes);
                    self.module_sources(
                        format!("module {}", response.final_url),
                        &response.final_url,
                        code,
                    )
                }
            }
            Err(error) => {
                self.diagnostics.push(format!(
                    "JavaScript module {key} could not be loaded: {error}"
                ));
                Vec::new()
            }
        };
        self.visiting_modules.remove(&key);
        self.module_cache.insert(key, Some(sources.clone()));
        sources
    }

    fn load_external_script(&mut self, label: &str, src: &str) -> Option<(url::Url, String)> {
        let Some(loader) = self.loader else {
            self.diagnostics.push(format!(
                "JavaScript skipped {label} {src}: external script loading requires a resource loader"
            ));
            return None;
        };
        let script_url = self.resolve_script_url(label, src)?;
        match loader.load(&script_url) {
            Ok(response) => {
                if !self.script_source_within_limit(script_url.as_ref(), response.bytes.len()) {
                    return None;
                }
                Some((response.final_url, decode_text_utf8(&response.bytes)))
            }
            Err(error) => {
                self.diagnostics.push(format!(
                    "JavaScript skipped {label} {}: failed to load script: {error}",
                    script_url
                ));
                None
            }
        }
    }

    fn script_source_within_limit(&mut self, label: &str, byte_len: usize) -> bool {
        if byte_len <= MAX_SCRIPT_SOURCE_BYTES {
            return true;
        }
        self.diagnostics.push(format!(
            "JavaScript skipped {label}: script is {byte_len} bytes, limit is {MAX_SCRIPT_SOURCE_BYTES} bytes"
        ));
        false
    }

    fn resolve_script_url(&mut self, label: &str, src: &str) -> Option<url::Url> {
        match self.page_url.join(src) {
            Ok(script_url) => Some(script_url),
            Err(_) => {
                self.diagnostics.push(format!(
                    "JavaScript skipped {label} {src}: invalid script URL"
                ));
                None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderedScriptSource {
    source: ScriptSource,
    defer: bool,
    async_attr: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScriptKind {
    Classic,
    Module,
    Unsupported(String),
}

fn script_type(script_type: Option<&str>) -> ScriptKind {
    match script_type.map(str::trim).filter(|value| !value.is_empty()) {
        None => ScriptKind::Classic,
        Some(value) if value.eq_ignore_ascii_case("text/javascript") => ScriptKind::Classic,
        Some(value) if value.eq_ignore_ascii_case("module") => ScriptKind::Module,
        Some(value) => ScriptKind::Unsupported(value.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ParsedModule {
    imports: Vec<String>,
    code: String,
    supported: bool,
}

fn parse_module(label: &str, code: &str, diagnostics: &mut Vec<String>) -> ParsedModule {
    let mut parsed = ParsedModule {
        supported: true,
        ..ParsedModule::default()
    };
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import(") {
            diagnostics.push(format!(
                "JavaScript module {label} skipped unsupported dynamic import"
            ));
            parsed.supported = false;
            return parsed;
        }
        if trimmed.starts_with("import") {
            match static_import_target(trimmed) {
                Some(target) => parsed.imports.push(target),
                None => {
                    diagnostics.push(format!(
                        "JavaScript module {label} skipped unsupported import syntax"
                    ));
                    parsed.supported = false;
                    return parsed;
                }
            }
            continue;
        }
        parsed.code.push_str(&strip_export(trimmed));
        parsed.code.push('\n');
    }
    parsed
}

fn static_import_target(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("import ") {
        let trimmed = rest.trim().trim_end_matches(';').trim();
        if quoted(trimmed).is_some() {
            return quoted(trimmed);
        }
        if let Some((_, target)) = trimmed.rsplit_once(" from ") {
            return quoted(target.trim());
        }
    }
    None
}

fn quoted(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let first = trimmed.chars().next()?;
    if first != '\'' && first != '"' {
        return None;
    }
    let tail = &trimmed[first.len_utf8()..];
    let end = tail.find(first)?;
    Some(tail[..end].to_string())
}

fn strip_export(line: &str) -> String {
    if let Some(rest) = line.strip_prefix("export default ") {
        rest.to_string()
    } else if let Some(rest) = line.strip_prefix("export ") {
        rest.to_string()
    } else {
        line.to_string()
    }
}

fn label_for_url(label: String, url: &url::Url) -> String {
    if label.starts_with("external script") {
        format!("{label} {url}")
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use super::load_document_scripts;
    use webby_core::{WebbyError, WebbyResult};
    use webby_net::{ResourceLoader, ResourceResponse};

    #[test]
    fn loads_classic_and_module_scripts_in_deterministic_order() -> WebbyResult<()> {
        let page = url::Url::parse("https://example.test/page").map_err(url_error)?;
        let dep = url::Url::parse("https://example.test/dep.js").map_err(url_error)?;
        let main = url::Url::parse("https://example.test/main.js").map_err(url_error)?;
        let loader = TestLoader::new([
            (
                dep.as_str(),
                "document.getElementById('out').textContent += ' dep';",
            ),
            (
                main.as_str(),
                "import './dep.js';\ndocument.getElementById('out').textContent += ' main';",
            ),
        ]);
        let document = webby_html::parse_document(
            "<script>var order = 'classic';</script><script type=\"module\" src=\"main.js\"></script>",
        )?;

        let loaded = load_document_scripts(&document, &page, Some(&loader));

        assert_eq!(loaded.diagnostics, Vec::<String>::new());
        assert_eq!(loaded.scripts.len(), 3);
        assert_eq!(loaded.scripts[0].label, "inline script 1");
        assert!(loaded.scripts[1].label.contains("dep.js"));
        assert!(loaded.scripts[2].label.contains("main.js"));
        Ok(())
    }

    #[test]
    fn module_cycle_is_diagnostic_and_does_not_loop() -> WebbyResult<()> {
        let page = url::Url::parse("https://example.test/page").map_err(url_error)?;
        let a = url::Url::parse("https://example.test/a.js").map_err(url_error)?;
        let b = url::Url::parse("https://example.test/b.js").map_err(url_error)?;
        let loader = TestLoader::new([
            (a.as_str(), "import './b.js';\nconsole.log('a');"),
            (b.as_str(), "import './a.js';\nconsole.log('b');"),
        ]);
        let document =
            webby_html::parse_document("<script type=\"module\" src=\"a.js\"></script>")?;

        let loaded = load_document_scripts(&document, &page, Some(&loader));

        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("module cycle detected"))
        );
        assert_eq!(loaded.scripts.len(), 2);
        Ok(())
    }

    #[test]
    fn unsupported_import_syntax_is_diagnostic() -> WebbyResult<()> {
        let page = url::Url::parse("https://example.test/page").map_err(url_error)?;
        let document = webby_html::parse_document(
            "<script type=\"module\">import value from source; console.log('skip');</script>",
        )?;

        let loaded = load_document_scripts::<TestLoader>(&document, &page, None);

        assert!(loaded.scripts.is_empty());
        assert_eq!(
            loaded.diagnostics,
            vec!["JavaScript module inline script 1 skipped unsupported import syntax".to_string()]
        );
        Ok(())
    }

    #[test]
    fn oversized_inline_script_is_skipped_with_diagnostic() -> WebbyResult<()> {
        let page = url::Url::parse("https://example.test/page").map_err(url_error)?;
        let code = "a".repeat(super::MAX_SCRIPT_SOURCE_BYTES + 1);
        let document = webby_html::parse_document(&format!("<script>{code}</script>"))?;

        let loaded = load_document_scripts::<TestLoader>(&document, &page, None);

        assert!(loaded.scripts.is_empty());
        assert_eq!(
            loaded.diagnostics,
            vec![format!(
                "JavaScript skipped inline script 1: script is {} bytes, limit is {} bytes",
                super::MAX_SCRIPT_SOURCE_BYTES + 1,
                super::MAX_SCRIPT_SOURCE_BYTES
            )]
        );
        Ok(())
    }

    #[test]
    fn oversized_external_script_is_skipped_with_diagnostic() -> WebbyResult<()> {
        let page = url::Url::parse("https://example.test/page").map_err(url_error)?;
        let script_url = url::Url::parse("https://example.test/big.js").map_err(url_error)?;
        let code = "a".repeat(super::MAX_SCRIPT_SOURCE_BYTES + 1);
        let loader = TestLoader::new([(script_url.as_str(), code.as_str())]);
        let document = webby_html::parse_document("<script src=\"big.js\"></script>")?;

        let loaded = load_document_scripts(&document, &page, Some(&loader));

        assert!(loaded.scripts.is_empty());
        assert_eq!(
            loaded.diagnostics,
            vec![format!(
                "JavaScript skipped {}: script is {} bytes, limit is {} bytes",
                script_url,
                super::MAX_SCRIPT_SOURCE_BYTES + 1,
                super::MAX_SCRIPT_SOURCE_BYTES
            )]
        );
        Ok(())
    }

    #[derive(Debug)]
    struct TestLoader {
        resources: Vec<(String, String)>,
    }

    impl TestLoader {
        fn new<const N: usize>(resources: [(&str, &str); N]) -> Self {
            Self {
                resources: resources
                    .into_iter()
                    .map(|(url, body)| (url.to_string(), body.to_string()))
                    .collect(),
            }
        }
    }

    impl ResourceLoader for TestLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            for (resource_url, body) in &self.resources {
                if resource_url == url.as_str() {
                    return Ok(ResourceResponse {
                        requested_url: url.clone(),
                        final_url: url.clone(),
                        status: Some(200),
                        content_type: Some("text/javascript".to_string()),
                        headers: Vec::new(),
                        bytes: body.as_bytes().to_vec(),
                    });
                }
            }
            Err(WebbyError::Network {
                message: format!("missing test script {url}"),
            })
        }
    }

    fn url_error(error: url::ParseError) -> WebbyError {
        WebbyError::Url {
            message: error.to_string(),
        }
    }
}
