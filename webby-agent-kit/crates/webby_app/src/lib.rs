//! Testable browser-shell state and page pipeline for Webby's native app.
//!
//! The native window adapter lives in `main.rs`. This library owns browser
//! chrome state, navigation state, scroll state, and composition of the engine
//! pipeline without reimplementing parsing, styling, layout, URL resolution, or
//! rendering algorithms.

use webby_cache::{CacheMode, DiskResourceCache, ResourceCache};
use webby_core::{WebbyError, WebbyResult};
use webby_layout::{
    Dimensions, ElementMetadata, FormControlHitBox, FormControlType, ImageMap, LayoutBox,
    LayoutKind, LayoutTree, LinkHitBox, Rect, ScrollOffsets, Viewport,
};
use webby_net::{
    BasicAuthChallenge, BasicCredentials, DownloadMetadata, NavigationResponseDisposition,
    ResourceLoader, ResourceResponse, basic_auth_header, decode_text,
};
use webby_render::{
    Color, DisplayList, FontWeight, FormControlVisualState, RenderBackend, SoftwareRenderBackend,
    Surface, build_display_list, build_display_list_with_scroll_offsets, draw_text_control_overlay,
    render_with_backend,
};
use webby_state::{Bookmark, BrowserProfile, CookieJar, ProfileStore, StorageState};
use webby_url::{
    FormField, SearchEngine, form_urlencoded_body, resolve_form_action_url, resolve_get_form_url,
    resolve_input,
};

use std::cell::RefCell;
use std::collections::BTreeMap;

/// Height reserved for browser chrome in native-window pixels.
pub const CHROME_HEIGHT: usize = 48;
/// Advisory node-count threshold for large-document pipeline diagnostics.
pub const LARGE_DOCUMENT_NODE_DIAGNOSTIC_THRESHOLD: usize = 1_000;
const TAB_STRIP_HEIGHT: usize = 18;
const CHROME_BUTTON_Y: usize = 22;
const CHROME_BUTTON_SIZE: usize = 20;
const CHROME_BUTTON_GAP: usize = 4;
const ADDRESS_X: usize = 112;
const ADDRESS_Y: usize = 20;
const ADDRESS_HEIGHT: usize = 24;
/// Startup address shown by default. This points to the local fixture page.
pub const STARTUP_ADDRESS: &str = webby_state::DEFAULT_HOMEPAGE;
/// Internal URL used for Webby's generated bookmarks page.
pub const BOOKMARKS_PAGE_URL: &str = "https://webby.local/bookmarks";
/// Default native-window width.
pub const DEFAULT_WINDOW_WIDTH: usize = 900;
/// Default native-window height.
pub const DEFAULT_WINDOW_HEIGHT: usize = 700;

/// Editable top browser chrome state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserChrome {
    /// Current address/search text.
    pub address_input: String,
    /// Whether keyboard input is directed to the address bar.
    pub address_focused: bool,
    /// Whether the next typed character replaces the whole address field.
    pub address_selected: bool,
    /// UTF-8 byte offset for the address insertion cursor.
    pub address_cursor: usize,
}

impl BrowserChrome {
    /// Creates chrome initialized to Webby's documented startup page.
    pub fn new() -> Self {
        Self {
            address_input: STARTUP_ADDRESS.to_string(),
            address_focused: true,
            address_selected: true,
            address_cursor: STARTUP_ADDRESS.len(),
        }
    }

    /// Replaces address/search text.
    pub fn set_address_input(&mut self, input: impl Into<String>) {
        self.address_input = input.into();
        self.address_selected = false;
        self.address_cursor = self.address_input.len();
    }

    /// Focuses the address/search field.
    pub fn focus_address(&mut self, select_all: bool) {
        self.address_focused = true;
        self.address_selected = select_all;
        self.address_cursor = self.address_input.len();
    }

    /// Appends one typed character to the address/search bar.
    pub fn type_character(&mut self, character: char) {
        if !character.is_control() {
            self.address_cursor = floor_char_boundary(&self.address_input, self.address_cursor);
            if self.address_selected {
                self.address_input.clear();
                self.address_selected = false;
                self.address_cursor = 0;
            }
            self.address_input.insert(self.address_cursor, character);
            self.address_cursor = self.address_cursor.saturating_add(character.len_utf8());
        }
    }

    /// Removes the last address/search character, if any.
    pub fn backspace(&mut self) {
        self.address_cursor = floor_char_boundary(&self.address_input, self.address_cursor);
        if self.address_selected {
            self.address_input.clear();
            self.address_selected = false;
            self.address_cursor = 0;
        } else {
            let previous = previous_char_boundary(&self.address_input, self.address_cursor);
            if previous < self.address_cursor {
                self.address_input.drain(previous..self.address_cursor);
                self.address_cursor = previous;
            }
        }
    }

    /// Moves the address insertion cursor one Unicode scalar to the left.
    pub fn move_cursor_left(&mut self) {
        self.address_selected = false;
        self.address_cursor = previous_char_boundary(
            &self.address_input,
            floor_char_boundary(&self.address_input, self.address_cursor),
        );
    }

    /// Moves the address insertion cursor one Unicode scalar to the right.
    pub fn move_cursor_right(&mut self) {
        self.address_selected = false;
        self.address_cursor = next_char_boundary(
            &self.address_input,
            floor_char_boundary(&self.address_input, self.address_cursor),
        );
    }

    /// Moves the address insertion cursor to the beginning.
    pub fn move_cursor_home(&mut self) {
        self.address_selected = false;
        self.address_cursor = 0;
    }

    /// Moves the address insertion cursor to the end.
    pub fn move_cursor_end(&mut self) {
        self.address_selected = false;
        self.address_cursor = self.address_input.len();
    }

    /// Replaces the current selection or inserts clipboard text at the cursor.
    pub fn paste(&mut self, text: &str) {
        let filtered = text
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>();
        if self.address_selected {
            self.address_input.clear();
            self.address_cursor = 0;
            self.address_selected = false;
        }
        self.address_cursor = floor_char_boundary(&self.address_input, self.address_cursor);
        self.address_input
            .insert_str(self.address_cursor, &filtered);
        self.address_cursor = self.address_cursor.saturating_add(filtered.len());
    }

    /// Returns selected address text. Webby's v0.1 selection is select-all.
    pub fn selected_text(&self) -> Option<&str> {
        self.address_selected.then_some(self.address_input.as_str())
    }
}

fn floor_char_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    while !text.is_char_boundary(cursor) {
        cursor = cursor.saturating_sub(1);
    }
    cursor
}

fn previous_char_boundary(text: &str, cursor: usize) -> usize {
    text.get(..cursor.min(text.len()))
        .and_then(|prefix| prefix.char_indices().next_back().map(|(index, _)| index))
        .unwrap_or(0)
}

fn next_char_boundary(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text.get(cursor..)
        .and_then(|suffix| suffix.chars().next().map(char::len_utf8))
        .map(|width| cursor.saturating_add(width))
        .unwrap_or(text.len())
}

impl Default for BrowserChrome {
    fn default() -> Self {
        Self::new()
    }
}

/// Navigation state independent from the chrome text field.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavigationController {
    /// Last successfully loaded URL.
    pub current_url: Option<url::Url>,
    /// Most recent resolved navigation target, including failed loads.
    pub pending_url: Option<url::Url>,
    /// Most recent target that failed to load, render, or resolve.
    pub failed_url: Option<String>,
    /// Successfully committed navigation entries.
    pub history: Vec<url::Url>,
    /// Current index into committed navigation history.
    pub history_index: Option<usize>,
    /// Monotonic token for the most recently started navigation.
    pub generation: u64,
    /// Generation token for the currently committed page.
    pub active_page_generation: Option<u64>,
    /// Testable lifecycle state for the active navigation/page.
    pub lifecycle: NavigationLifecycle,
    /// Deterministic lifecycle diagnostics, including cancellation notes.
    pub lifecycle_diagnostics: Vec<String>,
    pending_history_action: Option<PendingHistoryAction>,
}

/// Explicit app-owned navigation lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavigationLifecycle {
    /// No navigation is currently active.
    #[default]
    Idle,
    /// Address/search input is being resolved.
    Resolving,
    /// Main resource loading has started.
    LoadingMainResource,
    /// Subresource loading is part of the active page pipeline.
    LoadingSubresources,
    /// Script execution is part of the active page pipeline.
    ExecutingScripts,
    /// Layout/display-list/render work is applying the loaded document.
    Rendering,
    /// The page completed successfully.
    Complete,
    /// The active navigation failed.
    Failed,
    /// A previous navigation was cancelled by a newer one.
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingHistoryAction {
    Push,
    Reload,
    Traverse { target_index: usize },
}

/// Page status shown by the browser shell.
#[derive(Debug, Clone, PartialEq)]
pub enum PageStatus {
    /// The app has startup chrome state but has not loaded a page yet.
    Startup,
    /// A navigation is in progress.
    Loading { url: String },
    /// A page has loaded and has renderable content.
    Loaded { url: String },
    /// A readable app error state.
    Error { message: String },
}

/// Browser chrome action identified from a window coordinate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChromeAction {
    /// Select a tab by index.
    SwitchTab(usize),
    /// Close a tab by index.
    CloseTab(usize),
    /// Navigate back in the active tab.
    Back,
    /// Navigate forward in the active tab.
    Forward,
    /// Reload the active tab.
    Reload,
    /// Bookmark the current successful page.
    BookmarkCurrentPage,
    /// Focus the address/search field.
    FocusAddress,
}

/// Hover target used by the native adapter for cursor/status feedback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverTarget {
    /// The pointer is over browser chrome.
    Chrome(ChromeAction),
    /// The pointer is over a page link.
    Link(String),
    /// The pointer is over an interactive form control.
    FormControl(usize),
}

/// Keyboard-focus target inside page content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardFocusTarget {
    /// Link hit region by layout-order index.
    Link { index: usize },
    /// Form control by stable layout control id.
    FormControl { id: usize },
}

/// Basic accessibility role exposed for debugging and keyboard behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibleRole {
    /// Hyperlink.
    Link,
    /// Push button or submit/reset button.
    Button,
    /// Editable text/search/password/email/textarea control.
    Textbox,
    /// Checkbox control.
    Checkbox,
    /// Radio control.
    Radio,
}

/// State metadata for an accessible node.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessibleState {
    /// Whether the control is disabled.
    pub disabled: bool,
    /// Whether a checkbox/radio is checked.
    pub checked: Option<bool>,
    /// Whether this node currently has keyboard focus.
    pub focused: bool,
}

/// Inspectable accessibility metadata derived from the loaded page.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibleNode {
    /// Accessibility role.
    pub role: AccessibleRole,
    /// Deterministic accessible name.
    pub name: String,
    /// Window-independent page rectangle.
    pub rect: Rect,
    /// Source DOM node id, when known.
    pub node_id: Option<webby_dom::NodeId>,
    /// Link href for link nodes.
    pub href: Option<String>,
    /// Form control id for form nodes.
    pub control_id: Option<usize>,
    /// Basic state metadata.
    pub state: AccessibleState,
}

/// Rendered page data held by app state.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPage {
    /// URL that produced this page.
    pub url: url::Url,
    /// Document title shown by browser chrome.
    pub title: String,
    /// First favicon/icon href exposed by the document, if present.
    pub favicon_href: Option<String>,
    /// DOM document after inline scripts and DOM mutations have run.
    pub document: webby_dom::Document,
    /// Event handlers registered by inline scripts.
    pub event_handlers: Vec<webby_js::EventHandler>,
    /// Image resources supplied to layout for this page.
    pub images: ImageMap,
    /// External stylesheet rules supplied to style/cascade for this page.
    pub external_stylesheets: Vec<webby_css::Stylesheet>,
    /// Layout-to-render display list used for the software render.
    pub display_list: DisplayList,
    /// Layout tree used by rendering and debug inspection.
    pub layout: LayoutTree,
    /// Rendered page content surface, excluding browser chrome.
    pub surface: Surface,
    /// Full content height in page coordinates.
    pub content_height: f32,
    /// Link hit rectangles produced by layout for future navigation.
    pub links: Vec<LinkHitBox>,
    /// Form control hit rectangles produced by layout.
    pub form_controls: Vec<FormControlHitBox>,
    /// Nested iframe browsing contexts, positioned in parent page coordinates.
    pub iframes: Vec<IframeContext>,
    /// Non-fatal pipeline diagnostics, including external stylesheet failures.
    pub diagnostics: Vec<String>,
    /// Browser API actions requested by inline JavaScript during page load.
    pub browser_actions: Vec<webby_js::BrowserAction>,
    /// Web Storage mutations requested by inline JavaScript during page load.
    pub storage_actions: Vec<webby_js::StorageAction>,
    /// Pipeline stages dirtied by the most recent dynamic DOM mutation pass.
    pub dirty: webby_js::DirtyState,
    /// Page-level transition metadata gathered from computed styles.
    pub transition: PageTransitionSpec,
}

/// A rendered nested browsing context for an iframe element.
#[derive(Debug, Clone, PartialEq)]
pub struct IframeContext {
    /// `src` value as written in the parent document.
    pub src: String,
    /// Resolved current URL for the iframe context.
    pub url: url::Url,
    /// Optional iframe browsing-context name.
    pub name: Option<String>,
    /// Optional sandbox attribute, currently diagnostic-only.
    pub sandbox: Option<String>,
    /// Iframe content rectangle in parent page coordinates.
    pub rect: webby_layout::Rect,
    /// Independent iframe scroll offset.
    pub scroll_y: f32,
    /// Rendered child page.
    pub page: Box<RenderedPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IframeLinkHit {
    src: String,
    base_url: url::Url,
    href: String,
    viewport_width: usize,
    viewport_height: usize,
}

/// Page-level transition metadata used by the app animation clock.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageTransitionSpec {
    /// Longest supported transition duration in milliseconds.
    pub duration_ms: u32,
    /// Delay in milliseconds.
    pub delay_ms: u32,
    /// Timing function.
    pub timing_function: AnimationTimingFunction,
}

/// App-owned animation timing function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationTimingFunction {
    /// Linear interpolation.
    Linear,
    /// Deterministic ease interpolation.
    #[default]
    Ease,
}

/// Active page transition for the visible tab.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveTransition {
    /// Previous display list.
    pub from: DisplayList,
    /// New display list.
    pub to: DisplayList,
    /// Duration in milliseconds.
    pub duration_ms: u32,
    /// Delay in milliseconds.
    pub delay_ms: u32,
    /// Elapsed animation clock in milliseconds.
    pub elapsed_ms: u32,
    /// Timing function.
    pub timing_function: AnimationTimingFunction,
}

/// Inputs for rendering an already parsed and script-mutated document.
#[derive(Debug, Clone, PartialEq)]
pub struct MutatedDocumentRenderInput {
    /// DOM document after script/event/timer mutations.
    pub document: webby_dom::Document,
    /// URL that owns the document.
    pub url: url::Url,
    /// Decoded image metadata keyed by source attribute.
    pub images: ImageMap,
    /// Already-rendered iframe pages keyed by source attribute.
    pub iframes: std::collections::BTreeMap<String, RenderedPage>,
    /// External stylesheet rules already loaded by the caller.
    pub external_stylesheets: Vec<webby_css::Stylesheet>,
    /// Event handlers registered for the page.
    pub event_handlers: Vec<webby_js::EventHandler>,
    /// Non-fatal diagnostics collected so far.
    pub diagnostics: Vec<String>,
    /// Browser API actions collected so far.
    pub browser_actions: Vec<webby_js::BrowserAction>,
    /// Web Storage mutations collected so far.
    pub storage_actions: Vec<webby_js::StorageAction>,
    /// Dirty state collected so far.
    pub dirty: webby_js::DirtyState,
}

/// Selected layout-box inspection details for Webby's debug overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectionInfo {
    /// Source tag name or synthetic node label.
    pub tag_name: String,
    /// Source `id` attribute, if present.
    pub id: Option<String>,
    /// Source classes, in source order.
    pub classes: Vec<String>,
    /// CSS box-model dimensions.
    pub dimensions: Dimensions,
    /// Short computed style summary.
    pub computed_style: String,
    /// Paint/display-list summary for this page.
    pub paint_summary: String,
    /// Link target, when the inspected box participates in link behavior.
    pub link_href: Option<String>,
    /// Image metadata, when inspecting an image box.
    pub image_metadata: Option<String>,
    /// Form metadata, when inspecting a form control.
    pub form_metadata: Option<String>,
}

/// One independent browser tab/session context.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserTab {
    /// Address/search bar state for this tab.
    pub chrome: BrowserChrome,
    /// Navigation state for this tab.
    pub navigation: NavigationController,
    /// Visible page status for this tab.
    pub status: PageStatus,
    /// Last rendered page for this tab, if available.
    pub page: Option<RenderedPage>,
    /// Vertical scroll offset for this tab.
    pub scroll_y: f32,
    /// Per-container nested scroll offsets for this tab.
    pub scroll_offsets: ScrollOffsets,
    /// Focused form control id for this tab.
    pub focused_form_control: Option<usize>,
    /// Keyboard-focused page target for this tab.
    pub keyboard_focus: Option<KeyboardFocusTarget>,
    /// Edited form input values for this tab.
    pub form_values: BTreeMap<usize, String>,
    /// Deterministic one-shot timers owned by this tab.
    pub timers: Vec<BrowserTimer>,
    /// Non-persistent sessionStorage values for this tab.
    pub session_storage: StorageState,
    /// Active visual transition for this tab.
    pub active_transition: Option<ActiveTransition>,
    /// Current find-in-page query.
    pub find_query: String,
    /// Number of visible-text matches for the current query.
    pub find_match_count: usize,
    /// Whether find-in-page input is active.
    pub find_active: bool,
}

impl BrowserTab {
    /// Creates an empty tab with startup chrome state.
    pub fn new() -> Self {
        Self {
            chrome: BrowserChrome::new(),
            navigation: NavigationController::default(),
            status: PageStatus::Startup,
            page: None,
            scroll_y: 0.0,
            scroll_offsets: ScrollOffsets::new(),
            focused_form_control: None,
            keyboard_focus: None,
            form_values: BTreeMap::new(),
            timers: Vec::new(),
            session_storage: StorageState::default(),
            active_transition: None,
            find_query: String::new(),
            find_match_count: 0,
            find_active: false,
        }
    }
}

/// One deterministic JavaScript timer callback scheduled by a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTimer {
    /// JavaScript-visible timer id.
    pub id: u32,
    /// Callback source captured from `setTimeout`.
    pub callback: String,
    /// Requested delay in milliseconds. Tests drive timers explicitly.
    pub delay_ms: u32,
    /// URL of the page that created the timer.
    pub page_url: String,
    /// Navigation generation of the page that created the timer.
    pub page_generation: u64,
}

/// One deterministic shell-managed resource download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRecord {
    /// Final resource URL written by the shell.
    pub url: url::Url,
    /// Sanitized destination filename.
    pub filename: String,
    /// Destination path selected by the caller.
    pub destination: std::path::PathBuf,
    /// Number of bytes written.
    pub byte_len: usize,
}

/// Visible HTTP Basic authentication challenge state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthChallengeState {
    /// Protected resource URL.
    pub url: url::Url,
    /// Origin key used for the in-memory credential map.
    pub origin: String,
    /// Redacted challenge metadata.
    pub challenge: BasicAuthChallenge,
}

impl Default for BrowserTab {
    fn default() -> Self {
        Self::new()
    }
}

/// Native-shell state that can be unit-tested without opening a window.
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    /// Open browser tabs.
    pub tabs: Vec<BrowserTab>,
    /// Active tab index.
    pub active_tab_index: usize,
    /// Address/search bar state.
    pub chrome: BrowserChrome,
    /// Navigation state.
    pub navigation: NavigationController,
    /// Current page status.
    pub status: PageStatus,
    /// Last rendered page, if available.
    pub page: Option<RenderedPage>,
    /// Window width in pixels.
    pub window_width: usize,
    /// Window height in pixels.
    pub window_height: usize,
    /// Vertical scroll offset for page content.
    pub scroll_y: f32,
    /// Per-container nested scroll offsets for the active tab.
    pub scroll_offsets: ScrollOffsets,
    /// Focused form control id, if keyboard input is directed into page content.
    pub focused_form_control: Option<usize>,
    /// Keyboard-focused page target.
    pub keyboard_focus: Option<KeyboardFocusTarget>,
    /// Edited form input values keyed by layout control id.
    pub form_values: BTreeMap<usize, String>,
    /// Deterministic one-shot timers for the active tab.
    pub timers: Vec<BrowserTimer>,
    /// Non-persistent sessionStorage values for the active tab.
    pub session_storage: StorageState,
    /// Active deterministic transition for the visible page.
    pub active_transition: Option<ActiveTransition>,
    /// Current find-in-page query for the active tab.
    pub find_query: String,
    /// Number of visible-text matches for the current query.
    pub find_match_count: usize,
    /// Whether find-in-page input is active.
    pub find_active: bool,
    /// App-local clipboard used by deterministic shell shortcuts.
    pub clipboard: String,
    /// Whether keyboard shortcut help is visible.
    pub shortcut_help_visible: bool,
    /// Shell-managed resource downloads completed this session.
    pub downloads: Vec<DownloadRecord>,
    /// Directory used for downloads started by normal page navigation.
    pub download_directory: std::path::PathBuf,
    /// Deterministic shell download diagnostics.
    pub download_diagnostics: Vec<String>,
    /// In-memory HTTP Basic credentials keyed by Webby origin.
    pub basic_auth_credentials: BTreeMap<String, BasicCredentials>,
    /// Current redacted Basic-auth challenge, if navigation needs credentials.
    pub auth_challenge: Option<AuthChallengeState>,
    /// Deterministic authentication diagnostics with credentials redacted.
    pub auth_diagnostics: Vec<String>,
    /// Whether visual transitions are enabled.
    pub animations_enabled: bool,
    /// Whether the debug layout overlay is visible.
    pub debug_overlay_enabled: bool,
    /// Last selected debug inspection target.
    pub selected_inspection: Option<InspectionInfo>,
    /// Current hover target for native cursor/status feedback.
    pub hover_target: Option<HoverTarget>,
    /// Hovered page DOM node id used for `:hover` styling.
    pub hovered_node_id: Option<webby_dom::NodeId>,
    /// Profile-level cookie jar for stateful resource loading.
    pub cookie_jar: CookieJar,
    /// Whether cookies are accepted and sent for this app session.
    pub cookies_enabled: bool,
    /// Whether persistent cookies are saved back to profile state.
    pub persist_cookies: bool,
    /// Whether inline JavaScript executes during page loads.
    pub javascript_enabled: bool,
    /// Whether Web Storage APIs are enabled for scripts.
    pub storage_enabled: bool,
    /// Persistent localStorage values for this app session/profile.
    pub local_storage: StorageState,
    /// Shared in-memory byte cache for page, stylesheet, and image resources.
    pub resource_cache: ResourceCache,
    /// Optional persistent disk tier below the in-memory resource cache.
    pub disk_cache: Option<DiskResourceCache>,
}

impl AppState {
    /// Creates default app state for the native window.
    pub fn new() -> Self {
        Self::with_window_size(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
    }

    /// Creates app state with explicit dimensions.
    pub fn with_window_size(width: usize, height: usize) -> Self {
        let tab = BrowserTab::new();
        Self {
            tabs: vec![tab.clone()],
            active_tab_index: 0,
            chrome: tab.chrome,
            navigation: tab.navigation,
            status: tab.status,
            page: tab.page,
            window_width: width.max(1),
            window_height: height.max(CHROME_HEIGHT + 1),
            scroll_y: tab.scroll_y,
            scroll_offsets: tab.scroll_offsets,
            focused_form_control: tab.focused_form_control,
            keyboard_focus: tab.keyboard_focus,
            form_values: tab.form_values,
            timers: tab.timers,
            session_storage: tab.session_storage,
            active_transition: tab.active_transition,
            find_query: tab.find_query,
            find_match_count: tab.find_match_count,
            find_active: tab.find_active,
            clipboard: String::new(),
            shortcut_help_visible: false,
            downloads: Vec::new(),
            download_directory: default_download_directory(),
            download_diagnostics: Vec::new(),
            basic_auth_credentials: BTreeMap::new(),
            auth_challenge: None,
            auth_diagnostics: Vec::new(),
            animations_enabled: true,
            debug_overlay_enabled: false,
            selected_inspection: None,
            hover_target: None,
            hovered_node_id: None,
            cookie_jar: CookieJar::default(),
            cookies_enabled: true,
            persist_cookies: false,
            javascript_enabled: true,
            storage_enabled: true,
            local_storage: StorageState::default(),
            resource_cache: ResourceCache::new(),
            disk_cache: None,
        }
    }

    /// Creates app state using the persisted homepage/start page setting.
    pub fn with_profile(profile: &BrowserProfile) -> WebbyResult<Self> {
        Self::with_profile_and_window_size(profile, DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
    }

    /// Creates app state using persisted config and explicit dimensions.
    pub fn with_profile_and_window_size(
        profile: &BrowserProfile,
        width: usize,
        height: usize,
    ) -> WebbyResult<Self> {
        let mut state = Self::with_window_size(width, height);
        state
            .chrome
            .set_address_input(profile.validated_homepage()?.to_string());
        state.cookie_jar = profile.cookies.clone();
        state.cookies_enabled = profile.config.cookies_enabled;
        state.persist_cookies = profile.config.persist_cookies;
        state.javascript_enabled = profile.config.javascript_enabled;
        state.storage_enabled = profile.config.storage_enabled;
        state.animations_enabled = profile.config.animations_enabled;
        state.local_storage = profile.local_storage.clone();
        state.save_active_tab();
        Ok(state)
    }

    /// Enables the persistent resource cache for this app session.
    pub fn enable_disk_cache(&mut self, root: impl Into<std::path::PathBuf>) -> WebbyResult<()> {
        self.disk_cache = Some(DiskResourceCache::open(root)?);
        Ok(())
    }

    /// Clears non-persistent sessionStorage across open tabs without closing or
    /// corrupting active browsing contexts.
    pub fn clear_session_storage(&mut self) {
        self.session_storage.clear();
        for tab in &mut self.tabs {
            tab.session_storage.clear();
        }
        self.save_active_tab();
    }

    /// Number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Returns the active tab, if the tab list is valid.
    pub fn active_tab(&self) -> Option<&BrowserTab> {
        self.tabs.get(self.active_tab_index)
    }

    /// Verifies that the public active-tab compatibility view matches `BrowserTab`.
    pub fn validate_active_tab_sync(&self) -> WebbyResult<()> {
        let Some(tab) = self.active_tab() else {
            return Err(WebbyError::invalid_input(format!(
                "active tab index {} is out of range for {} tabs",
                self.active_tab_index,
                self.tabs.len()
            )));
        };
        if self.chrome != tab.chrome
            || self.navigation != tab.navigation
            || self.status != tab.status
            || self.page != tab.page
            || self.scroll_y != tab.scroll_y
            || self.scroll_offsets != tab.scroll_offsets
            || self.focused_form_control != tab.focused_form_control
            || self.keyboard_focus != tab.keyboard_focus
            || self.form_values != tab.form_values
            || self.timers != tab.timers
            || self.session_storage != tab.session_storage
            || self.active_transition != tab.active_transition
            || self.find_query != tab.find_query
            || self.find_match_count != tab.find_match_count
            || self.find_active != tab.find_active
        {
            return Err(WebbyError::invalid_input(
                "active tab compatibility view is out of sync with authoritative tab state",
            ));
        }
        Ok(())
    }

    /// Toggles the debug layout overlay.
    pub fn toggle_debug_overlay(&mut self) {
        self.debug_overlay_enabled = !self.debug_overlay_enabled;
        if !self.debug_overlay_enabled {
            self.selected_inspection = None;
        }
    }

    /// Selects a layout box under a window coordinate for debug inspection.
    pub fn inspect_at_window_position(&mut self, x: f32, y: f32) -> Option<InspectionInfo> {
        if !self.debug_overlay_enabled
            || !matches!(self.status, PageStatus::Loaded { .. })
            || !x.is_finite()
            || !y.is_finite()
            || y < CHROME_HEIGHT as f32
        {
            self.selected_inspection = None;
            return None;
        }

        let page_y = y - CHROME_HEIGHT as f32 + self.scroll_y;
        let page = self.page.as_ref()?;
        let layout_box = find_deepest_box_at(&page.layout.root, x, page_y)?;
        let info = inspection_info_for_box(layout_box, page);
        self.selected_inspection = Some(info.clone());
        Some(info)
    }

    /// Opens a new startup tab and switches to it.
    pub fn new_tab(&mut self) -> usize {
        self.save_active_tab();
        self.tabs.push(BrowserTab::new());
        self.active_tab_index = self.tabs.len().saturating_sub(1);
        self.hovered_node_id = None;
        self.hover_target = None;
        self.load_active_tab();
        self.active_tab_index
    }

    /// Closes a tab. At least one tab remains open.
    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return false;
        }
        self.save_active_tab();
        self.tabs.remove(index);
        if index < self.active_tab_index {
            self.active_tab_index = self.active_tab_index.saturating_sub(1);
        } else if index == self.active_tab_index && self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len().saturating_sub(1);
        }
        self.hovered_node_id = None;
        self.hover_target = None;
        self.load_active_tab();
        true
    }

    /// Closes the active tab, preserving at least one open tab.
    pub fn close_active_tab(&mut self) -> bool {
        self.close_tab(self.active_tab_index)
    }

    /// Switches to an existing tab.
    pub fn switch_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return index < self.tabs.len();
        }
        self.save_active_tab();
        self.active_tab_index = index;
        self.hovered_node_id = None;
        self.hover_target = None;
        self.load_active_tab();
        true
    }

    /// Switches to the next tab, wrapping at the end.
    pub fn next_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        let next = (self.active_tab_index + 1) % self.tabs.len();
        self.switch_tab(next)
    }

    /// Switches to the previous tab, wrapping at the beginning.
    pub fn previous_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        let previous = if self.active_tab_index == 0 {
            self.tabs.len().saturating_sub(1)
        } else {
            self.active_tab_index - 1
        };
        self.switch_tab(previous)
    }

    /// Returns the configured homepage/start page address.
    pub fn homepage_from_profile(profile: &BrowserProfile) -> WebbyResult<String> {
        Ok(profile.validated_homepage()?.to_string())
    }

    /// Adds a bookmark through the persistence API.
    pub fn add_bookmark(
        &self,
        store: &ProfileStore,
        profile: &mut BrowserProfile,
        url: &url::Url,
    ) -> WebbyResult<()> {
        store.add_bookmark(profile, url)
    }

    /// Removes a bookmark through the persistence API.
    pub fn remove_bookmark(
        &self,
        store: &ProfileStore,
        profile: &mut BrowserProfile,
        url: &url::Url,
    ) -> WebbyResult<bool> {
        store.remove_bookmark(profile, url)
    }

    /// Lists bookmarks from the loaded profile.
    pub fn list_bookmarks<'a>(&self, profile: &'a BrowserProfile) -> &'a [Bookmark] {
        profile.list_bookmarks()
    }

    /// Opens a persisted bookmark through normal navigation.
    pub fn open_bookmark<L: ResourceLoader>(&mut self, loader: &L, bookmark: &Bookmark) -> bool {
        let target = match url::Url::parse(&bookmark.url) {
            Ok(target) => target,
            Err(error) => {
                self.set_target_error(
                    bookmark.url.clone(),
                    WebbyError::Url {
                        message: format!("invalid bookmark URL {:?}: {error}", bookmark.url),
                    },
                );
                return true;
            }
        };
        self.navigate_to_url(loader, target);
        true
    }

    /// Opens Webby's simple generated bookmarks page.
    pub fn open_bookmarks_page(&mut self, profile: &BrowserProfile) -> WebbyResult<()> {
        let url = url::Url::parse(BOOKMARKS_PAGE_URL).map_err(|error| WebbyError::Url {
            message: format!("invalid internal bookmarks URL: {error}"),
        })?;
        let html = bookmarks_page_html(profile.list_bookmarks());
        let pipeline = PagePipeline::new(self.window_width, self.page_viewport_height())
            .with_javascript_enabled(self.javascript_enabled);
        self.finish_navigation(pipeline.render_html(&html, url));
        Ok(())
    }

    /// Records the visible successful page in persistent history.
    pub fn record_successful_navigation(
        &self,
        store: &ProfileStore,
        profile: &mut BrowserProfile,
    ) -> WebbyResult<bool> {
        if !matches!(self.status, PageStatus::Loaded { .. }) {
            return Ok(false);
        }
        let Some(current_url) = &self.navigation.current_url else {
            return Ok(false);
        };
        if self.persist_cookies {
            profile.cookies = self.cookie_jar.persistent_only();
        }
        profile.local_storage = self.local_storage.clone();
        store.record_successful_navigation(profile, current_url)?;
        Ok(true)
    }

    /// Page viewport height below browser chrome.
    pub fn page_viewport_height(&self) -> usize {
        self.window_height.saturating_sub(CHROME_HEIGHT).max(1)
    }

    /// Updates window dimensions and clamps scroll to the new viewport.
    pub fn resize(&mut self, width: usize, height: usize) {
        let old_width = self.window_width;
        let old_height = self.window_height;
        self.window_width = width.max(1);
        self.window_height = height.max(CHROME_HEIGHT + 1);
        if old_width != self.window_width || old_height != self.window_height {
            self.rerender_loaded_page_for_current_viewport();
        }
        self.clamp_scroll();
        self.save_active_tab();
    }

    fn rerender_loaded_page_for_current_viewport(&mut self) {
        if !matches!(self.status, PageStatus::Loaded { .. }) {
            return;
        }
        let Some(page) = self.page.clone() else {
            return;
        };
        let previous_diagnostics = page.diagnostics.clone();
        let interaction = self.current_style_interaction();
        let pipeline = PagePipeline::new(self.window_width, self.page_viewport_height())
            .with_javascript_enabled(self.javascript_enabled)
            .with_storage(
                self.storage_enabled,
                self.local_storage_entries_for_url(&page.url),
                self.session_storage_entries_for_url(&page.url),
            )
            .with_style_interaction(interaction);
        if let Ok(mut rerendered) = pipeline.render_mutated_document(MutatedDocumentRenderInput {
            document: page.document,
            url: page.url,
            images: page.images,
            iframes: iframe_page_map(&page.iframes),
            external_stylesheets: page.external_stylesheets,
            event_handlers: page.event_handlers,
            diagnostics: Vec::new(),
            browser_actions: page.browser_actions,
            storage_actions: page.storage_actions,
            dirty: page.dirty,
        }) {
            let rerender_diagnostics = rerendered.diagnostics;
            rerendered.diagnostics = previous_diagnostics;
            for diagnostic in rerender_diagnostics {
                if !rerendered.diagnostics.contains(&diagnostic) {
                    rerendered.diagnostics.push(diagnostic);
                }
            }
            self.apply_storage_actions_for_page(&mut rerendered);
            self.install_rerendered_page(rerendered);
        }
    }

    fn install_rerendered_page(&mut self, mut page: RenderedPage) {
        let _ =
            apply_nested_scroll_offsets_to_page(&mut page, &self.scroll_offsets, self.window_width);
        let transition = self.page.as_ref().and_then(|previous| {
            transition_between_pages(previous, &page, self.animations_enabled)
        });
        self.active_transition = transition;
        self.page = Some(page);
        self.save_active_tab();
    }

    /// Selects the whole address field.
    pub fn select_all_address(&mut self) {
        self.find_active = false;
        self.chrome.focus_address(true);
        self.save_active_tab();
    }

    /// Moves the address insertion cursor one character left.
    pub fn move_address_cursor_left(&mut self) {
        if self.chrome.address_focused {
            self.chrome.move_cursor_left();
            self.save_active_tab();
        }
    }

    /// Moves the address insertion cursor one character right.
    pub fn move_address_cursor_right(&mut self) {
        if self.chrome.address_focused {
            self.chrome.move_cursor_right();
            self.save_active_tab();
        }
    }

    /// Moves the address insertion cursor to the beginning.
    pub fn move_address_cursor_home(&mut self) {
        if self.chrome.address_focused {
            self.chrome.move_cursor_home();
            self.save_active_tab();
        }
    }

    /// Moves the address insertion cursor to the end.
    pub fn move_address_cursor_end(&mut self) {
        if self.chrome.address_focused {
            self.chrome.move_cursor_end();
            self.save_active_tab();
        }
    }

    /// Copies selected address text, or the committed URL when nothing is selected.
    pub fn copy_address_or_current_url(&mut self) -> bool {
        let value = self.chrome.selected_text().map(str::to_string).or_else(|| {
            self.navigation
                .current_url
                .as_ref()
                .map(url::Url::to_string)
        });
        let Some(value) = value else {
            return false;
        };
        self.clipboard = value;
        true
    }

    /// Pastes app-local clipboard text into the focused address field.
    pub fn paste_address(&mut self) -> bool {
        if !self.chrome.address_focused {
            return false;
        }
        self.chrome.paste(&self.clipboard);
        self.save_active_tab();
        true
    }

    /// Opens find-in-page input for the active tab.
    pub fn open_find(&mut self) {
        self.find_active = true;
        self.chrome.address_focused = false;
        self.update_find_matches();
        self.save_active_tab();
    }

    /// Closes find-in-page input without clearing its query.
    pub fn close_find(&mut self) {
        self.find_active = false;
        self.save_active_tab();
    }

    /// Toggles the keyboard-shortcut help panel.
    pub fn toggle_shortcut_help(&mut self) {
        self.shortcut_help_visible = !self.shortcut_help_visible;
    }

    /// Returns the current page title, falling back to the committed URL.
    pub fn window_title(&self) -> String {
        self.page
            .as_ref()
            .map(|page| page.title.clone())
            .filter(|title| !title.is_empty())
            .or_else(|| {
                self.navigation
                    .current_url
                    .as_ref()
                    .map(url::Url::to_string)
            })
            .unwrap_or_else(|| "Webby".to_string())
    }

    /// Returns concise visible shell status, including hovered-link feedback.
    pub fn status_bar_text(&self) -> String {
        if let Some(HoverTarget::Link(href)) = &self.hover_target {
            return href.clone();
        }
        match &self.status {
            PageStatus::Startup => "Ready".to_string(),
            PageStatus::Loading { url } => format!("Loading {url}"),
            PageStatus::Loaded { .. } => self.page.as_ref().map_or_else(
                || "Loaded".to_string(),
                |page| format!("Ready diagnostics={}", page.diagnostics.len()),
            ),
            PageStatus::Error { message } => format!("Error: {message}"),
        }
    }

    /// Resolves and opens a local HTML file through normal page navigation.
    pub fn open_file<L: ResourceLoader>(&mut self, loader: &L, path: &std::path::Path) -> bool {
        let absolute = match path.canonicalize() {
            Ok(path) => path,
            Err(source) => {
                self.set_error(WebbyError::Io {
                    path: Some(path.to_path_buf()),
                    source,
                });
                return false;
            }
        };
        let target = match url::Url::from_file_path(&absolute) {
            Ok(url) => url,
            Err(()) => {
                self.set_error(WebbyError::Url {
                    message: format!(
                        "could not convert open-file path to URL: {}",
                        absolute.display()
                    ),
                });
                return false;
            }
        };
        self.navigate_to_url(loader, target);
        matches!(self.status, PageStatus::Loaded { .. })
    }

    /// Downloads one resource through the supplied loader to an explicit path.
    pub fn download_resource<L: ResourceLoader>(
        &mut self,
        loader: &L,
        url: &url::Url,
        destination: &std::path::Path,
    ) -> WebbyResult<DownloadRecord> {
        let response = loader.load(url)?;
        std::fs::write(destination, &response.bytes).map_err(|source| WebbyError::Io {
            path: Some(destination.to_path_buf()),
            source,
        })?;
        let record = DownloadRecord {
            url: response.final_url,
            filename: destination
                .file_name()
                .and_then(|filename| filename.to_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "download".to_string()),
            destination: destination.to_path_buf(),
            byte_len: response.bytes.len(),
        };
        self.downloads.push(record.clone());
        Ok(record)
    }

    /// Replaces the directory used for navigation-triggered downloads.
    pub fn set_download_directory(&mut self, directory: impl Into<std::path::PathBuf>) {
        self.download_directory = directory.into();
    }

    /// Stores HTTP Basic credentials in memory for one origin.
    pub fn set_basic_auth_credentials(
        &mut self,
        url: &url::Url,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> WebbyResult<()> {
        let origin = webby_security::origin_key(url)?;
        self.basic_auth_credentials
            .insert(origin, BasicCredentials::new(username, password));
        self.auth_challenge = None;
        self.save_active_tab();
        Ok(())
    }

    fn complete_navigation_download(
        &mut self,
        response: ResourceResponse,
        metadata: DownloadMetadata,
        mut diagnostics: Vec<String>,
    ) {
        let result = self.write_download_response(&response, &metadata);
        match result {
            Ok(record) => {
                diagnostics.push(format!(
                    "download complete url={} destination={} bytes={} reason={}",
                    record.url,
                    record.destination.display(),
                    record.byte_len,
                    metadata.reason
                ));
                self.downloads.push(record);
                self.download_diagnostics.extend(diagnostics);
                self.navigation.pending_url = None;
                self.navigation.pending_history_action = None;
                self.navigation.failed_url = None;
                self.navigation.lifecycle = NavigationLifecycle::Complete;
                self.status = if let Some(url) = self.navigation.current_url.as_ref() {
                    PageStatus::Loaded {
                        url: url.to_string(),
                    }
                } else {
                    PageStatus::Startup
                };
                self.save_active_tab();
            }
            Err(error) => self.set_navigation_error(error),
        }
    }

    fn write_download_response(
        &self,
        response: &ResourceResponse,
        metadata: &DownloadMetadata,
    ) -> WebbyResult<DownloadRecord> {
        std::fs::create_dir_all(&self.download_directory).map_err(|source| WebbyError::Io {
            path: Some(self.download_directory.clone()),
            source,
        })?;
        let filename = sanitize_download_filename(&metadata.suggested_filename);
        let destination = unique_download_destination(&self.download_directory, &filename);
        std::fs::write(&destination, &response.bytes).map_err(|source| WebbyError::Io {
            path: Some(destination.clone()),
            source,
        })?;
        Ok(DownloadRecord {
            url: response.final_url.clone(),
            filename: destination
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| filename.clone()),
            destination,
            byte_len: response.bytes.len(),
        })
    }

    /// Adds a character to the focused address bar.
    pub fn type_character(&mut self, character: char) {
        if self.find_active {
            if !character.is_control() {
                self.find_query.push(character);
                self.update_find_matches();
            }
        } else if self.chrome.address_focused {
            self.chrome.type_character(character);
        } else if let Some(control_id) = self.focused_form_control
            && !character.is_control()
            && focused_control_is_text_editable(self.page.as_ref(), control_id)
        {
            self.form_values
                .entry(control_id)
                .or_insert_with(|| form_control_value(self.page.as_ref(), control_id))
                .push(character);
            let _ = self.dispatch_input_event_for_control(control_id);
        }
        self.save_active_tab();
    }

    /// Removes one character from the address bar.
    pub fn backspace(&mut self) {
        if self.find_active {
            self.find_query.pop();
            self.update_find_matches();
        } else if self.chrome.address_focused {
            self.chrome.backspace();
        } else if let Some(control_id) = self.focused_form_control
            && focused_control_is_text_editable(self.page.as_ref(), control_id)
        {
            self.form_values
                .entry(control_id)
                .or_insert_with(|| form_control_value(self.page.as_ref(), control_id))
                .pop();
            let _ = self.dispatch_input_event_for_control(control_id);
        }
        self.save_active_tab();
    }

    fn update_find_matches(&mut self) {
        self.find_match_count = find_match_count(self.page.as_ref(), &self.find_query);
    }

    /// Moves keyboard focus to the next page link or form control.
    pub fn focus_next_page_item(&mut self) -> bool {
        self.move_page_focus(false)
    }

    /// Moves keyboard focus to the previous page link or form control.
    pub fn focus_previous_page_item(&mut self) -> bool {
        self.move_page_focus(true)
    }

    /// Activates the currently focused link or form control with Enter.
    pub fn activate_keyboard_focus<L: ResourceLoader>(&mut self, loader: &L) -> bool {
        let Some(target) = self.keyboard_focus else {
            return false;
        };
        match target {
            KeyboardFocusTarget::Link { index } => {
                let Some(link) = self
                    .page
                    .as_ref()
                    .and_then(|page| page.links.get(index))
                    .cloned()
                else {
                    return false;
                };
                self.activate_link_hit(loader, link)
            }
            KeyboardFocusTarget::FormControl { id } => {
                self.activate_form_control_by_keyboard(loader, id, KeyboardActivation::Enter)
            }
        }
    }

    /// Activates Space semantics for the currently focused button/checkbox/radio.
    pub fn press_space_on_keyboard_focus<L: ResourceLoader>(&mut self, loader: &L) -> bool {
        let Some(KeyboardFocusTarget::FormControl { id }) = self.keyboard_focus else {
            return false;
        };
        self.activate_form_control_by_keyboard(loader, id, KeyboardActivation::Space)
    }

    /// Scrolls the page from keyboard input and clamps to valid extents.
    pub fn keyboard_scroll_by(&mut self, delta_y: f32) {
        self.scroll_by(delta_y);
    }

    /// Returns inspectable accessibility metadata for the loaded page.
    pub fn accessible_nodes(&self) -> Vec<AccessibleNode> {
        let Some(page) = &self.page else {
            return Vec::new();
        };
        if !matches!(self.status, PageStatus::Loaded { .. }) {
            return Vec::new();
        }
        let mut nodes = Vec::new();
        for (index, link) in page.links.iter().enumerate() {
            nodes.push(AccessibleNode {
                role: AccessibleRole::Link,
                name: link
                    .node_id
                    .and_then(|node_id| page.document.find_node(node_id))
                    .map(node_text_content)
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| link.href.clone()),
                rect: link.rect,
                node_id: link.node_id,
                href: Some(link.href.clone()),
                control_id: None,
                state: AccessibleState {
                    focused: self.keyboard_focus == Some(KeyboardFocusTarget::Link { index }),
                    ..AccessibleState::default()
                },
            });
        }
        for control in &page.form_controls {
            if control.disabled {
                continue;
            }
            let node_id = find_form_control_node_id(&page.document.root, control.id);
            nodes.push(AccessibleNode {
                role: accessible_role_for_control(control.control_type),
                name: accessible_name_for_control(page, control, node_id),
                rect: control.rect,
                node_id,
                href: None,
                control_id: Some(control.id),
                state: AccessibleState {
                    disabled: control.disabled,
                    checked: match control.control_type {
                        FormControlType::Checkbox | FormControlType::Radio => {
                            Some(control_checked_state(control, &self.form_values))
                        }
                        _ => None,
                    },
                    focused: self.keyboard_focus
                        == Some(KeyboardFocusTarget::FormControl { id: control.id }),
                },
            });
        }
        nodes.sort_by(focusable_node_order);
        nodes
    }

    /// Handles Enter from a focused form input.
    pub fn submit_focused_form<L: ResourceLoader>(&mut self, loader: &L) -> bool {
        let Some(control_id) = self.focused_form_control else {
            return false;
        };
        self.submit_form_for_control(loader, control_id, None)
    }

    /// Handles Enter from the address bar using the provided loader.
    pub fn submit_address<L: ResourceLoader>(&mut self, loader: &L) {
        let Some(url) = self.begin_navigation() else {
            return;
        };
        self.load_pending_url(loader, &url, CacheMode::Use);
    }

    /// Resolves current address/search text and transitions to loading state.
    pub fn begin_navigation(&mut self) -> Option<url::Url> {
        self.navigation.lifecycle = NavigationLifecycle::Resolving;
        let target = match resolve_address_input(&self.chrome.address_input) {
            Ok(url) => url,
            Err(error) => {
                self.set_error(error);
                return None;
            }
        };

        self.begin_navigation_to_url(target.clone(), PendingHistoryAction::Push);
        Some(target)
    }

    /// Starts navigation to an already resolved URL.
    fn begin_navigation_to_url(&mut self, target: url::Url, action: PendingHistoryAction) {
        self.cancel_pending_navigation_for_new_target(&target);
        self.navigation.generation = self.navigation.generation.saturating_add(1);
        self.navigation.pending_url = Some(target.clone());
        self.navigation.pending_history_action = Some(action);
        self.navigation.lifecycle = NavigationLifecycle::LoadingMainResource;
        self.timers.clear();
        self.active_transition = None;
        self.status = PageStatus::Loading {
            url: target.to_string(),
        };
        self.focused_form_control = None;
        self.keyboard_focus = None;
        self.hovered_node_id = None;
        self.hover_target = None;
        self.selected_inspection = None;
        self.save_active_tab();
    }

    fn cancel_pending_navigation_for_new_target(&mut self, next_target: &url::Url) {
        let Some(previous) = self.navigation.pending_url.as_ref() else {
            return;
        };
        self.navigation.lifecycle = NavigationLifecycle::Cancelled;
        self.navigation.lifecycle_diagnostics.push(format!(
            "navigation cancelled generation={} url={} replaced_by={}",
            self.navigation.generation, previous, next_target
        ));
    }

    /// Navigates to an already resolved URL and commits on success.
    pub fn navigate_to_url<L: ResourceLoader>(&mut self, loader: &L, target: url::Url) {
        self.begin_navigation_to_url(target.clone(), PendingHistoryAction::Push);
        self.load_pending_url(loader, &target, CacheMode::Use);
    }

    /// Navigates to an href resolved against the current page URL.
    pub fn navigate_href<L: ResourceLoader>(&mut self, loader: &L, href: &str) {
        let target = match self.resolve_href(href) {
            Ok(target) => target,
            Err(error) => {
                self.set_target_error(href.to_string(), error);
                return;
            }
        };
        self.navigate_to_url(loader, target);
    }

    /// Starts link navigation at a window coordinate and returns the resolved target.
    pub fn begin_link_navigation_at(&mut self, x: f32, y: f32) -> Option<url::Url> {
        let link = self.link_at_window_position(x, y)?;
        let target = match self.resolve_href(&link.href) {
            Ok(target) => target,
            Err(error) => {
                self.set_target_error(link.href, error);
                return None;
            }
        };
        self.begin_navigation_to_url(target.clone(), PendingHistoryAction::Push);
        Some(target)
    }

    /// Handles a window click, accounting for chrome height and scroll offset.
    pub fn click_at<L: ResourceLoader>(&mut self, x: f32, y: f32, loader: &L) -> bool {
        if self.chrome_action_at(x, y).is_some() {
            return false;
        }

        if self.debug_overlay_enabled && self.inspect_at_window_position(x, y).is_some() {
            return true;
        }

        if let Some(control) = self.form_control_at_window_position(x, y) {
            let before_url = self.navigation.current_url.clone();
            if let Some(default_prevented) =
                self.dispatch_event_for_form_control("click", control.id)
            {
                self.apply_loaded_page_browser_actions(loader);
                if default_prevented || self.navigation.current_url != before_url {
                    return true;
                }
            }
            match control.control_type {
                FormControlType::Text
                | FormControlType::Search
                | FormControlType::Password
                | FormControlType::Email
                | FormControlType::Textarea => {
                    self.chrome.address_focused = false;
                    self.focused_form_control = Some(control.id);
                    self.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: control.id });
                    self.form_values
                        .entry(control.id)
                        .or_insert_with(|| control.value.clone());
                    self.rerender_loaded_page_for_current_viewport();
                    self.save_active_tab();
                    return true;
                }
                FormControlType::Submit => {
                    self.chrome.address_focused = false;
                    self.focused_form_control = None;
                    self.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: control.id });
                    self.rerender_loaded_page_for_current_viewport();
                    return self.submit_form_for_control(loader, control.id, Some(control.id));
                }
                FormControlType::Checkbox => {
                    self.chrome.address_focused = false;
                    self.focused_form_control = None;
                    self.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: control.id });
                    let new_value = if control_checked_state(&control, &self.form_values) {
                        "false"
                    } else {
                        "true"
                    };
                    self.form_values.insert(control.id, new_value.to_string());
                    let _ = self.dispatch_input_event_for_control(control.id);
                    self.save_active_tab();
                    return true;
                }
                FormControlType::Radio => {
                    self.chrome.address_focused = false;
                    self.focused_form_control = None;
                    self.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: control.id });
                    self.select_radio_control(&control);
                    let _ = self.dispatch_input_event_for_control(control.id);
                    self.save_active_tab();
                    return true;
                }
                FormControlType::Select => {
                    self.chrome.address_focused = false;
                    self.focused_form_control = None;
                    self.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: control.id });
                    if let Some(next) = next_select_value(&control, &self.form_values) {
                        self.form_values.insert(control.id, next);
                        let _ = self.dispatch_input_event_for_control(control.id);
                    }
                    self.save_active_tab();
                    return true;
                }
                FormControlType::Reset => {
                    self.chrome.address_focused = false;
                    self.focused_form_control = None;
                    self.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: control.id });
                    self.reset_form_for_control(&control);
                    self.save_active_tab();
                    return true;
                }
                FormControlType::Button => {
                    self.chrome.address_focused = false;
                    self.focused_form_control = None;
                    self.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: control.id });
                    self.save_active_tab();
                    return true;
                }
            }
        }

        if let Some(hit) = self.iframe_link_at_window_position(x, y) {
            return self.navigate_iframe_link(loader, hit);
        }

        let Some(link) = self.link_at_window_position(x, y) else {
            if let Some(default_prevented) = self.dispatch_event_at_window_position("click", x, y) {
                self.apply_loaded_page_browser_actions(loader);
                return default_prevented || self.page.is_some();
            }
            return false;
        };
        self.keyboard_focus = link
            .node_id
            .and_then(|node_id| {
                self.page.as_ref().and_then(|page| {
                    page.links
                        .iter()
                        .position(|candidate| candidate.node_id == Some(node_id))
                })
            })
            .or_else(|| {
                self.page.as_ref().and_then(|page| {
                    page.links.iter().position(|candidate| {
                        candidate.href == link.href && candidate.rect == link.rect
                    })
                })
            })
            .map(|index| KeyboardFocusTarget::Link { index });
        self.focused_form_control = None;
        self.chrome.address_focused = false;
        let before_url = self.navigation.current_url.clone();
        if let Some(default_prevented) = self.dispatch_event_for_link("click", &link) {
            self.apply_loaded_page_browser_actions(loader);
            if default_prevented || self.navigation.current_url != before_url {
                return true;
            }
        }
        let target = match self.resolve_href(&link.href) {
            Ok(target) => target,
            Err(error) => {
                self.set_target_error(link.href, error);
                return true;
            }
        };
        self.begin_navigation_to_url(target.clone(), PendingHistoryAction::Push);
        self.load_pending_url(loader, &target, CacheMode::Use);
        true
    }

    fn move_page_focus(&mut self, reverse: bool) -> bool {
        if !matches!(self.status, PageStatus::Loaded { .. }) {
            return false;
        }
        let focusables = self.focusable_targets();
        if focusables.is_empty() {
            return false;
        }
        let current = self
            .keyboard_focus
            .and_then(|target| focusables.iter().position(|candidate| *candidate == target));
        let next_index = match (current, reverse) {
            (Some(0), true) | (None, true) => focusables.len().saturating_sub(1),
            (Some(index), true) => index.saturating_sub(1),
            (Some(index), false) => (index + 1) % focusables.len(),
            (None, false) => 0,
        };
        let target = focusables[next_index];
        self.chrome.address_focused = false;
        self.keyboard_focus = Some(target);
        self.focused_form_control = match target {
            KeyboardFocusTarget::FormControl { id } => Some(id),
            KeyboardFocusTarget::Link { .. } => None,
        };
        if let KeyboardFocusTarget::FormControl { id } = target
            && focused_control_is_text_editable(self.page.as_ref(), id)
        {
            let value = form_control_value(self.page.as_ref(), id);
            self.form_values.entry(id).or_insert(value);
        }
        self.rerender_loaded_page_for_current_viewport();
        self.save_active_tab();
        true
    }

    fn focusable_targets(&self) -> Vec<KeyboardFocusTarget> {
        let Some(page) = &self.page else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for (index, link) in page.links.iter().enumerate() {
            entries.push((link.rect, KeyboardFocusTarget::Link { index }));
        }
        for control in &page.form_controls {
            if !control.disabled {
                entries.push((
                    control.rect,
                    KeyboardFocusTarget::FormControl { id: control.id },
                ));
            }
        }
        entries.sort_by(|(left_rect, left), (right_rect, right)| {
            compare_focus_rects(*left_rect, *right_rect)
                .then_with(|| focus_target_key(*left).cmp(&focus_target_key(*right)))
        });
        entries.into_iter().map(|(_, target)| target).collect()
    }

    fn activate_link_hit<L: ResourceLoader>(&mut self, loader: &L, link: LinkHitBox) -> bool {
        let before_url = self.navigation.current_url.clone();
        if let Some(default_prevented) = self.dispatch_event_for_link("click", &link) {
            self.apply_loaded_page_browser_actions(loader);
            if default_prevented || self.navigation.current_url != before_url {
                return true;
            }
        }
        let target = match self.resolve_href(&link.href) {
            Ok(target) => target,
            Err(error) => {
                self.set_target_error(link.href, error);
                return true;
            }
        };
        self.begin_navigation_to_url(target.clone(), PendingHistoryAction::Push);
        self.load_pending_url(loader, &target, CacheMode::Use);
        true
    }

    fn activate_form_control_by_keyboard<L: ResourceLoader>(
        &mut self,
        loader: &L,
        control_id: usize,
        activation: KeyboardActivation,
    ) -> bool {
        let Some(control) = self
            .page
            .as_ref()
            .and_then(|page| {
                page.form_controls
                    .iter()
                    .find(|candidate| candidate.id == control_id)
            })
            .cloned()
        else {
            return false;
        };
        if control.disabled {
            return false;
        }
        match (activation, control.control_type) {
            (
                KeyboardActivation::Enter,
                FormControlType::Text
                | FormControlType::Search
                | FormControlType::Password
                | FormControlType::Email
                | FormControlType::Textarea,
            )
            | (KeyboardActivation::Enter | KeyboardActivation::Space, FormControlType::Submit) => {
                self.submit_form_for_control(loader, control.id, Some(control.id))
            }
            (KeyboardActivation::Enter | KeyboardActivation::Space, FormControlType::Button) => {
                true
            }
            (KeyboardActivation::Space, FormControlType::Checkbox) => {
                let new_value = if control_checked_state(&control, &self.form_values) {
                    "false"
                } else {
                    "true"
                };
                self.form_values.insert(control.id, new_value.to_string());
                let _ = self.dispatch_input_event_for_control(control.id);
                self.save_active_tab();
                true
            }
            (KeyboardActivation::Space, FormControlType::Radio) => {
                self.select_radio_control(&control);
                let _ = self.dispatch_input_event_for_control(control.id);
                self.save_active_tab();
                true
            }
            (KeyboardActivation::Enter | KeyboardActivation::Space, FormControlType::Reset) => {
                self.reset_form_for_control(&control);
                true
            }
            (KeyboardActivation::Enter, FormControlType::Select) => {
                if let Some(next) = next_select_value(&control, &self.form_values) {
                    self.form_values.insert(control.id, next);
                    let _ = self.dispatch_input_event_for_control(control.id);
                    self.save_active_tab();
                }
                true
            }
            _ => false,
        }
    }

    /// Returns the browser chrome action under a window coordinate.
    pub fn chrome_action_at(&self, x: f32, y: f32) -> Option<ChromeAction> {
        if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
            return None;
        }
        let x = x.floor() as usize;
        let y = y.floor() as usize;
        if y >= CHROME_HEIGHT {
            return None;
        }

        if y < TAB_STRIP_HEIGHT {
            return tab_action_at(self, x, y);
        }

        for (action, rect) in chrome_button_rects() {
            if point_in_usize_rect(x, y, rect) {
                return Some(action);
            }
        }

        let address_width = self.window_width.saturating_sub(ADDRESS_X + 12);
        if point_in_usize_rect(x, y, (ADDRESS_X, ADDRESS_Y, address_width, ADDRESS_HEIGHT)) {
            return Some(ChromeAction::FocusAddress);
        }

        None
    }

    /// Applies a chrome action that does not require profile mutation.
    pub fn apply_chrome_action<L: ResourceLoader>(
        &mut self,
        action: ChromeAction,
        loader: &L,
    ) -> bool {
        match action {
            ChromeAction::SwitchTab(index) => self.switch_tab(index),
            ChromeAction::CloseTab(index) => self.close_tab(index),
            ChromeAction::Back => self.go_back(loader),
            ChromeAction::Forward => self.go_forward(loader),
            ChromeAction::Reload => self.reload(loader),
            ChromeAction::BookmarkCurrentPage => false,
            ChromeAction::FocusAddress => {
                self.find_active = false;
                self.chrome.focus_address(true);
                self.focused_form_control = None;
                self.keyboard_focus = None;
                self.rerender_loaded_page_for_current_viewport();
                self.save_active_tab();
                true
            }
        }
    }

    /// Updates hover state from a window coordinate.
    pub fn update_hover_at(&mut self, x: f32, y: f32) -> Option<HoverTarget> {
        let previous_hovered_node_id = self.hovered_node_id;
        let target = if let Some(action) = self.chrome_action_at(x, y) {
            Some(HoverTarget::Chrome(action))
        } else if let Some(control) = self.form_control_at_window_position(x, y) {
            Some(HoverTarget::FormControl(control.id))
        } else {
            self.link_href_at_window_position(x, y)
                .map(HoverTarget::Link)
        };
        self.hovered_node_id = self.hover_node_id_at_window_position(x, y);
        self.hover_target = target.clone();
        if self.hovered_node_id != previous_hovered_node_id {
            self.rerender_loaded_page_for_current_viewport();
        }
        target
    }

    /// Returns the href under a window coordinate, if it hits page content.
    pub fn link_href_at_window_position(&self, x: f32, y: f32) -> Option<String> {
        self.link_at_window_position(x, y).map(|link| link.href)
    }

    fn current_style_interaction(&self) -> webby_style::StyleInteraction {
        let mut interaction = webby_style::StyleInteraction::new();
        if let Some(node_id) = self.hovered_node_id {
            interaction = interaction.with_hovered_node(node_id);
        }
        if let Some(control_id) = self.focused_form_control
            && let Some(node_id) = self
                .page
                .as_ref()
                .and_then(|page| find_form_control_node_id(&page.document.root, control_id))
        {
            interaction = interaction.with_focused_node(node_id);
        } else if let Some(KeyboardFocusTarget::Link { index }) = self.keyboard_focus
            && let Some(node_id) = self
                .page
                .as_ref()
                .and_then(|page| page.links.get(index))
                .and_then(|link| link.node_id)
        {
            interaction = interaction.with_focused_node(node_id);
        }
        interaction
    }

    fn hover_node_id_at_window_position(&self, x: f32, y: f32) -> Option<webby_dom::NodeId> {
        if self.chrome_action_at(x, y).is_some()
            || !matches!(self.status, PageStatus::Loaded { .. })
            || !x.is_finite()
            || !y.is_finite()
            || y < CHROME_HEIGHT as f32
        {
            return None;
        }

        if let Some(control) = self.form_control_at_window_position(x, y) {
            return self
                .page
                .as_ref()
                .and_then(|page| find_form_control_node_id(&page.document.root, control.id));
        }

        if let Some(link) = self.link_at_window_position(x, y)
            && let Some(node_id) = link.node_id
        {
            return Some(node_id);
        }

        let page_y = y - CHROME_HEIGHT as f32 + self.scroll_y;
        self.page
            .as_ref()
            .and_then(|page| find_deepest_box_at(&page.layout.root, x, page_y))
            .and_then(node_id_for_layout_box)
    }

    fn link_at_window_position(&self, x: f32, y: f32) -> Option<LinkHitBox> {
        if !matches!(self.status, PageStatus::Loaded { .. }) {
            return None;
        }

        if !x.is_finite() || !y.is_finite() || y < CHROME_HEIGHT as f32 {
            return None;
        }

        let page_y = y - CHROME_HEIGHT as f32 + self.scroll_y;
        self.page.as_ref().and_then(|page| {
            page.links
                .iter()
                .find(|link| point_in_link(x, page_y, link))
                .cloned()
        })
    }

    fn iframe_link_at_window_position(&self, x: f32, y: f32) -> Option<IframeLinkHit> {
        if !matches!(self.status, PageStatus::Loaded { .. }) {
            return None;
        }
        if !x.is_finite() || !y.is_finite() || y < CHROME_HEIGHT as f32 {
            return None;
        }
        let page_y = y - CHROME_HEIGHT as f32 + self.scroll_y;
        let page = self.page.as_ref()?;
        for iframe in &page.iframes {
            if !point_in_rect(x, page_y, iframe.rect) {
                continue;
            }
            let child_x = x - iframe.rect.x;
            let child_y = page_y - iframe.rect.y + iframe.scroll_y;
            if let Some(link) = iframe
                .page
                .links
                .iter()
                .find(|link| point_in_link(child_x, child_y, link))
            {
                return Some(IframeLinkHit {
                    src: iframe.src.clone(),
                    base_url: iframe.page.url.clone(),
                    href: link.href.clone(),
                    viewport_width: iframe.rect.width.ceil().max(1.0) as usize,
                    viewport_height: iframe.rect.height.ceil().max(1.0) as usize,
                });
            }
        }
        None
    }

    /// Returns the form control under a window coordinate, if visible.
    pub fn form_control_at_window_position(&self, x: f32, y: f32) -> Option<FormControlHitBox> {
        if !matches!(self.status, PageStatus::Loaded { .. }) {
            return None;
        }

        if !x.is_finite() || !y.is_finite() || y < CHROME_HEIGHT as f32 {
            return None;
        }

        let page_y = y - CHROME_HEIGHT as f32 + self.scroll_y;
        self.page.as_ref().and_then(|page| {
            page.form_controls
                .iter()
                .find(|control| point_in_rect(x, page_y, control.rect))
                .cloned()
        })
    }

    /// Goes back to the previous successful history entry when one exists.
    pub fn go_back<L: ResourceLoader>(&mut self, loader: &L) -> bool {
        let Some(index) = self.navigation.history_index else {
            return false;
        };
        if index == 0 {
            return false;
        }
        self.traverse_history(loader, index - 1)
    }

    /// Goes forward to the next successful history entry when one exists.
    pub fn go_forward<L: ResourceLoader>(&mut self, loader: &L) -> bool {
        let Some(index) = self.navigation.history_index else {
            return false;
        };
        let target_index = index + 1;
        if target_index >= self.navigation.history.len() {
            return false;
        }
        self.traverse_history(loader, target_index)
    }

    /// Reloads the current successful page.
    pub fn reload<L: ResourceLoader>(&mut self, loader: &L) -> bool {
        let Some(current_url) = self.navigation.current_url.clone() else {
            return false;
        };
        self.begin_navigation_to_url(current_url.clone(), PendingHistoryAction::Reload);
        self.load_pending_url(loader, &current_url, CacheMode::Refresh);
        true
    }

    /// Applies the result of a pending navigation.
    pub fn finish_navigation(&mut self, result: WebbyResult<RenderedPage>) {
        let generation = self.navigation.generation;
        self.finish_navigation_for_generation(generation, result);
    }

    /// Applies a navigation result only if the generation token is still active.
    pub fn finish_navigation_for_generation(
        &mut self,
        generation: u64,
        result: WebbyResult<RenderedPage>,
    ) {
        if generation != self.navigation.generation {
            self.navigation.lifecycle_diagnostics.push(format!(
                "late navigation result ignored generation={} active_generation={}",
                generation, self.navigation.generation
            ));
            self.save_active_tab();
            return;
        }
        self.navigation.lifecycle = NavigationLifecycle::Rendering;
        match result {
            Ok(mut page) => {
                self.apply_storage_actions_for_page(&mut page);
                self.chrome.set_address_input(page.url.to_string());
                self.navigation.current_url = Some(page.url.clone());
                self.navigation.pending_url = None;
                self.navigation.failed_url = None;
                self.navigation.active_page_generation = Some(generation);
                let action = match self.navigation.pending_history_action.take() {
                    Some(action) => action,
                    None => PendingHistoryAction::Push,
                };
                commit_history(&mut self.navigation, &page.url, action);
                self.scroll_y = 0.0;
                self.scroll_offsets.clear();
                self.focused_form_control = None;
                self.keyboard_focus = None;
                self.hovered_node_id = None;
                self.hover_target = None;
                self.form_values.clear();
                self.selected_inspection = None;
                self.status = PageStatus::Loaded {
                    url: page.url.to_string(),
                };
                self.navigation.lifecycle = NavigationLifecycle::Complete;
                self.timers.clear();
                self.active_transition = None;
                self.page = Some(page);
                self.update_find_matches();
                self.save_active_tab();
            }
            Err(error) => self.set_navigation_error(error),
        }
    }

    /// Adjusts page scroll and clamps it to valid content extents.
    pub fn scroll_by(&mut self, delta_y: f32) {
        if !delta_y.is_finite() {
            return;
        }
        self.scroll_y += delta_y;
        self.clamp_scroll();
        self.save_active_tab();
    }

    /// Routes wheel scrolling to the deepest visible nested container, then
    /// falls back to page-level scrolling.
    pub fn scroll_at_window_position(&mut self, x: f32, y: f32, delta_y: f32) {
        if !x.is_finite() || !y.is_finite() || !delta_y.is_finite() || y < CHROME_HEIGHT as f32 {
            return;
        }
        let page_y = y - CHROME_HEIGHT as f32 + self.scroll_y;
        let target = self.page.as_ref().and_then(|page| {
            page.layout
                .visible_scroll_containers(&self.scroll_offsets)
                .into_iter()
                .rev()
                .find(|container| {
                    point_in_rect(x, page_y, container.viewport) && container.max_scroll_y > 0.0
                })
        });
        let Some(target) = target else {
            self.scroll_by(delta_y);
            return;
        };
        let offset = self.scroll_offsets.entry(target.id).or_default();
        offset.y = (offset.y + delta_y).clamp(0.0, target.max_scroll_y);
        self.rerender_nested_scroll_offsets();
        self.save_active_tab();
    }

    fn rerender_nested_scroll_offsets(&mut self) {
        let Some(page) = self.page.as_mut() else {
            return;
        };
        let _ = apply_nested_scroll_offsets_to_page(page, &self.scroll_offsets, self.window_width);
    }

    /// Advances the deterministic animation clock for the active tab.
    pub fn tick_animations(&mut self, delta_ms: u32) -> bool {
        let Some(transition) = self.active_transition.as_mut() else {
            return false;
        };
        transition.elapsed_ms = transition.elapsed_ms.saturating_add(delta_ms);
        let finished =
            transition.elapsed_ms >= transition.duration_ms.saturating_add(transition.delay_ms);
        if finished {
            self.active_transition = None;
        }
        self.save_active_tab();
        true
    }

    /// Composes browser chrome plus current page/error content into an RGBA surface.
    pub fn compose_frame(&self) -> WebbyResult<Surface> {
        let mut surface = Surface::new(self.window_width, self.window_height, Color::WHITE)?;
        draw_chrome(&mut surface, self);

        match (&self.status, &self.page) {
            (PageStatus::Loaded { .. }, Some(page)) => {
                let animated = self.active_transition.as_ref().and_then(|transition| {
                    render_transition_surface(transition, page.surface.width, page.surface.height)
                        .ok()
                });
                let page_surface = animated.as_ref().unwrap_or(&page.surface);
                blit_page(&mut surface, page_surface, CHROME_HEIGHT, self.scroll_y);
                draw_form_value_overlays(
                    &mut surface,
                    page,
                    &self.form_values,
                    self.focused_form_control,
                    self.scroll_y,
                );
                draw_keyboard_focus_ring(&mut surface, page, self.keyboard_focus, self.scroll_y);
                if self.debug_overlay_enabled {
                    draw_debug_overlay(
                        &mut surface,
                        page,
                        self.scroll_y,
                        self.selected_inspection.as_ref(),
                    );
                }
            }
            (PageStatus::Loading { url }, _) => {
                draw_status_message(&mut surface, "Loading", url);
            }
            (PageStatus::Error { message }, _) => {
                draw_status_message(&mut surface, "Error", message);
            }
            (PageStatus::Startup, _) => {
                draw_status_message(&mut surface, "Webby", STARTUP_ADDRESS);
            }
            (PageStatus::Loaded { url }, None) => {
                draw_status_message(&mut surface, "Loaded", url);
            }
        }
        draw_shell_overlay(&mut surface, self);

        Ok(surface)
    }

    fn set_error(&mut self, error: WebbyError) {
        self.navigation.pending_url = None;
        self.navigation.pending_history_action = None;
        self.navigation.lifecycle = NavigationLifecycle::Failed;
        self.status = PageStatus::Error {
            message: error.to_string(),
        };
        self.active_transition = None;
        self.hovered_node_id = None;
        self.hover_target = None;
        self.keyboard_focus = None;
        self.focused_form_control = None;
        self.selected_inspection = None;
        self.clamp_scroll();
        self.save_active_tab();
    }

    fn set_navigation_error(&mut self, error: WebbyError) {
        self.navigation.failed_url = self
            .navigation
            .pending_url
            .take()
            .map(|url| url.to_string());
        self.navigation.pending_history_action = None;
        self.navigation.lifecycle = NavigationLifecycle::Failed;
        self.status = PageStatus::Error {
            message: error.to_string(),
        };
        self.active_transition = None;
        self.hovered_node_id = None;
        self.hover_target = None;
        self.keyboard_focus = None;
        self.focused_form_control = None;
        self.selected_inspection = None;
        self.clamp_scroll();
        self.save_active_tab();
    }

    fn set_auth_challenge(
        &mut self,
        url: url::Url,
        challenge: BasicAuthChallenge,
        diagnostics: Vec<String>,
        failed: bool,
    ) {
        self.navigation.failed_url = self
            .navigation
            .pending_url
            .take()
            .map(|url| url.to_string());
        self.navigation.pending_history_action = None;
        self.navigation.lifecycle = NavigationLifecycle::Failed;
        let origin =
            webby_security::origin_key(&url).unwrap_or_else(|_| url.origin().ascii_serialization());
        self.auth_diagnostics.extend(diagnostics);
        self.auth_diagnostics.push(challenge.diagnostic(&url));
        self.auth_challenge = Some(AuthChallengeState {
            url: url.clone(),
            origin,
            challenge: challenge.clone(),
        });
        self.status = PageStatus::Error {
            message: if failed {
                format!("authentication failed for {url}")
            } else {
                challenge.diagnostic(&url)
            },
        };
        self.active_transition = None;
        self.hovered_node_id = None;
        self.hover_target = None;
        self.keyboard_focus = None;
        self.focused_form_control = None;
        self.selected_inspection = None;
        self.clamp_scroll();
        self.save_active_tab();
    }

    fn set_target_error(&mut self, target: String, error: WebbyError) {
        self.navigation.pending_url = None;
        self.navigation.pending_history_action = None;
        self.navigation.failed_url = Some(target);
        self.navigation.lifecycle = NavigationLifecycle::Failed;
        self.status = PageStatus::Error {
            message: error.to_string(),
        };
        self.active_transition = None;
        self.hovered_node_id = None;
        self.hover_target = None;
        self.keyboard_focus = None;
        self.focused_form_control = None;
        self.selected_inspection = None;
        self.clamp_scroll();
        self.save_active_tab();
    }

    fn load_pending_url<L: ResourceLoader>(&mut self, loader: &L, url: &url::Url, mode: CacheMode) {
        self.navigation.lifecycle = NavigationLifecycle::LoadingMainResource;
        self.save_active_tab();
        let pipeline = PagePipeline::new(self.window_width, self.page_viewport_height())
            .with_javascript_enabled(self.javascript_enabled)
            .with_storage(
                self.storage_enabled,
                self.local_storage_entries_for_url(url),
                self.session_storage_entries_for_url(url),
            );
        let auth_credentials = self.basic_auth_credentials.clone();
        let result = pipeline.load_navigation_url_with_cache_and_cookies(
            loader,
            NavigationLoadContext {
                cache: CacheTiers {
                    memory: &self.resource_cache,
                    disk: self.disk_cache.as_ref(),
                },
                cookies: &mut self.cookie_jar,
                cookies_enabled: self.cookies_enabled,
                auth_credentials: &auth_credentials,
            },
            url,
            mode,
        );
        self.navigation.lifecycle = NavigationLifecycle::LoadingSubresources;
        self.save_active_tab();
        self.navigation.lifecycle = NavigationLifecycle::ExecutingScripts;
        self.save_active_tab();
        match result {
            Ok(NavigationLoad::Page(page)) => self.finish_navigation(Ok(*page)),
            Ok(NavigationLoad::Download {
                response,
                metadata,
                diagnostics,
            }) => self.complete_navigation_download(*response, metadata, diagnostics),
            Ok(NavigationLoad::AuthenticationRequired {
                url,
                challenge,
                diagnostics,
            }) => self.set_auth_challenge(url, challenge, diagnostics, false),
            Ok(NavigationLoad::AuthenticationFailed {
                url,
                challenge,
                diagnostics,
            }) => self.set_auth_challenge(url, challenge, diagnostics, true),
            Err(error) => self.finish_navigation(Err(error)),
        }
        self.apply_loaded_page_browser_actions(loader);
    }

    fn local_storage_entries_for_url(&self, url: &url::Url) -> Vec<webby_js::StorageEntry> {
        storage_entries_for_url(&self.local_storage, url)
    }

    fn session_storage_entries_for_url(&self, url: &url::Url) -> Vec<webby_js::StorageEntry> {
        storage_entries_for_url(&self.session_storage, url)
    }

    fn browser_api_context_for_page(&self, page: &RenderedPage) -> webby_js::BrowserApiContext {
        webby_js::BrowserApiContext {
            current_url: page.url.to_string(),
            fetch_resources: Vec::new(),
            local_storage: self.local_storage_entries_for_url(&page.url),
            session_storage: self.session_storage_entries_for_url(&page.url),
            storage_enabled: self.storage_enabled,
            storage_quota_bytes: webby_state::STORAGE_QUOTA_BYTES,
            viewport_width: self.window_width as u32,
            viewport_height: self.page_viewport_height() as u32,
            geometry: dom_geometry_from_layout(&page.layout),
        }
    }

    fn apply_storage_actions_for_page(&mut self, page: &mut RenderedPage) {
        let actions = std::mem::take(&mut page.storage_actions);
        let Ok(origin) = webby_state::storage_origin_key(&page.url) else {
            return;
        };
        for action in actions {
            if let Err(error) = apply_storage_action(
                &origin,
                action,
                &mut self.local_storage,
                &mut self.session_storage,
            ) {
                page.diagnostics.push(error.to_string());
            }
        }
    }

    fn traverse_history<L: ResourceLoader>(&mut self, loader: &L, target_index: usize) -> bool {
        let Some(target) = self.navigation.history.get(target_index).cloned() else {
            return false;
        };
        self.begin_navigation_to_url(
            target.clone(),
            PendingHistoryAction::Traverse { target_index },
        );
        self.load_pending_url(loader, &target, CacheMode::Refresh);
        true
    }

    fn resolve_href(&self, href: &str) -> WebbyResult<url::Url> {
        let Some(base) = &self.navigation.current_url else {
            return Err(WebbyError::Url {
                message: "cannot resolve link without a current page URL".to_string(),
            });
        };
        base.join(href).map_err(|error| WebbyError::Url {
            message: format!("invalid link target {href:?}: {error}"),
        })
    }

    fn resolve_script_url(&self, target: &str) -> WebbyResult<url::Url> {
        if let Some(base) = &self.navigation.current_url {
            return base.join(target).map_err(|error| WebbyError::Url {
                message: format!("invalid JavaScript navigation target {target:?}: {error}"),
            });
        }
        url::Url::parse(target).map_err(|error| WebbyError::Url {
            message: format!("invalid JavaScript navigation target {target:?}: {error}"),
        })
    }

    fn submit_form_for_control<L: ResourceLoader>(
        &mut self,
        loader: &L,
        control_id: usize,
        submitter_id: Option<usize>,
    ) -> bool {
        let Some(page) = &self.page else {
            return false;
        };
        let Some(control) = page
            .form_controls
            .iter()
            .find(|candidate| candidate.id == control_id)
            .cloned()
        else {
            return false;
        };
        let page_url = page.url.clone();
        let Some(form) = control.form.clone() else {
            self.set_target_error(
                page_url.to_string(),
                WebbyError::unsupported("form control is not inside a form"),
            );
            return true;
        };
        let before_url = self.navigation.current_url.clone();
        if let Some(default_prevented) = self.dispatch_event_for_form("submit", form.id) {
            self.apply_loaded_page_browser_actions(loader);
            if default_prevented || self.navigation.current_url != before_url {
                return true;
            }
        }
        let method = control
            .form
            .as_ref()
            .and_then(|form| form.method.as_deref())
            .unwrap_or("get")
            .to_string();
        if !method.eq_ignore_ascii_case("get") && !method.eq_ignore_ascii_case("post") {
            self.set_target_error(
                control
                    .form
                    .as_ref()
                    .and_then(|form| form.action.clone())
                    .unwrap_or_else(|| page_url.to_string()),
                WebbyError::unsupported(format!("form method {method:?} is unsupported")),
            );
            return true;
        }
        let Some(page) = &self.page else {
            return false;
        };
        let fields = form_fields_for_submission(page, &self.form_values, Some(&form), submitter_id);
        let action = form.action.as_deref();
        if method.eq_ignore_ascii_case("post") {
            let target = match resolve_form_action_url(&page_url, action) {
                Ok(target) => target,
                Err(error) => {
                    self.set_target_error(action.unwrap_or(page_url.as_str()).to_string(), error);
                    return true;
                }
            };
            let body = form_urlencoded_body(&fields);
            self.begin_navigation_to_url(target.clone(), PendingHistoryAction::Push);
            let result = loader
                .submit_form_urlencoded(&target, &[], body.as_bytes())
                .and_then(|response| self.render_form_response(response, loader));
            self.finish_navigation(result);
            self.apply_loaded_page_browser_actions(loader);
            return true;
        }
        let target = match resolve_get_form_url(&page_url, action, &fields) {
            Ok(target) => target,
            Err(error) => {
                self.set_target_error(action.unwrap_or(page_url.as_str()).to_string(), error);
                return true;
            }
        };
        self.navigate_to_url(loader, target);
        true
    }

    fn render_form_response<L: ResourceLoader>(
        &self,
        response: ResourceResponse,
        loader: &L,
    ) -> WebbyResult<RenderedPage> {
        let html = decode_text(&response.bytes, response.content_type.as_deref());
        PagePipeline::new(self.window_width, self.page_viewport_height())
            .with_javascript_enabled(self.javascript_enabled)
            .with_storage(
                self.storage_enabled,
                self.local_storage_entries_for_url(&response.final_url),
                self.session_storage_entries_for_url(&response.final_url),
            )
            .with_style_interaction(self.current_style_interaction())
            .render_html_with_loader(&html, response.final_url, loader)
    }

    fn navigate_iframe_link<L: ResourceLoader>(&mut self, loader: &L, hit: IframeLinkHit) -> bool {
        let target = match hit.base_url.join(&hit.href) {
            Ok(target) => target,
            Err(error) => {
                self.set_target_error(
                    hit.href,
                    WebbyError::Url {
                        message: format!("invalid iframe link target: {error}"),
                    },
                );
                return true;
            }
        };
        let Some(page) = self.page.clone() else {
            return false;
        };
        let pipeline = PagePipeline::new(hit.viewport_width, hit.viewport_height)
            .with_javascript_enabled(self.javascript_enabled)
            .with_style_interaction(self.current_style_interaction());
        let iframe_page = match pipeline.load_url(loader, &target) {
            Ok(page) => page,
            Err(error) => {
                self.set_target_error(target.to_string(), error);
                return true;
            }
        };
        let mut iframe_pages = iframe_page_map(&page.iframes);
        iframe_pages.insert(hit.src, iframe_page);
        let mut images = page.images.clone();
        insert_iframe_images(&mut images, &iframe_pages);
        match pipeline.render_mutated_document(MutatedDocumentRenderInput {
            document: page.document,
            url: page.url,
            images,
            iframes: iframe_pages,
            external_stylesheets: page.external_stylesheets,
            event_handlers: page.event_handlers,
            diagnostics: page.diagnostics,
            browser_actions: page.browser_actions,
            storage_actions: page.storage_actions,
            dirty: page.dirty,
        }) {
            Ok(updated) => {
                self.install_rerendered_page(updated);
                true
            }
            Err(error) => {
                self.set_error(error);
                true
            }
        }
    }

    fn select_radio_control(&mut self, selected: &FormControlHitBox) {
        let Some(page) = &self.page else {
            return;
        };
        let selected_form = selected.form.as_ref();
        for control in &page.form_controls {
            if control.control_type == FormControlType::Radio
                && control.name == selected.name
                && same_form(control.form.as_ref(), selected_form)
            {
                let value = if control.id == selected.id {
                    "true"
                } else {
                    "false"
                };
                self.form_values.insert(control.id, value.to_string());
            }
        }
    }

    fn reset_form_for_control(&mut self, control: &FormControlHitBox) {
        let Some(page) = &self.page else {
            return;
        };
        let control_form = control.form.as_ref();
        let reset_ids = page
            .form_controls
            .iter()
            .filter(|candidate| same_form(candidate.form.as_ref(), control_form))
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        for id in reset_ids {
            self.form_values.remove(&id);
        }
        self.rerender_loaded_page_for_current_viewport();
    }

    fn dispatch_event_at_window_position(
        &mut self,
        event_type: &str,
        x: f32,
        y: f32,
    ) -> Option<bool> {
        let page_y = y - CHROME_HEIGHT as f32 + self.scroll_y;
        let page = self.page.as_ref()?;
        let layout_box = find_deepest_box_at(&page.layout.root, x, page_y)?;
        let node_id = node_id_for_layout_box(layout_box)?;
        self.dispatch_dom_event(event_type, node_id)
    }

    fn dispatch_event_for_link(&mut self, event_type: &str, link: &LinkHitBox) -> Option<bool> {
        let node_id = link.node_id.or_else(|| {
            self.page
                .as_ref()
                .and_then(|page| find_link_node_id(&page.document.root, &link.href))
        })?;
        self.dispatch_dom_event(event_type, node_id)
    }

    fn dispatch_event_for_form_control(
        &mut self,
        event_type: &str,
        control_id: usize,
    ) -> Option<bool> {
        let node_id = self
            .page
            .as_ref()
            .and_then(|page| find_form_control_node_id(&page.document.root, control_id))?;
        self.dispatch_dom_event(event_type, node_id)
    }

    fn dispatch_input_event_for_control(&mut self, control_id: usize) -> Option<bool> {
        self.dispatch_event_for_form_control("input", control_id)
    }

    fn dispatch_event_for_form(&mut self, event_type: &str, form_id: usize) -> Option<bool> {
        let node_id = self
            .page
            .as_ref()
            .and_then(|page| {
                page.form_controls
                    .iter()
                    .filter_map(|control| control.form.as_ref())
                    .find(|form| form.id == form_id)
                    .map(|form| form.node_id)
            })
            .or_else(|| {
                self.page
                    .as_ref()
                    .and_then(|page| find_form_node_id(&page.document.root, form_id))
            })?;
        self.dispatch_dom_event(event_type, node_id)
    }

    fn dispatch_dom_event(&mut self, event_type: &str, target_node_id: u64) -> Option<bool> {
        let mut page = self.page.clone()?;
        if page.event_handlers.is_empty() {
            return None;
        }
        if !event_path_has_handler(
            &page.document.root,
            target_node_id,
            event_type,
            &page.event_handlers,
        ) {
            return None;
        }
        let browser = self.browser_api_context_for_page(&page);
        let report = match webby_js::dispatch_event_with_dom_and_browser(
            &mut page.document,
            &page.event_handlers,
            webby_js::EventDispatch {
                event_type: event_type.to_string(),
                target_node_id,
            },
            webby_js::ExecutionOptions {
                enabled: self.javascript_enabled,
                ..webby_js::ExecutionOptions::default()
            },
            &browser,
        ) {
            Ok(report) => report,
            Err(error) => {
                self.set_error(error);
                return Some(true);
            }
        };
        let mut diagnostics = page.diagnostics.clone();
        diagnostics.extend(report.diagnostics);
        let pipeline = PagePipeline::new(self.window_width, self.page_viewport_height())
            .with_javascript_enabled(false)
            .with_style_interaction(self.current_style_interaction());
        match pipeline.render_mutated_document(MutatedDocumentRenderInput {
            document: page.document,
            url: page.url,
            images: page.images,
            iframes: iframe_page_map(&page.iframes),
            external_stylesheets: page.external_stylesheets,
            event_handlers: page.event_handlers,
            diagnostics,
            browser_actions: report.browser_actions,
            storage_actions: report.storage_actions,
            dirty: report.dirty,
        }) {
            Ok(mut updated) => {
                self.apply_storage_actions_for_page(&mut updated);
                self.install_rerendered_page(updated);
                self.apply_timer_browser_actions_from_page();
                if let Some(current_url) = &self.navigation.current_url {
                    self.status = PageStatus::Loaded {
                        url: current_url.to_string(),
                    };
                }
                self.clamp_scroll();
                self.save_active_tab();
            }
            Err(error) => self.set_error(error),
        }
        Some(report.default_prevented)
    }

    fn apply_timer_browser_actions_from_page(&mut self) {
        let actions = self
            .page
            .as_mut()
            .map(|page| std::mem::take(&mut page.browser_actions))
            .unwrap_or_default();
        let mut deferred = Vec::new();
        for action in actions {
            match action {
                webby_js::BrowserAction::SetTimeout {
                    id,
                    callback,
                    delay_ms,
                } => self.schedule_timer(id, callback, delay_ms),
                webby_js::BrowserAction::ClearTimeout { id } => self.clear_timer(id),
                other => deferred.push(other),
            }
        }
        if let Some(page) = &mut self.page {
            page.browser_actions = deferred;
        }
    }

    fn apply_loaded_page_browser_actions<L: ResourceLoader>(&mut self, loader: &L) {
        let actions = self
            .page
            .as_mut()
            .map(|page| std::mem::take(&mut page.browser_actions))
            .unwrap_or_default();
        self.apply_browser_actions(loader, actions);
    }

    fn apply_browser_actions<L: ResourceLoader>(
        &mut self,
        loader: &L,
        actions: Vec<webby_js::BrowserAction>,
    ) {
        for action in actions {
            match action {
                webby_js::BrowserAction::Navigate { url, replace: _ } => {
                    let target = match self.resolve_script_url(&url) {
                        Ok(target) => target,
                        Err(error) => {
                            self.set_target_error(url, error);
                            return;
                        }
                    };
                    self.navigate_to_url(loader, target);
                    return;
                }
                webby_js::BrowserAction::HistoryBack => {
                    let _ = self.go_back(loader);
                    return;
                }
                webby_js::BrowserAction::HistoryForward => {
                    let _ = self.go_forward(loader);
                    return;
                }
                webby_js::BrowserAction::SetTimeout {
                    id,
                    callback,
                    delay_ms,
                } => self.schedule_timer(id, callback, delay_ms),
                webby_js::BrowserAction::ClearTimeout { id } => self.clear_timer(id),
            }
        }
        self.save_active_tab();
    }

    fn schedule_timer(&mut self, id: u32, callback: String, delay_ms: u32) {
        let Some(current_url) = &self.navigation.current_url else {
            return;
        };
        let Some(page_generation) = self.navigation.active_page_generation else {
            return;
        };
        self.timers.retain(|timer| timer.id != id);
        self.timers.push(BrowserTimer {
            id,
            callback,
            delay_ms,
            page_url: current_url.to_string(),
            page_generation,
        });
    }

    fn clear_timer(&mut self, id: u32) {
        self.timers.retain(|timer| timer.id != id);
    }

    /// Runs currently scheduled one-shot JavaScript timers in deterministic order.
    pub fn run_due_timers<L: ResourceLoader>(&mut self, loader: &L) -> bool {
        if !matches!(self.status, PageStatus::Loaded { .. }) {
            return false;
        }
        let Some(current_url) = self
            .navigation
            .current_url
            .as_ref()
            .map(url::Url::to_string)
        else {
            return false;
        };
        let Some(current_generation) = self.navigation.active_page_generation else {
            return false;
        };
        let timers = std::mem::take(&mut self.timers);
        let mut ran = false;
        for timer in timers {
            if timer.page_url != current_url || timer.page_generation != current_generation {
                self.navigation.lifecycle_diagnostics.push(format!(
                    "late timer ignored id={} page_generation={} active_generation={}",
                    timer.id, timer.page_generation, current_generation
                ));
                continue;
            }
            if !self.run_timer(loader, timer) {
                return ran;
            }
            ran = true;
            if self
                .navigation
                .current_url
                .as_ref()
                .map(url::Url::to_string)
                != Some(current_url.clone())
            {
                return ran;
            }
        }
        self.save_active_tab();
        ran
    }

    fn run_timer<L: ResourceLoader>(&mut self, loader: &L, timer: BrowserTimer) -> bool {
        let Some(mut page) = self.page.clone() else {
            return false;
        };
        let browser = self.browser_api_context_for_page(&page);
        let report = match webby_js::execute_timer_callback_with_dom(
            &mut page.document,
            &timer.callback,
            webby_js::ExecutionOptions {
                enabled: self.javascript_enabled,
                ..webby_js::ExecutionOptions::default()
            },
            &browser,
        ) {
            Ok(report) => report,
            Err(error) => {
                self.set_error(error);
                return false;
            }
        };
        let mut diagnostics = page.diagnostics.clone();
        diagnostics.extend(report.diagnostics);
        let pipeline = PagePipeline::new(self.window_width, self.page_viewport_height())
            .with_javascript_enabled(false)
            .with_style_interaction(self.current_style_interaction());
        match pipeline.render_mutated_document(MutatedDocumentRenderInput {
            document: page.document,
            url: page.url,
            images: page.images,
            iframes: iframe_page_map(&page.iframes),
            external_stylesheets: page.external_stylesheets,
            event_handlers: merge_event_handlers(page.event_handlers, report.event_handlers),
            diagnostics,
            browser_actions: report.browser_actions,
            storage_actions: report.storage_actions,
            dirty: report.dirty,
        }) {
            Ok(mut updated) => {
                self.apply_storage_actions_for_page(&mut updated);
                self.install_rerendered_page(updated);
                self.clamp_scroll();
                self.apply_loaded_page_browser_actions(loader);
                true
            }
            Err(error) => {
                self.set_error(error);
                false
            }
        }
    }

    fn clamp_scroll(&mut self) {
        let max_scroll = self
            .page
            .as_ref()
            .map(|page| (page.content_height - self.page_viewport_height() as f32).max(0.0))
            .unwrap_or(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    fn save_active_tab(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active_tab_index) else {
            return;
        };
        tab.chrome = self.chrome.clone();
        tab.navigation = self.navigation.clone();
        tab.status = self.status.clone();
        tab.page = self.page.clone();
        tab.scroll_y = self.scroll_y;
        tab.scroll_offsets = self.scroll_offsets.clone();
        tab.focused_form_control = self.focused_form_control;
        tab.keyboard_focus = self.keyboard_focus;
        tab.form_values = self.form_values.clone();
        tab.timers = self.timers.clone();
        tab.session_storage = self.session_storage.clone();
        tab.active_transition = self.active_transition.clone();
        tab.find_query = self.find_query.clone();
        tab.find_match_count = self.find_match_count;
        tab.find_active = self.find_active;
    }

    fn load_active_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab_index).cloned() else {
            return;
        };
        self.chrome = tab.chrome;
        self.navigation = tab.navigation;
        self.status = tab.status;
        self.page = tab.page;
        self.scroll_y = tab.scroll_y;
        self.scroll_offsets = tab.scroll_offsets;
        self.focused_form_control = tab.focused_form_control;
        self.keyboard_focus = tab.keyboard_focus;
        self.form_values = tab.form_values;
        self.timers = tab.timers;
        self.session_storage = tab.session_storage;
        self.active_transition = tab.active_transition;
        self.find_query = tab.find_query;
        self.find_match_count = tab.find_match_count;
        self.find_active = tab.find_active;
    }
}

fn apply_nested_scroll_offsets_to_page(
    page: &mut RenderedPage,
    offsets: &ScrollOffsets,
    surface_width: usize,
) -> WebbyResult<()> {
    let display_list = build_display_list_with_scroll_offsets(&page.layout, offsets);
    let surface_height = page.layout.scroll_height.ceil().max(1.0) as usize;
    page.surface = render_with_backend(
        &SoftwareRenderBackend,
        &display_list,
        surface_width,
        surface_height,
    )?;
    page.links = page.layout.visible_links(offsets);
    page.form_controls = page.layout.visible_form_controls(offsets);
    page.display_list = display_list;
    Ok(())
}

fn commit_history(
    navigation: &mut NavigationController,
    url: &url::Url,
    action: PendingHistoryAction,
) {
    match action {
        PendingHistoryAction::Push => {
            if let Some(index) = navigation.history_index {
                let keep_len = index.saturating_add(1);
                if keep_len < navigation.history.len() {
                    navigation.history.truncate(keep_len);
                }
            }
            navigation.history.push(url.clone());
            navigation.history_index = navigation.history.len().checked_sub(1);
        }
        PendingHistoryAction::Reload => {
            if let Some(index) = navigation.history_index
                && let Some(entry) = navigation.history.get_mut(index)
            {
                *entry = url.clone();
                return;
            }
            navigation.history.push(url.clone());
            navigation.history_index = navigation.history.len().checked_sub(1);
        }
        PendingHistoryAction::Traverse { target_index } => {
            if target_index < navigation.history.len() {
                navigation.history_index = Some(target_index);
            }
        }
    }
}

fn point_in_link(x: f32, y: f32, link: &LinkHitBox) -> bool {
    point_in_rect(x, y, link.rect)
}

fn point_in_rect(x: f32, y: f32, rect: webby_layout::Rect) -> bool {
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}

fn dom_geometry_from_layout(layout: &LayoutTree) -> Vec<webby_js::DomGeometry> {
    let mut geometry = Vec::new();
    collect_dom_geometry_from_box(&layout.root, &mut geometry);
    geometry.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    geometry.dedup_by(|left, right| left.node_id == right.node_id);
    geometry
}

fn collect_dom_geometry_from_box(
    layout_box: &webby_layout::LayoutBox,
    output: &mut Vec<webby_js::DomGeometry>,
) {
    if let Some(node_id) = layout_box_node_id(layout_box) {
        let rect = layout_box.dimensions.border_box();
        output.push(webby_js::DomGeometry {
            node_id: node_id.to_string(),
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        });
    }
    for item in &layout_box.contents {
        collect_dom_geometry_from_item(item, output);
    }
}

fn collect_dom_geometry_from_item(
    item: &webby_layout::LayoutItem,
    output: &mut Vec<webby_js::DomGeometry>,
) {
    match item {
        webby_layout::LayoutItem::Box(child) => collect_dom_geometry_from_box(child, output),
        webby_layout::LayoutItem::LineBox(line) => {
            for fragment in &line.fragments {
                if let webby_layout::InlineFragment::Box(child) = fragment {
                    collect_dom_geometry_from_box(child, output);
                }
            }
        }
        webby_layout::LayoutItem::Text(_) => {}
    }
}

fn layout_box_node_id(layout_box: &webby_layout::LayoutBox) -> Option<u64> {
    match &layout_box.kind {
        webby_layout::LayoutKind::Document => None,
        webby_layout::LayoutKind::Block { metadata, .. }
        | webby_layout::LayoutKind::Image { metadata, .. }
        | webby_layout::LayoutKind::FormControl { metadata, .. } => metadata.node_id,
    }
}

fn point_in_usize_rect(x: usize, y: usize, rect: (usize, usize, usize, usize)) -> bool {
    let (rect_x, rect_y, width, height) = rect;
    x >= rect_x
        && x < rect_x.saturating_add(width)
        && y >= rect_y
        && y < rect_y.saturating_add(height)
}

fn tab_action_at(state: &AppState, x: usize, y: usize) -> Option<ChromeAction> {
    let tab_count = state.tabs.len().max(1);
    let tab_width = chrome_tab_width(state.window_width, tab_count);
    if tab_width == 0 {
        return None;
    }
    let index = (x / tab_width).min(tab_count.saturating_sub(1));
    let tab_x = index.saturating_mul(tab_width);
    let tab_width = tab_render_width(state.window_width, tab_width, index, tab_count);
    if !point_in_usize_rect(x, y, (tab_x, 0, tab_width, TAB_STRIP_HEIGHT)) {
        return None;
    }
    if tab_width >= 36
        && x >= tab_x.saturating_add(tab_width.saturating_sub(18))
        && y >= 2
        && y < TAB_STRIP_HEIGHT.saturating_sub(2)
    {
        Some(ChromeAction::CloseTab(index))
    } else {
        Some(ChromeAction::SwitchTab(index))
    }
}

fn chrome_tab_width(surface_width: usize, tab_count: usize) -> usize {
    if tab_count == 0 {
        return surface_width;
    }
    (surface_width / tab_count).clamp(48, 160)
}

fn tab_render_width(
    surface_width: usize,
    tab_width: usize,
    index: usize,
    tab_count: usize,
) -> usize {
    let x = index.saturating_mul(tab_width);
    if index + 1 == tab_count {
        surface_width.saturating_sub(x)
    } else {
        tab_width.min(surface_width.saturating_sub(x))
    }
}

fn chrome_button_rects() -> [(ChromeAction, (usize, usize, usize, usize)); 4] {
    [
        (
            ChromeAction::Back,
            (12, CHROME_BUTTON_Y, CHROME_BUTTON_SIZE, CHROME_BUTTON_SIZE),
        ),
        (
            ChromeAction::Forward,
            (
                12 + (CHROME_BUTTON_SIZE + CHROME_BUTTON_GAP),
                CHROME_BUTTON_Y,
                CHROME_BUTTON_SIZE,
                CHROME_BUTTON_SIZE,
            ),
        ),
        (
            ChromeAction::Reload,
            (
                12 + (CHROME_BUTTON_SIZE + CHROME_BUTTON_GAP) * 2,
                CHROME_BUTTON_Y,
                CHROME_BUTTON_SIZE,
                CHROME_BUTTON_SIZE,
            ),
        ),
        (
            ChromeAction::BookmarkCurrentPage,
            (
                12 + (CHROME_BUTTON_SIZE + CHROME_BUTTON_GAP) * 3,
                CHROME_BUTTON_Y,
                CHROME_BUTTON_SIZE,
                CHROME_BUTTON_SIZE,
            ),
        ),
    ]
}

fn form_control_value(page: Option<&RenderedPage>, control_id: usize) -> String {
    page.and_then(|page| {
        page.form_controls
            .iter()
            .find(|control| control.id == control_id)
    })
    .map(|control| control.value.clone())
    .unwrap_or_default()
}

fn focused_control_is_text_editable(page: Option<&RenderedPage>, control_id: usize) -> bool {
    page.and_then(|page| {
        page.form_controls
            .iter()
            .find(|control| control.id == control_id)
    })
    .is_some_and(|control| {
        matches!(
            control.control_type,
            FormControlType::Text
                | FormControlType::Search
                | FormControlType::Password
                | FormControlType::Email
                | FormControlType::Textarea
        ) && !control.disabled
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyboardActivation {
    Enter,
    Space,
}

fn compare_focus_rects(left: Rect, right: Rect) -> std::cmp::Ordering {
    left.y
        .total_cmp(&right.y)
        .then_with(|| left.x.total_cmp(&right.x))
        .then_with(|| left.width.total_cmp(&right.width))
        .then_with(|| left.height.total_cmp(&right.height))
}

fn focus_target_key(target: KeyboardFocusTarget) -> (u8, usize) {
    match target {
        KeyboardFocusTarget::Link { index } => (0, index),
        KeyboardFocusTarget::FormControl { id } => (1, id),
    }
}

fn focusable_node_order(left: &AccessibleNode, right: &AccessibleNode) -> std::cmp::Ordering {
    compare_focus_rects(left.rect, right.rect).then_with(|| {
        let left_key = left
            .control_id
            .map(|id| (1, id))
            .or_else(|| left.href.as_ref().map(|_| (0, 0)))
            .unwrap_or((2, 0));
        let right_key = right
            .control_id
            .map(|id| (1, id))
            .or_else(|| right.href.as_ref().map(|_| (0, 0)))
            .unwrap_or((2, 0));
        left_key.cmp(&right_key)
    })
}

fn accessible_role_for_control(control_type: FormControlType) -> AccessibleRole {
    match control_type {
        FormControlType::Text
        | FormControlType::Search
        | FormControlType::Password
        | FormControlType::Email
        | FormControlType::Textarea
        | FormControlType::Select => AccessibleRole::Textbox,
        FormControlType::Checkbox => AccessibleRole::Checkbox,
        FormControlType::Radio => AccessibleRole::Radio,
        FormControlType::Submit | FormControlType::Button | FormControlType::Reset => {
            AccessibleRole::Button
        }
    }
}

fn accessible_name_for_control(
    page: &RenderedPage,
    control: &FormControlHitBox,
    node_id: Option<webby_dom::NodeId>,
) -> String {
    if let Some(node_id) = node_id
        && let Some(node) = page.document.find_node(node_id)
    {
        if let Some(name) = element_attribute(node, "aria-label")
            && !name.trim().is_empty()
        {
            return name.to_string();
        }
        if let Some(id) = element_attribute(node, "id")
            && let Some(label) = label_text_for_id(&page.document.root, id)
            && !label.trim().is_empty()
        {
            return label;
        }
        if let Some(label) = ancestor_label_text(&page.document.root, node_id)
            && !label.trim().is_empty()
        {
            return label;
        }
        if let Some(alt) = element_attribute(node, "alt")
            && !alt.trim().is_empty()
        {
            return alt.to_string();
        }
    }
    control
        .placeholder
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| control.value.clone())
}

fn element_attribute<'a>(node: &'a webby_dom::Node, name: &str) -> Option<&'a str> {
    let webby_dom::NodeKind::Element(element) = &node.kind else {
        return None;
    };
    element.attributes.get(name).map(String::as_str)
}

fn label_text_for_id(node: &webby_dom::Node, id: &str) -> Option<String> {
    if let webby_dom::NodeKind::Element(element) = &node.kind
        && element.tag_name == "label"
        && element
            .attributes
            .get("for")
            .is_some_and(|candidate| candidate == id)
    {
        return Some(node_text_content(node));
    }
    node.tree_children()
        .find_map(|child| label_text_for_id(child, id))
}

fn ancestor_label_text(node: &webby_dom::Node, target_id: webby_dom::NodeId) -> Option<String> {
    if let webby_dom::NodeKind::Element(element) = &node.kind
        && element.tag_name == "label"
        && node_contains_id(node, target_id)
    {
        return Some(node_text_content(node));
    }
    node.tree_children()
        .find_map(|child| ancestor_label_text(child, target_id))
}

fn node_contains_id(node: &webby_dom::Node, target_id: webby_dom::NodeId) -> bool {
    node.id == target_id
        || node
            .tree_children()
            .any(|child| node_contains_id(child, target_id))
}

fn node_text_content(node: &webby_dom::Node) -> String {
    match &node.kind {
        webby_dom::NodeKind::Text(text) => text.clone(),
        webby_dom::NodeKind::Element(element) if element.tag_name == "img" => {
            element.attributes.get("alt").cloned().unwrap_or_default()
        }
        webby_dom::NodeKind::Document | webby_dom::NodeKind::Element(_) => {
            let mut text = String::new();
            for child in node.render_children() {
                text.push_str(&node_text_content(child));
            }
            text
        }
    }
}

fn find_match_count(page: Option<&RenderedPage>, query: &str) -> usize {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 0;
    }
    page.map_or(0, |page| {
        webby_html::extract_visible_text(&page.document)
            .to_lowercase()
            .matches(&query)
            .count()
    })
}

fn merge_event_handlers(
    mut existing: Vec<webby_js::EventHandler>,
    mut added: Vec<webby_js::EventHandler>,
) -> Vec<webby_js::EventHandler> {
    existing.append(&mut added);
    existing
}

fn bookmarks_page_html(bookmarks: &[Bookmark]) -> String {
    let mut html = String::from(
        "<!doctype html><html><head><title>Webby Bookmarks</title><style>body{font-family:sans;margin:16px}li{margin:4px 0}</style></head><body><h1>Bookmarks</h1><ul>",
    );
    for bookmark in bookmarks {
        let escaped_url = escape_html(&bookmark.url);
        html.push_str("<li><a href=\"");
        html.push_str(&escaped_url);
        html.push_str("\">");
        html.push_str(&escaped_url);
        html.push_str("</a></li>");
    }
    html.push_str("</ul></body></html>");
    html
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::new();
    for character in input.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            character => escaped.push(character),
        }
    }
    escaped
}

fn form_fields_for_submission(
    page: &RenderedPage,
    values: &BTreeMap<usize, String>,
    form: Option<&webby_layout::FormMetadata>,
    submitter_id: Option<usize>,
) -> Vec<FormField> {
    page.form_controls
        .iter()
        .filter(|control| same_form(control.form.as_ref(), form))
        .filter_map(|control| successful_control_field(control, values, submitter_id))
        .collect()
}

fn same_form(
    left: Option<&webby_layout::FormMetadata>,
    right: Option<&webby_layout::FormMetadata>,
) -> bool {
    left.map(|form| form.id) == right.map(|form| form.id)
}

fn successful_control_field(
    control: &FormControlHitBox,
    values: &BTreeMap<usize, String>,
    submitter_id: Option<usize>,
) -> Option<FormField> {
    if control.disabled {
        return None;
    }
    let name = control.name.as_ref()?.clone();
    match control.control_type {
        FormControlType::Text
        | FormControlType::Search
        | FormControlType::Password
        | FormControlType::Email
        | FormControlType::Textarea
        | FormControlType::Select => Some(FormField {
            name,
            value: values
                .get(&control.id)
                .cloned()
                .unwrap_or_else(|| control.value.clone()),
        }),
        FormControlType::Checkbox | FormControlType::Radio => {
            control_checked_state(control, values).then(|| FormField {
                name,
                value: control.value.clone(),
            })
        }
        FormControlType::Submit => (submitter_id == Some(control.id)).then(|| FormField {
            name,
            value: control.value.clone(),
        }),
        FormControlType::Button | FormControlType::Reset => None,
    }
}

fn control_checked_state(control: &FormControlHitBox, values: &BTreeMap<usize, String>) -> bool {
    values
        .get(&control.id)
        .map(|value| value == "true")
        .unwrap_or(control.checked)
}

fn next_select_value(
    control: &FormControlHitBox,
    values: &BTreeMap<usize, String>,
) -> Option<String> {
    let enabled = control
        .options
        .iter()
        .filter(|option| !option.disabled)
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return None;
    }
    let current = values
        .get(&control.id)
        .map(String::as_str)
        .unwrap_or(control.value.as_str());
    let next_index = enabled
        .iter()
        .position(|option| option.value == current)
        .map(|index| (index + 1) % enabled.len())
        .unwrap_or(0);
    enabled.get(next_index).map(|option| option.value.clone())
}

fn node_id_for_layout_box(layout_box: &LayoutBox) -> Option<u64> {
    match &layout_box.kind {
        LayoutKind::Document => None,
        LayoutKind::Block { metadata, .. }
        | LayoutKind::Image { metadata, .. }
        | LayoutKind::FormControl { metadata, .. } => metadata.node_id,
    }
}

fn event_path_has_handler(
    root: &webby_dom::Node,
    target_node_id: u64,
    event_type: &str,
    handlers: &[webby_js::EventHandler],
) -> bool {
    let mut path = Vec::new();
    if !collect_dom_path(root, target_node_id, &mut path) {
        return false;
    }
    path.iter().any(|node_id| {
        handlers.iter().any(|handler| {
            handler.event_type == event_type && handler.id.parse::<u64>().ok() == Some(*node_id)
        })
    })
}

fn collect_dom_path(node: &webby_dom::Node, target_node_id: u64, path: &mut Vec<u64>) -> bool {
    path.push(node.id);
    if node.id == target_node_id {
        return true;
    }
    if node
        .tree_children()
        .any(|child| collect_dom_path(child, target_node_id, path))
    {
        return true;
    }
    let _ = path.pop();
    false
}

fn find_link_node_id(node: &webby_dom::Node, href: &str) -> Option<u64> {
    if let webby_dom::NodeKind::Element(element) = &node.kind
        && element.tag_name == "a"
        && element
            .attributes
            .get("href")
            .is_some_and(|candidate| candidate == href)
    {
        return Some(node.id);
    }
    node.tree_children()
        .find_map(|child| find_link_node_id(child, href))
}

fn find_form_control_node_id(node: &webby_dom::Node, control_id: usize) -> Option<u64> {
    let mut index = 0;
    find_form_control_node_id_inner(node, control_id, &mut index)
}

fn find_form_control_node_id_inner(
    node: &webby_dom::Node,
    control_id: usize,
    index: &mut usize,
) -> Option<u64> {
    if let webby_dom::NodeKind::Element(element) = &node.kind
        && matches!(
            element.tag_name.as_str(),
            "input" | "button" | "select" | "textarea"
        )
    {
        let current = *index;
        *index = index.saturating_add(1);
        if current == control_id {
            return Some(node.id);
        }
    }
    node.tree_children()
        .find_map(|child| find_form_control_node_id_inner(child, control_id, index))
}

fn find_form_node_id(node: &webby_dom::Node, form_id: usize) -> Option<u64> {
    let mut index = 0;
    find_form_node_id_inner(node, form_id, &mut index)
}

fn find_form_node_id_inner(
    node: &webby_dom::Node,
    form_id: usize,
    index: &mut usize,
) -> Option<u64> {
    if let webby_dom::NodeKind::Element(element) = &node.kind
        && element.tag_name == "form"
    {
        let current = *index;
        *index = index.saturating_add(1);
        if current == form_id {
            return Some(node.id);
        }
    }
    node.tree_children()
        .find_map(|child| find_form_node_id_inner(child, form_id, index))
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Pipeline coordinator for URL/resource bytes through rendered pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagePipeline {
    viewport_width: usize,
    viewport_height: usize,
    render_backend: SoftwareRenderBackend,
    javascript_enabled: bool,
    storage_enabled: bool,
    local_storage: Vec<webby_js::StorageEntry>,
    session_storage: Vec<webby_js::StorageEntry>,
    style_interaction: webby_style::StyleInteraction,
    iframe_depth: usize,
}

struct CookiePipelineLoader<'a, L> {
    loader: &'a L,
    cookies: RefCell<&'a mut CookieJar>,
    cookies_enabled: bool,
    auth_credentials: &'a BTreeMap<String, BasicCredentials>,
    diagnostics: RefCell<Vec<String>>,
}

struct InitialDocumentRenderInput {
    document: webby_dom::Document,
    url: url::Url,
    images: ImageMap,
    iframes: BTreeMap<String, RenderedPage>,
    external_stylesheets: Vec<webby_css::Stylesheet>,
    diagnostics: Vec<String>,
    fetch_resources: Vec<webby_js::FetchResource>,
    loaded_scripts: webby_script::LoadedScripts,
}

struct DocumentScriptExecutionInput {
    url: url::Url,
    viewport_width: usize,
    viewport_height: usize,
    javascript_enabled: bool,
    storage_enabled: bool,
    local_storage: Vec<webby_js::StorageEntry>,
    session_storage: Vec<webby_js::StorageEntry>,
    fetch_resources: Vec<webby_js::FetchResource>,
    scripts: Vec<webby_js::ScriptSource>,
}

struct PreloadedJavascriptResources {
    resources: Vec<webby_js::FetchResource>,
    diagnostics: Vec<String>,
}

struct LoadedIframes {
    pages: BTreeMap<String, RenderedPage>,
    diagnostics: Vec<String>,
}

struct NavigationLoadContext<'a> {
    cache: CacheTiers<'a>,
    cookies: &'a mut CookieJar,
    cookies_enabled: bool,
    auth_credentials: &'a BTreeMap<String, BasicCredentials>,
}

enum NavigationLoad {
    Page(Box<RenderedPage>),
    Download {
        response: Box<ResourceResponse>,
        metadata: DownloadMetadata,
        diagnostics: Vec<String>,
    },
    AuthenticationRequired {
        url: url::Url,
        challenge: BasicAuthChallenge,
        diagnostics: Vec<String>,
    },
    AuthenticationFailed {
        url: url::Url,
        challenge: BasicAuthChallenge,
        diagnostics: Vec<String>,
    },
}

fn decode_html_response(response: &ResourceResponse) -> WebbyResult<String> {
    match response.navigation_disposition() {
        NavigationResponseDisposition::RenderHtml => Ok(decode_text(
            &response.bytes,
            response.content_type.as_deref(),
        )),
        NavigationResponseDisposition::Download(metadata) => Err(WebbyError::unsupported(format!(
            "top-level response {} must be downloaded: {}",
            response.final_url, metadata.reason
        ))),
    }
}

fn request_headers_with_auth(
    mut headers: Vec<(String, String)>,
    url: &url::Url,
    credentials: &BTreeMap<String, BasicCredentials>,
) -> Vec<(String, String)> {
    if let Ok(origin) = webby_security::origin_key(url)
        && let Some(credentials) = credentials.get(&origin)
        && !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
    {
        headers.push(basic_auth_header(credentials));
    }
    headers
}

fn request_has_authorization(headers: &[(String, String)]) -> bool {
    headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
}

fn auth_response_diagnostics(
    mut diagnostics: Vec<String>,
    response: &ResourceResponse,
    request_headers: &[(String, String)],
) -> Vec<String> {
    diagnostics.push(format!(
        "authentication diagnostic: challenge url={} request_headers={:?}",
        response.final_url,
        webby_net::redact_request_headers(request_headers)
    ));
    diagnostics
}

impl<L: ResourceLoader> CookiePipelineLoader<'_, L> {
    fn take_diagnostics(&self) -> Vec<String> {
        std::mem::take(&mut *self.diagnostics.borrow_mut())
    }
}

impl<L: ResourceLoader> ResourceLoader for CookiePipelineLoader<'_, L> {
    fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
        let headers = if self.cookies_enabled {
            self.cookies.borrow().request_headers(url)
        } else {
            Vec::new()
        };
        let headers = request_headers_with_auth(headers, url, self.auth_credentials);
        let response = self.loader.load_with_headers(url, &headers)?;
        if self.cookies_enabled {
            self.diagnostics.borrow_mut().extend(
                self.cookies
                    .borrow_mut()
                    .store_from_headers(&response.final_url, &response.headers),
            );
        }
        Ok(response)
    }

    fn submit_form_urlencoded(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
        body: &[u8],
    ) -> WebbyResult<ResourceResponse> {
        let mut request_headers = if self.cookies_enabled {
            self.cookies.borrow().request_headers(url)
        } else {
            Vec::new()
        };
        request_headers = request_headers_with_auth(request_headers, url, self.auth_credentials);
        request_headers.extend(headers.iter().cloned());
        let response = self
            .loader
            .submit_form_urlencoded(url, &request_headers, body)?;
        if self.cookies_enabled {
            self.diagnostics.borrow_mut().extend(
                self.cookies
                    .borrow_mut()
                    .store_from_headers(&response.final_url, &response.headers),
            );
        }
        Ok(response)
    }
}

impl PagePipeline {
    /// Creates a page pipeline for the page viewport below chrome.
    pub fn new(viewport_width: usize, viewport_height: usize) -> Self {
        Self {
            viewport_width: viewport_width.max(1),
            viewport_height: viewport_height.max(1),
            render_backend: SoftwareRenderBackend,
            javascript_enabled: true,
            storage_enabled: true,
            local_storage: Vec::new(),
            session_storage: Vec::new(),
            style_interaction: webby_style::StyleInteraction::new(),
            iframe_depth: 0,
        }
    }

    /// Sets whether inline JavaScript executes during this pipeline run.
    pub fn with_javascript_enabled(mut self, enabled: bool) -> Self {
        self.javascript_enabled = enabled;
        self
    }

    /// Supplies Web Storage snapshots for script execution.
    pub fn with_storage(
        mut self,
        enabled: bool,
        local_storage: Vec<webby_js::StorageEntry>,
        session_storage: Vec<webby_js::StorageEntry>,
    ) -> Self {
        self.storage_enabled = enabled;
        self.local_storage = local_storage;
        self.session_storage = session_storage;
        self
    }

    /// Supplies interaction-dependent CSS state such as `:hover` and `:focus`.
    pub fn with_style_interaction(mut self, interaction: webby_style::StyleInteraction) -> Self {
        self.style_interaction = interaction;
        self
    }

    /// Returns the deterministic renderer backend used by this pipeline.
    pub fn render_backend_name(&self) -> &'static str {
        self.render_backend.name()
    }

    fn with_iframe_depth(mut self, depth: usize) -> Self {
        self.iframe_depth = depth;
        self
    }

    /// Loads and renders a URL using Webby's engine crates.
    pub fn load_url<L: ResourceLoader>(
        &self,
        loader: &L,
        url: &url::Url,
    ) -> WebbyResult<RenderedPage> {
        let response = loader.load(url)?;
        let html = decode_html_response(&response)?;
        self.render_html_with_loader(&html, response.final_url, loader)
    }

    /// Loads and renders a URL through Webby's shared byte cache.
    pub fn load_url_with_cache<L: ResourceLoader>(
        &self,
        loader: &L,
        cache: &ResourceCache,
        url: &url::Url,
        mode: CacheMode,
    ) -> WebbyResult<RenderedPage> {
        let cached_loader = webby_cache::CachedResourceLoader::new(loader, cache);
        let response = cached_loader.load_with_mode(url, mode)?;
        let html = decode_html_response(&response)?;
        let mut page =
            self.render_html_with_loader(&html, response.final_url.clone(), &cached_loader)?;
        let cache_diagnostics = cached_loader
            .take_diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.format());
        page.diagnostics.splice(0..0, cache_diagnostics);
        Ok(page)
    }

    /// Loads and renders a URL through cache and an optional cookie jar.
    pub fn load_url_with_cache_and_cookies<L: ResourceLoader>(
        &self,
        loader: &L,
        cache: CacheTiers<'_>,
        cookies: &mut CookieJar,
        cookies_enabled: bool,
        url: &url::Url,
        mode: CacheMode,
    ) -> WebbyResult<RenderedPage> {
        let mut cached_loader = webby_cache::CachedResourceLoader::new(loader, cache.memory);
        if let Some(disk_cache) = cache.disk {
            cached_loader = cached_loader.with_disk_cache(disk_cache);
        }
        let headers = if cookies_enabled {
            cookies.request_headers(url)
        } else {
            Vec::new()
        };
        let response = cached_loader.load_with_headers_and_mode(url, &headers, mode)?;
        let mut cookie_diagnostics = if cookies_enabled {
            cookies.store_from_headers(&response.final_url, &response.headers)
        } else {
            Vec::new()
        };
        let html = decode_html_response(&response)?;
        let empty_auth = BTreeMap::new();
        let cookie_loader = CookiePipelineLoader {
            loader: &cached_loader,
            cookies: RefCell::new(cookies),
            cookies_enabled,
            auth_credentials: &empty_auth,
            diagnostics: RefCell::new(Vec::new()),
        };
        let mut page =
            self.render_html_with_loader(&html, response.final_url.clone(), &cookie_loader)?;
        let cache_diagnostics = cached_loader
            .take_diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.format());
        let mut diagnostics = cache_diagnostics.collect::<Vec<_>>();
        diagnostics.append(&mut cookie_diagnostics);
        diagnostics.extend(cookie_loader.take_diagnostics());
        page.diagnostics.splice(0..0, diagnostics);
        Ok(page)
    }

    fn load_navigation_url_with_cache_and_cookies<L: ResourceLoader>(
        &self,
        loader: &L,
        context: NavigationLoadContext<'_>,
        url: &url::Url,
        mode: CacheMode,
    ) -> WebbyResult<NavigationLoad> {
        let mut cached_loader =
            webby_cache::CachedResourceLoader::new(loader, context.cache.memory);
        if let Some(disk_cache) = context.cache.disk {
            cached_loader = cached_loader.with_disk_cache(disk_cache);
        }
        let headers = if context.cookies_enabled {
            context.cookies.request_headers(url)
        } else {
            Vec::new()
        };
        let headers = request_headers_with_auth(headers, url, context.auth_credentials);
        let mode = if request_has_authorization(&headers) {
            CacheMode::Refresh
        } else {
            mode
        };
        let response = cached_loader.load_with_headers_and_mode(url, &headers, mode)?;
        let mut diagnostics = cached_loader
            .take_diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.format())
            .collect::<Vec<_>>();
        if context.cookies_enabled {
            diagnostics.extend(
                context
                    .cookies
                    .store_from_headers(&response.final_url, &response.headers),
            );
        }
        if let Some(challenge) = response.basic_auth_challenge() {
            let diagnostics = auth_response_diagnostics(diagnostics, &response, &headers);
            return if request_has_authorization(&headers) {
                Ok(NavigationLoad::AuthenticationFailed {
                    url: response.final_url.clone(),
                    challenge,
                    diagnostics,
                })
            } else {
                Ok(NavigationLoad::AuthenticationRequired {
                    url: response.final_url.clone(),
                    challenge,
                    diagnostics,
                })
            };
        }
        match response.navigation_disposition() {
            NavigationResponseDisposition::RenderHtml => {
                let html = decode_text(&response.bytes, response.content_type.as_deref());
                let cookie_loader = CookiePipelineLoader {
                    loader: &cached_loader,
                    cookies: RefCell::new(context.cookies),
                    cookies_enabled: context.cookies_enabled,
                    auth_credentials: context.auth_credentials,
                    diagnostics: RefCell::new(Vec::new()),
                };
                let mut page =
                    self.render_html_with_loader(&html, response.final_url, &cookie_loader)?;
                diagnostics.extend(
                    cached_loader
                        .take_diagnostics()
                        .into_iter()
                        .map(|diagnostic| diagnostic.format()),
                );
                diagnostics.extend(cookie_loader.take_diagnostics());
                page.diagnostics.splice(0..0, diagnostics);
                Ok(NavigationLoad::Page(Box::new(page)))
            }
            NavigationResponseDisposition::Download(metadata) => Ok(NavigationLoad::Download {
                response: Box::new(response),
                metadata,
                diagnostics,
            }),
        }
    }

    /// Parses, styles, lays out, display-lists, and renders an HTML string.
    ///
    /// This direct string path intentionally does not fetch linked external
    /// stylesheets, external scripts, or JavaScript fetch/XHR resources. Use
    /// [`Self::render_html_with_loader`] or [`Self::load_url`] when resource
    /// loading should be part of the render pipeline.
    pub fn render_html(&self, html: &str, url: url::Url) -> WebbyResult<RenderedPage> {
        self.render_html_with_images(html, url, &ImageMap::empty())
    }

    /// Parses and renders HTML while loading referenced images through a caller
    /// supplied resource loader.
    pub fn render_html_with_loader<L: ResourceLoader>(
        &self,
        html: &str,
        url: url::Url,
        loader: &L,
    ) -> WebbyResult<RenderedPage> {
        let parsed = webby_html::parse_document_with_diagnostics(html)?;
        let document = parsed.document;
        let mut diagnostics = parsed
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.to_string())
            .collect::<Vec<_>>();
        diagnostics.extend(collect_security_diagnostics(&document, &url));
        let decoded = webby_image::load_images(&document, &url, loader);
        let mut images = webby_image::to_layout_image_map(&decoded);
        let iframe_load = self.load_iframe_pages(&document, &url, loader);
        diagnostics.extend(iframe_load.diagnostics);
        insert_iframe_images(&mut images, &iframe_load.pages);
        let loaded_stylesheets =
            webby_stylesheet::load_external_stylesheets(&document, &url, loader);
        let loaded_scripts = webby_script::load_document_scripts(&document, &url, Some(loader));
        let fetch_resources =
            preload_javascript_request_resources(&loaded_scripts.scripts, &url, loader);
        diagnostics.extend(loaded_stylesheets.diagnostics);
        diagnostics.extend(fetch_resources.diagnostics);
        self.render_document_with_images(InitialDocumentRenderInput {
            document,
            url,
            images,
            iframes: iframe_load.pages,
            external_stylesheets: loaded_stylesheets.stylesheets,
            diagnostics,
            fetch_resources: fetch_resources.resources,
            loaded_scripts,
        })
    }

    fn load_iframe_pages<L: ResourceLoader>(
        &self,
        document: &webby_dom::Document,
        base_url: &url::Url,
        loader: &L,
    ) -> LoadedIframes {
        let mut pages = BTreeMap::new();
        let mut diagnostics = Vec::new();
        if self.iframe_depth >= 1 {
            if !webby_html::collect_iframes(document).is_empty() {
                diagnostics
                    .push("iframe nesting diagnostic: nested iframes are not loaded".to_string());
            }
            return LoadedIframes { pages, diagnostics };
        }

        for iframe in webby_html::collect_iframes(document) {
            if iframe.sandbox.is_some() {
                diagnostics.push(
                    "iframe sandbox diagnostic: sandbox is observed but not enforced".to_string(),
                );
            }
            let Some(src) = iframe.src.as_deref() else {
                diagnostics.push("iframe without src uses placeholder".to_string());
                continue;
            };
            if pages.contains_key(src) {
                continue;
            }
            let target = match base_url.join(src) {
                Ok(target) => target,
                Err(error) => {
                    diagnostics.push(format!("iframe src {src:?} failed to resolve: {error}"));
                    continue;
                }
            };
            if let Some(message) = webby_security::mixed_content_diagnostic(base_url, &target) {
                diagnostics.push(message);
            }
            match self.load_iframe_page(loader, &target, &iframe) {
                Ok(page) => {
                    pages.insert(src.to_string(), page);
                }
                Err(error) => diagnostics.push(format!("iframe {target} failed to load: {error}")),
            }
        }
        LoadedIframes { pages, diagnostics }
    }

    fn load_iframe_page<L: ResourceLoader>(
        &self,
        loader: &L,
        target: &url::Url,
        iframe: &webby_html::IframeElement,
    ) -> WebbyResult<RenderedPage> {
        let response = loader.load(target)?;
        let html = decode_text(&response.bytes, response.content_type.as_deref());
        let (width, height) = iframe_viewport_size(iframe);
        PagePipeline::new(width, height)
            .with_javascript_enabled(self.javascript_enabled)
            .with_storage(false, Vec::new(), Vec::new())
            .with_iframe_depth(self.iframe_depth.saturating_add(1))
            .render_html_with_loader(&html, response.final_url, loader)
    }

    fn render_html_with_images(
        &self,
        html: &str,
        url: url::Url,
        images: &ImageMap,
    ) -> WebbyResult<RenderedPage> {
        let parsed = webby_html::parse_document_with_diagnostics(html)?;
        let document = parsed.document;
        let mut diagnostics = parsed
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.to_string())
            .collect::<Vec<_>>();
        diagnostics.extend(collect_security_diagnostics(&document, &url));
        let loaded_scripts = webby_script::load_document_scripts::<webby_net::DefaultResourceLoader>(
            &document, &url, None,
        );
        self.render_document_with_images(InitialDocumentRenderInput {
            document,
            url,
            images: images.clone(),
            iframes: BTreeMap::new(),
            external_stylesheets: Vec::new(),
            diagnostics,
            fetch_resources: Vec::new(),
            loaded_scripts,
        })
    }

    fn render_document_with_images(
        &self,
        input: InitialDocumentRenderInput,
    ) -> WebbyResult<RenderedPage> {
        let InitialDocumentRenderInput {
            mut document,
            url,
            images,
            iframes,
            external_stylesheets,
            mut diagnostics,
            fetch_resources,
            loaded_scripts,
        } = input;
        diagnostics.extend(resource_hint_diagnostics(&document));
        diagnostics.extend(loaded_scripts.diagnostics);
        let script_report = execute_document_scripts(
            &mut document,
            DocumentScriptExecutionInput {
                url: url.clone(),
                viewport_width: self.viewport_width,
                viewport_height: self.viewport_height,
                javascript_enabled: self.javascript_enabled,
                storage_enabled: self.storage_enabled,
                local_storage: self.local_storage.clone(),
                session_storage: self.session_storage.clone(),
                fetch_resources,
                scripts: loaded_scripts.scripts,
            },
        )?;
        diagnostics.extend(script_report.diagnostics);
        self.render_mutated_document(MutatedDocumentRenderInput {
            document,
            url,
            images,
            iframes,
            external_stylesheets,
            event_handlers: script_report.event_handlers,
            diagnostics,
            browser_actions: script_report.browser_actions,
            storage_actions: script_report.storage_actions,
            dirty: script_report.dirty,
        })
    }

    /// Renders an already parsed/mutated document without re-running scripts.
    pub fn render_mutated_document(
        &self,
        input: MutatedDocumentRenderInput,
    ) -> WebbyResult<RenderedPage> {
        let MutatedDocumentRenderInput {
            document,
            url,
            images,
            iframes,
            external_stylesheets,
            event_handlers,
            mut diagnostics,
            browser_actions,
            storage_actions,
            dirty,
        } = input;
        let stylesheet = webby_style::compose_document_stylesheet(&document, &external_stylesheets);
        append_unique_diagnostics(&mut diagnostics, large_document_diagnostics(&document));
        diagnostics.extend(stylesheet.diagnostics.iter().map(|diagnostic| {
            format!(
                "CSS diagnostic at byte {}: {}",
                diagnostic.offset, diagnostic.message
            )
        }));
        let viewport = Viewport::new(self.viewport_width as f32, self.viewport_height as f32)?;
        let style_context = webby_style::StyleContext::new(viewport.width, viewport.height)
            .with_interaction(self.style_interaction.clone());
        let styled =
            webby_style::style_document_with_css_and_context(&document, &stylesheet, style_context);
        let transition = page_transition_spec(&styled);
        let layout = webby_layout::layout_tree_with_images(&styled, viewport, &images)?;
        let iframe_metadata = webby_html::collect_iframes(&document);
        let iframes = iframe_contexts_for_layout(&layout.root, &iframes, &iframe_metadata);
        let display_list = build_display_list(&layout);
        let surface_height = layout.scroll_height.ceil().max(1.0) as usize;
        let content_height = layout.scroll_height;
        let links = layout.visible_links(&ScrollOffsets::new());
        let form_controls = layout.visible_form_controls(&ScrollOffsets::new());
        let surface = render_with_backend(
            &self.render_backend,
            &display_list,
            self.viewport_width,
            surface_height,
        )?;

        Ok(RenderedPage {
            url,
            title: webby_html::extract_document_title(&document),
            favicon_href: webby_html::collect_icon_links(&document)
                .into_iter()
                .next()
                .map(|icon| icon.href),
            document,
            event_handlers,
            images,
            external_stylesheets,
            display_list,
            layout,
            surface,
            content_height,
            links,
            form_controls,
            iframes,
            diagnostics,
            browser_actions,
            storage_actions,
            dirty,
            transition,
        })
    }
}

fn large_document_diagnostics(document: &webby_dom::Document) -> Vec<String> {
    let count = count_dom_nodes(&document.root);
    if count <= LARGE_DOCUMENT_NODE_DIAGNOSTIC_THRESHOLD {
        return Vec::new();
    }
    vec![format!(
        "performance diagnostic: document has {count} DOM nodes; advisory threshold is {LARGE_DOCUMENT_NODE_DIAGNOSTIC_THRESHOLD}"
    )]
}

fn count_dom_nodes(node: &webby_dom::Node) -> usize {
    let mut count = 0_usize;
    let mut pending = vec![node];
    while let Some(current) = pending.pop() {
        count = count.saturating_add(1);
        pending.extend(current.children.iter());
        pending.extend(current.shadow_children.iter());
    }
    count
}

fn append_unique_diagnostics(target: &mut Vec<String>, diagnostics: Vec<String>) {
    for diagnostic in diagnostics {
        if !target.contains(&diagnostic) {
            target.push(diagnostic);
        }
    }
}

fn page_transition_spec(styled: &webby_style::StyledNode<'_>) -> PageTransitionSpec {
    let mut spec = PageTransitionSpec::default();
    collect_page_transition_spec(styled, &mut spec);
    spec
}

fn iframe_viewport_size(iframe: &webby_html::IframeElement) -> (usize, usize) {
    (
        parse_iframe_dimension(iframe.width.as_deref()).unwrap_or(300),
        parse_iframe_dimension(iframe.height.as_deref()).unwrap_or(150),
    )
}

fn parse_iframe_dimension(value: Option<&str>) -> Option<usize> {
    value
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.ceil() as usize)
}

fn insert_iframe_images(images: &mut ImageMap, iframes: &BTreeMap<String, RenderedPage>) {
    for (src, page) in iframes {
        let Some(width) = u32::try_from(page.surface.width).ok() else {
            continue;
        };
        let Some(height) = u32::try_from(page.surface.height).ok() else {
            continue;
        };
        images.insert(webby_layout::ImageResource {
            src: src.clone(),
            final_url: page.url.to_string(),
            image: webby_layout::DecodedImage {
                width,
                height,
                pixels: page.surface.pixels.clone(),
            },
        });
    }
}

fn iframe_page_map(iframes: &[IframeContext]) -> BTreeMap<String, RenderedPage> {
    iframes
        .iter()
        .map(|iframe| (iframe.src.clone(), iframe.page.as_ref().clone()))
        .collect()
}

fn iframe_contexts_for_layout(
    layout_box: &LayoutBox,
    pages: &BTreeMap<String, RenderedPage>,
    metadata: &[webby_html::IframeElement],
) -> Vec<IframeContext> {
    let mut iframes = Vec::new();
    collect_iframe_contexts_for_layout(layout_box, pages, metadata, &mut iframes);
    iframes
}

fn collect_iframe_contexts_for_layout(
    layout_box: &LayoutBox,
    pages: &BTreeMap<String, RenderedPage>,
    metadata: &[webby_html::IframeElement],
    iframes: &mut Vec<IframeContext>,
) {
    if let LayoutKind::Image {
        tag_name,
        src: Some(src),
        metadata: _,
        ..
    } = &layout_box.kind
        && tag_name == "iframe"
        && let Some(page) = pages.get(src)
    {
        let iframe_metadata = metadata
            .iter()
            .find(|iframe| iframe.src.as_deref() == Some(src.as_str()));
        iframes.push(IframeContext {
            src: src.clone(),
            url: page.url.clone(),
            name: iframe_metadata.and_then(|iframe| iframe.name.clone()),
            sandbox: iframe_metadata.and_then(|iframe| iframe.sandbox.clone()),
            rect: layout_box.dimensions.content,
            scroll_y: 0.0,
            page: Box::new(page.clone()),
        });
    }
    for child in layout_box.children() {
        collect_iframe_contexts_for_layout(child, pages, metadata, iframes);
    }
}

fn collect_page_transition_spec(node: &webby_style::StyledNode<'_>, spec: &mut PageTransitionSpec) {
    let transition = &node.style.transition;
    if transition.duration_ms > spec.duration_ms && !transition.properties.is_empty() {
        spec.duration_ms = transition.duration_ms;
        spec.delay_ms = transition.delay_ms;
        spec.timing_function = match transition.timing_function {
            webby_style::TransitionTimingFunction::Linear => AnimationTimingFunction::Linear,
            webby_style::TransitionTimingFunction::Ease => AnimationTimingFunction::Ease,
        };
    }
    for child in &node.children {
        collect_page_transition_spec(child, spec);
    }
}

fn execute_document_scripts(
    document: &mut webby_dom::Document,
    input: DocumentScriptExecutionInput,
) -> WebbyResult<webby_js::ExecutionReport> {
    let DocumentScriptExecutionInput {
        url,
        viewport_width,
        viewport_height,
        javascript_enabled,
        storage_enabled,
        local_storage,
        session_storage,
        fetch_resources,
        scripts,
    } = input;
    webby_js::execute_scripts_with_dom_and_browser(
        document,
        &scripts,
        webby_js::ExecutionOptions {
            enabled: javascript_enabled,
            ..webby_js::ExecutionOptions::default()
        },
        &webby_js::BrowserApiContext {
            current_url: url.to_string(),
            fetch_resources,
            local_storage,
            session_storage,
            storage_enabled,
            storage_quota_bytes: webby_state::STORAGE_QUOTA_BYTES,
            viewport_width: viewport_width as u32,
            viewport_height: viewport_height as u32,
            geometry: Vec::new(),
        },
    )
}

fn resource_hint_diagnostics(document: &webby_dom::Document) -> Vec<String> {
    webby_html::collect_resource_hints(document)
        .into_iter()
        .map(|hint| {
            let href = hint.href.unwrap_or_else(|| "(no href)".to_string());
            format!("resource hint rel={} href={} ignored", hint.rel, href)
        })
        .collect()
}

fn preload_javascript_request_resources<L: ResourceLoader>(
    scripts: &[webby_js::ScriptSource],
    page_url: &url::Url,
    loader: &L,
) -> PreloadedJavascriptResources {
    let mut resources = Vec::new();
    let mut diagnostics = Vec::new();
    for request in webby_js::collect_static_request_urls(scripts) {
        match preload_one_javascript_resource(&request, page_url, loader) {
            Ok(resource) => resources.push(resource),
            Err(message) => {
                diagnostics.push(message.clone());
                resources.push(webby_js::FetchResource {
                    request_url: request,
                    url: String::new(),
                    status: 0,
                    body: String::new(),
                    error: Some(message),
                });
            }
        }
    }
    PreloadedJavascriptResources {
        resources,
        diagnostics,
    }
}

fn preload_one_javascript_resource<L: ResourceLoader>(
    request: &str,
    page_url: &url::Url,
    loader: &L,
) -> Result<webby_js::FetchResource, String> {
    let target = page_url.join(request).map_err(|error| {
        format!("JavaScript request URL {request:?} failed to resolve: {error}")
    })?;
    let response = loader
        .load(&target)
        .map_err(|error| format!("JavaScript request {target} failed to load: {error}"))?;
    if let webby_security::CorsDecision::Blocked(message) =
        webby_security::check_cors(page_url, &response.final_url, &response.headers)
    {
        return Err(message);
    }
    let status = response.status.unwrap_or(200).clamp(0, u16::MAX);
    Ok(webby_js::FetchResource {
        request_url: request.to_string(),
        url: response.final_url.to_string(),
        status,
        body: decode_text(&response.bytes, response.content_type.as_deref()),
        error: None,
    })
}

fn collect_security_diagnostics(
    document: &webby_dom::Document,
    page_url: &url::Url,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for href in security_subresource_urls(document) {
        if let Ok(target) = page_url.join(&href)
            && let Some(message) = webby_security::mixed_content_diagnostic(page_url, &target)
        {
            diagnostics.push(message);
        }
    }
    diagnostics.extend(csp_noop_diagnostics(&document.root));
    diagnostics
}

fn security_subresource_urls(document: &webby_dom::Document) -> Vec<String> {
    let mut urls = Vec::new();
    urls.extend(
        webby_html::collect_stylesheet_links(document)
            .into_iter()
            .map(|link| link.href),
    );
    urls.extend(
        webby_image::collect_image_references(document)
            .into_iter()
            .map(|image| image.src),
    );
    urls.extend(
        webby_html::collect_scripts(document)
            .into_iter()
            .filter_map(|script| {
                if let webby_html::ScriptContent::External(src) = script.content {
                    Some(src)
                } else {
                    None
                }
            }),
    );
    urls
}

fn csp_noop_diagnostics(node: &webby_dom::Node) -> Vec<String> {
    let mut diagnostics = Vec::new();
    collect_csp_noop_diagnostics(node, &mut diagnostics);
    diagnostics
}

fn collect_csp_noop_diagnostics(node: &webby_dom::Node, diagnostics: &mut Vec<String>) {
    if let webby_dom::NodeKind::Element(element) = &node.kind
        && element.tag_name == "meta"
        && element
            .attributes
            .get("http-equiv")
            .is_some_and(|value| value.eq_ignore_ascii_case("content-security-policy"))
    {
        diagnostics.push(
            "Content-Security-Policy diagnostic: policy observed but not enforced".to_string(),
        );
    }
    for child in &node.children {
        collect_csp_noop_diagnostics(child, diagnostics);
    }
}

fn storage_entries_for_url(state: &StorageState, url: &url::Url) -> Vec<webby_js::StorageEntry> {
    let Ok(origin) = webby_state::storage_origin_key(url) else {
        return Vec::new();
    };
    state
        .entries_for_origin(&origin)
        .into_iter()
        .map(|entry| webby_js::StorageEntry {
            key: entry.key,
            value: entry.value,
        })
        .collect()
}

fn apply_storage_action(
    origin: &str,
    action: webby_js::StorageAction,
    local_storage: &mut StorageState,
    session_storage: &mut StorageState,
) -> WebbyResult<()> {
    match action {
        webby_js::StorageAction::SetItem { area, key, value } => {
            storage_area_mut(area, local_storage, session_storage).set_item(origin, &key, &value)
        }
        webby_js::StorageAction::RemoveItem { area, key } => {
            storage_area_mut(area, local_storage, session_storage).remove_item(origin, &key);
            Ok(())
        }
        webby_js::StorageAction::Clear { area } => {
            storage_area_mut(area, local_storage, session_storage).clear_origin(origin);
            Ok(())
        }
    }
}

fn storage_area_mut<'a>(
    area: webby_js::StorageArea,
    local_storage: &'a mut StorageState,
    session_storage: &'a mut StorageState,
) -> &'a mut StorageState {
    match area {
        webby_js::StorageArea::Local => local_storage,
        webby_js::StorageArea::Session => session_storage,
    }
}

fn find_deepest_box_at(layout_box: &LayoutBox, x: f32, y: f32) -> Option<&LayoutBox> {
    if !point_in_rect(x, y, layout_box.dimensions.margin_box()) {
        return None;
    }

    for item in layout_box.contents.iter().rev() {
        match item {
            webby_layout::LayoutItem::LineBox(line) => {
                for fragment in line.fragments.iter().rev() {
                    if let webby_layout::InlineFragment::Box(child) = fragment
                        && let Some(found) = find_deepest_box_at(child, x, y)
                    {
                        return Some(found);
                    }
                }
            }
            webby_layout::LayoutItem::Box(child) => {
                if let Some(found) = find_deepest_box_at(child, x, y) {
                    return Some(found);
                }
            }
            webby_layout::LayoutItem::Text(_) => {}
        }
    }

    Some(layout_box)
}

fn inspection_info_for_box(layout_box: &LayoutBox, page: &RenderedPage) -> InspectionInfo {
    let metadata = metadata_for_kind(&layout_box.kind)
        .cloned()
        .unwrap_or_default();
    InspectionInfo {
        tag_name: tag_name_for_kind(&layout_box.kind).to_string(),
        id: metadata.id,
        classes: metadata.classes,
        dimensions: layout_box.dimensions,
        computed_style: computed_style_summary(layout_box),
        paint_summary: format!(
            "display-commands={} links={} form-controls={}",
            page.display_list.commands.len(),
            page.links.len(),
            page.form_controls.len()
        ),
        link_href: link_for_box(layout_box, page),
        image_metadata: image_metadata_for_kind(&layout_box.kind),
        form_metadata: form_metadata_for_kind(&layout_box.kind),
    }
}

fn tag_name_for_kind(kind: &LayoutKind) -> &str {
    match kind {
        LayoutKind::Document => "#document",
        LayoutKind::Block { tag_name, .. }
        | LayoutKind::Image { tag_name, .. }
        | LayoutKind::FormControl { tag_name, .. } => tag_name,
    }
}

fn metadata_for_kind(kind: &LayoutKind) -> Option<&ElementMetadata> {
    match kind {
        LayoutKind::Document => None,
        LayoutKind::Block { metadata, .. }
        | LayoutKind::Image { metadata, .. }
        | LayoutKind::FormControl { metadata, .. } => Some(metadata.as_ref()),
    }
}

fn computed_style_summary(layout_box: &LayoutBox) -> String {
    format!(
        "background={} border-color={} border={} padding={} margin={}",
        color_summary(layout_box.visuals.background_color),
        color_summary(layout_box.visuals.border.color),
        edges_summary(layout_box.dimensions.border),
        edges_summary(layout_box.dimensions.padding),
        edges_summary(layout_box.dimensions.margin)
    )
}

fn color_summary(color: webby_layout::VisualColor) -> String {
    format!("rgba({},{},{},{})", color.r, color.g, color.b, color.a)
}

fn edges_summary(edges: webby_layout::EdgeSizes) -> String {
    format!(
        "{:.1}/{:.1}/{:.1}/{:.1}",
        edges.top, edges.right, edges.bottom, edges.left
    )
}

fn link_for_box(layout_box: &LayoutBox, page: &RenderedPage) -> Option<String> {
    let border_box = layout_box.dimensions.border_box();
    page.links
        .iter()
        .find(|link| rects_intersect(link.rect, border_box))
        .map(|link| link.href.clone())
}

fn image_metadata_for_kind(kind: &LayoutKind) -> Option<String> {
    let LayoutKind::Image {
        src, alt, image, ..
    } = kind
    else {
        return None;
    };
    let status = image
        .as_ref()
        .map_or("placeholder".to_string(), |resource| {
            format!(
                "decoded {}x{} final={}",
                resource.image.width, resource.image.height, resource.final_url
            )
        });
    Some(format!(
        "src={} alt={} {status}",
        src.as_deref().unwrap_or(""),
        alt.as_deref().unwrap_or("")
    ))
}

fn form_metadata_for_kind(kind: &LayoutKind) -> Option<String> {
    let LayoutKind::FormControl {
        control_type,
        form,
        name,
        value,
        placeholder,
        ..
    } = kind
    else {
        return None;
    };
    Some(format!(
        "type={:?} form={} name={} value={} placeholder={}",
        control_type,
        form.as_ref()
            .map(|form| form.id.to_string())
            .unwrap_or_else(|| "none".to_string()),
        name.as_deref().unwrap_or(""),
        value,
        placeholder.as_deref().unwrap_or("")
    ))
}

fn rects_intersect(left: Rect, right: Rect) -> bool {
    left.x < right.x + right.width
        && left.x + left.width > right.x
        && left.y < right.y + right.height
        && left.y + left.height > right.y
}

fn draw_chrome(surface: &mut Surface, state: &AppState) {
    fill_rect(
        surface,
        0,
        0,
        surface.width,
        CHROME_HEIGHT,
        [232, 235, 239, 255],
    );
    draw_tab_strip(surface, state);
    draw_chrome_buttons(surface);
    let address_width = surface.width.saturating_sub(ADDRESS_X + 12);
    fill_rect(
        surface,
        ADDRESS_X,
        ADDRESS_Y,
        address_width,
        ADDRESS_HEIGHT,
        [255, 255, 255, 255],
    );
    stroke_rect(
        surface,
        ADDRESS_X,
        ADDRESS_Y,
        address_width,
        ADDRESS_HEIGHT,
        [150, 156, 166, 255],
    );
    if state.chrome.address_selected {
        fill_rect(
            surface,
            ADDRESS_X + 4,
            ADDRESS_Y + 4,
            address_width.saturating_sub(8),
            ADDRESS_HEIGHT.saturating_sub(8),
            [214, 232, 255, 255],
        );
    }
    surface.draw_text(
        (ADDRESS_X + 8) as f32,
        25.0,
        &state.chrome.address_input,
        13.0,
        FontWeight::Normal,
        Color {
            r: 20,
            g: 24,
            b: 28,
            a: 255,
        },
    );
    draw_chrome_status(surface, state);
}

fn draw_tab_strip(surface: &mut Surface, state: &AppState) {
    let tab_count = state.tabs.len().max(1);
    let tab_width = chrome_tab_width(surface.width, tab_count);
    for index in 0..tab_count {
        let x = index.saturating_mul(tab_width);
        let width = tab_render_width(surface.width, tab_width, index, tab_count);
        let active = index == state.active_tab_index;
        fill_rect(
            surface,
            x,
            0,
            width,
            TAB_STRIP_HEIGHT,
            if active {
                [250, 251, 253, 255]
            } else {
                [210, 215, 222, 255]
            },
        );
        stroke_rect(surface, x, 0, width, TAB_STRIP_HEIGHT, [150, 156, 166, 255]);
        if width >= 32 {
            let label = state
                .tabs
                .get(index)
                .and_then(|tab| tab.page.as_ref())
                .map(|page| page.title.as_str())
                .filter(|title| !title.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}", index + 1));
            surface.draw_text(
                (x + 6) as f32,
                3.0,
                &label,
                10.0,
                FontWeight::Normal,
                Color {
                    r: 40,
                    g: 45,
                    b: 52,
                    a: 255,
                },
            );
        }
        if width >= 36 {
            surface.draw_text(
                x.saturating_add(width.saturating_sub(15)) as f32,
                3.0,
                "x",
                10.0,
                FontWeight::Normal,
                Color {
                    r: 100,
                    g: 35,
                    b: 35,
                    a: 255,
                },
            );
        }
    }
}

fn draw_chrome_buttons(surface: &mut Surface) {
    for (action, rect) in chrome_button_rects() {
        let label = match action {
            ChromeAction::Back => "<",
            ChromeAction::Forward => ">",
            ChromeAction::Reload => "R",
            ChromeAction::BookmarkCurrentPage => "*",
            ChromeAction::SwitchTab(_) | ChromeAction::CloseTab(_) | ChromeAction::FocusAddress => {
                ""
            }
        };
        let (x, y, width, height) = rect;
        fill_rect(surface, x, y, width, height, [246, 248, 251, 255]);
        stroke_rect(surface, x, y, width, height, [150, 156, 166, 255]);
        surface.draw_text(
            (x + 6) as f32,
            (y + 4) as f32,
            label,
            12.0,
            FontWeight::Bold,
            Color {
                r: 45,
                g: 50,
                b: 58,
                a: 255,
            },
        );
    }
}

fn draw_chrome_status(surface: &mut Surface, state: &AppState) {
    let label = match &state.status {
        PageStatus::Startup => "Start",
        PageStatus::Loading { .. } => "Loading",
        PageStatus::Loaded { .. } => "Ready",
        PageStatus::Error { .. } => "Error",
    };
    let color = match &state.status {
        PageStatus::Error { .. } => Color {
            r: 140,
            g: 24,
            b: 24,
            a: 255,
        },
        PageStatus::Loading { .. } => Color {
            r: 70,
            g: 80,
            b: 150,
            a: 255,
        },
        PageStatus::Startup | PageStatus::Loaded { .. } => Color {
            r: 70,
            g: 78,
            b: 88,
            a: 255,
        },
    };
    surface.draw_text(
        ADDRESS_X as f32,
        3.0,
        label,
        10.0,
        FontWeight::Normal,
        color,
    );
    if state
        .page
        .as_ref()
        .and_then(|page| page.favicon_href.as_ref())
        .is_some()
    {
        surface.draw_text(
            ADDRESS_X.saturating_add(48) as f32,
            3.0,
            "icon",
            10.0,
            FontWeight::Normal,
            color,
        );
    }
}

fn draw_shell_overlay(surface: &mut Surface, state: &AppState) {
    let status = state.status_bar_text();
    let y = surface.height.saturating_sub(16);
    fill_rect(surface, 0, y, surface.width, 16, [242, 244, 247, 255]);
    surface.draw_text(
        6.0,
        y.saturating_add(3) as f32,
        &status,
        10.0,
        FontWeight::Normal,
        Color {
            r: 55,
            g: 62,
            b: 72,
            a: 255,
        },
    );
    if state.find_active {
        draw_find_panel(surface, state);
    }
    if state.shortcut_help_visible {
        draw_shortcut_help(surface);
    }
}

fn draw_find_panel(surface: &mut Surface, state: &AppState) {
    let width = 260.min(surface.width);
    fill_rect(surface, 0, CHROME_HEIGHT, width, 24, [255, 252, 225, 255]);
    stroke_rect(surface, 0, CHROME_HEIGHT, width, 24, [170, 156, 98, 255]);
    surface.draw_text(
        6.0,
        (CHROME_HEIGHT + 6) as f32,
        &format!(
            "Find: {} ({} matches)",
            state.find_query, state.find_match_count
        ),
        11.0,
        FontWeight::Normal,
        Color {
            r: 55,
            g: 48,
            b: 24,
            a: 255,
        },
    );
}

fn draw_shortcut_help(surface: &mut Surface) {
    let width = 350.min(surface.width);
    let height = 92.min(surface.height.saturating_sub(CHROME_HEIGHT));
    fill_rect(
        surface,
        8,
        CHROME_HEIGHT + 8,
        width,
        height,
        [250, 251, 253, 255],
    );
    stroke_rect(
        surface,
        8,
        CHROME_HEIGHT + 8,
        width,
        height,
        [120, 128, 138, 255],
    );
    for (index, line) in [
        "Shortcuts",
        "Ctrl+L address  Ctrl+A select all  Ctrl+C/V copy/paste",
        "Ctrl+F find  Ctrl+O open local path  Ctrl+R reload",
        "Ctrl+T/W tabs  Alt+Left/Right history  F1 help  F12 inspect",
    ]
    .iter()
    .enumerate()
    {
        surface.draw_text(
            16.0,
            (CHROME_HEIGHT + 16 + index * 18) as f32,
            line,
            10.0,
            if index == 0 {
                FontWeight::Bold
            } else {
                FontWeight::Normal
            },
            Color {
                r: 38,
                g: 44,
                b: 52,
                a: 255,
            },
        );
    }
}

fn draw_status_message(surface: &mut Surface, label: &str, detail: &str) {
    surface.draw_text(
        20.0,
        (CHROME_HEIGHT + 22) as f32,
        label,
        16.0,
        FontWeight::Bold,
        Color {
            r: 25,
            g: 30,
            b: 36,
            a: 255,
        },
    );
    surface.draw_text(
        20.0,
        (CHROME_HEIGHT + 44) as f32,
        detail,
        13.0,
        FontWeight::Normal,
        Color {
            r: 80,
            g: 86,
            b: 96,
            a: 255,
        },
    );
}

fn draw_debug_overlay(
    surface: &mut Surface,
    page: &RenderedPage,
    scroll_y: f32,
    selected: Option<&InspectionInfo>,
) {
    draw_debug_box(surface, &page.layout.root, scroll_y);
    if let Some(info) = selected {
        draw_inspection_panel(surface, info);
    }
}

fn draw_debug_box(surface: &mut Surface, layout_box: &LayoutBox, scroll_y: f32) {
    stroke_page_rect(
        surface,
        layout_box.dimensions.margin_box(),
        scroll_y,
        [255, 170, 0, 255],
    );
    stroke_page_rect(
        surface,
        layout_box.dimensions.border_box(),
        scroll_y,
        [220, 40, 40, 255],
    );
    stroke_page_rect(
        surface,
        padding_box(layout_box.dimensions),
        scroll_y,
        [60, 150, 255, 255],
    );
    stroke_page_rect(
        surface,
        layout_box.dimensions.content,
        scroll_y,
        [40, 190, 80, 255],
    );
    for item in &layout_box.contents {
        match item {
            webby_layout::LayoutItem::LineBox(line) => {
                for fragment in &line.fragments {
                    if let webby_layout::InlineFragment::Box(child) = fragment {
                        draw_debug_box(surface, child, scroll_y);
                    }
                }
            }
            webby_layout::LayoutItem::Box(child) => draw_debug_box(surface, child, scroll_y),
            webby_layout::LayoutItem::Text(_) => {}
        }
    }
}

fn padding_box(dimensions: Dimensions) -> Rect {
    Rect {
        x: dimensions.content.x - dimensions.padding.left,
        y: dimensions.content.y - dimensions.padding.top,
        width: dimensions.content.width + dimensions.padding.left + dimensions.padding.right,
        height: dimensions.content.height + dimensions.padding.top + dimensions.padding.bottom,
    }
}

fn stroke_page_rect(surface: &mut Surface, rect: Rect, scroll_y: f32, color: [u8; 4]) {
    let x = rect.x.floor().max(0.0) as usize;
    let y = (rect.y - scroll_y + CHROME_HEIGHT as f32).floor().max(0.0) as usize;
    let width = rect.width.ceil().max(0.0) as usize;
    let height = rect.height.ceil().max(0.0) as usize;
    stroke_rect(surface, x, y, width, height, color);
}

fn draw_inspection_panel(surface: &mut Surface, info: &InspectionInfo) {
    let panel_width = surface.width.min(360);
    let panel_height = 92;
    let y = surface.height.saturating_sub(panel_height);
    fill_rect(
        surface,
        0,
        y,
        panel_width,
        panel_height,
        [248, 249, 252, 245],
    );
    stroke_rect(surface, 0, y, panel_width, panel_height, [80, 86, 96, 255]);
    let title = selector_summary(info);
    surface.draw_text(
        8.0,
        (y + 8) as f32,
        &title,
        12.0,
        FontWeight::Bold,
        Color::BLACK,
    );
    surface.draw_text(
        8.0,
        (y + 26) as f32,
        &format!("content={}", rect_summary(info.dimensions.content)),
        11.0,
        FontWeight::Normal,
        Color::BLACK,
    );
    surface.draw_text(
        8.0,
        (y + 42) as f32,
        &info.computed_style,
        10.0,
        FontWeight::Normal,
        Color::BLACK,
    );
    surface.draw_text(
        8.0,
        (y + 58) as f32,
        &info.paint_summary,
        10.0,
        FontWeight::Normal,
        Color::BLACK,
    );
    if let Some(metadata) = info
        .link_href
        .as_ref()
        .or(info.image_metadata.as_ref())
        .or(info.form_metadata.as_ref())
    {
        surface.draw_text(
            8.0,
            (y + 74) as f32,
            metadata,
            10.0,
            FontWeight::Normal,
            Color::BLACK,
        );
    }
}

fn selector_summary(info: &InspectionInfo) -> String {
    let mut output = info.tag_name.clone();
    if let Some(id) = &info.id {
        output.push('#');
        output.push_str(id);
    }
    for class_name in &info.classes {
        output.push('.');
        output.push_str(class_name);
    }
    output
}

fn rect_summary(rect: Rect) -> String {
    format!(
        "{:.1},{:.1},{:.1},{:.1}",
        rect.x, rect.y, rect.width, rect.height
    )
}

fn blit_page(target: &mut Surface, page: &Surface, y_offset: usize, scroll_y: f32) {
    let scroll = scroll_y.floor().max(0.0) as usize;
    let available_height = target.height.saturating_sub(y_offset);
    for destination_y in 0..available_height {
        let source_y = destination_y + scroll;
        if source_y >= page.height {
            break;
        }
        for x in 0..target.width.min(page.width) {
            let source = (source_y * page.width + x) * 4;
            let destination = ((destination_y + y_offset) * target.width + x) * 4;
            if source + 3 < page.pixels.len() && destination + 3 < target.pixels.len() {
                target.pixels[destination..destination + 4]
                    .copy_from_slice(&page.pixels[source..source + 4]);
            }
        }
    }
}

fn transition_between_pages(
    previous: &RenderedPage,
    next: &RenderedPage,
    enabled: bool,
) -> Option<ActiveTransition> {
    if !enabled
        || next.transition.duration_ms == 0
        || previous.display_list.commands == next.display_list.commands
    {
        return None;
    }
    Some(ActiveTransition {
        from: previous.display_list.clone(),
        to: next.display_list.clone(),
        duration_ms: next.transition.duration_ms,
        delay_ms: next.transition.delay_ms,
        elapsed_ms: 0,
        timing_function: next.transition.timing_function,
    })
}

fn render_transition_surface(
    transition: &ActiveTransition,
    width: usize,
    height: usize,
) -> WebbyResult<Surface> {
    let list = interpolated_display_list(transition);
    render_with_backend(&SoftwareRenderBackend, &list, width, height)
}

fn interpolated_display_list(transition: &ActiveTransition) -> DisplayList {
    let progress = transition_progress(transition);
    let commands = transition
        .to
        .commands
        .iter()
        .enumerate()
        .map(|(index, to)| {
            transition.from.commands.get(index).map_or_else(
                || to.clone(),
                |from| interpolate_command(from, to, progress),
            )
        })
        .collect();
    DisplayList { commands }
}

fn transition_progress(transition: &ActiveTransition) -> f32 {
    if transition.duration_ms == 0 {
        return 1.0;
    }
    if transition.elapsed_ms <= transition.delay_ms {
        return 0.0;
    }
    let elapsed = transition.elapsed_ms.saturating_sub(transition.delay_ms);
    let raw = (elapsed as f32 / transition.duration_ms as f32).clamp(0.0, 1.0);
    match transition.timing_function {
        AnimationTimingFunction::Linear => raw,
        AnimationTimingFunction::Ease => raw * raw * (3.0 - 2.0 * raw),
    }
}

fn interpolate_command(
    from: &webby_render::DisplayCommand,
    to: &webby_render::DisplayCommand,
    progress: f32,
) -> webby_render::DisplayCommand {
    match (from, to) {
        (
            webby_render::DisplayCommand::FillRect {
                color: from_color, ..
            },
            webby_render::DisplayCommand::FillRect { rect, color },
        ) => webby_render::DisplayCommand::FillRect {
            rect: interpolate_rect(from_rect(from), *rect, progress),
            color: interpolate_color(*from_color, *color, progress),
        },
        (
            webby_render::DisplayCommand::DrawText {
                color: from_color, ..
            },
            webby_render::DisplayCommand::DrawText {
                text,
                rect,
                font_size,
                font_weight,
                font_family,
                text_decoration,
                color,
                href,
            },
        ) => webby_render::DisplayCommand::DrawText {
            text: text.clone(),
            rect: interpolate_rect(from_rect(from), *rect, progress),
            font_size: *font_size,
            font_weight: *font_weight,
            font_family: *font_family,
            text_decoration: *text_decoration,
            color: interpolate_color(*from_color, *color, progress),
            href: href.clone(),
        },
        (
            webby_render::DisplayCommand::StrokeRect {
                color: from_color, ..
            },
            webby_render::DisplayCommand::StrokeRect { rect, color, width },
        ) => webby_render::DisplayCommand::StrokeRect {
            rect: interpolate_rect(from_rect(from), *rect, progress),
            color: interpolate_color(*from_color, *color, progress),
            width: *width,
        },
        (
            webby_render::DisplayCommand::Line {
                color: from_color, ..
            },
            webby_render::DisplayCommand::Line {
                from: target_from,
                to: target_to,
                color,
                width,
            },
        ) => webby_render::DisplayCommand::Line {
            from: interpolate_point(from_point(from), *target_from, progress),
            to: interpolate_point(to_point(from), *target_to, progress),
            color: interpolate_color(*from_color, *color, progress),
            width: *width,
        },
        _ => to.clone(),
    }
}

fn from_rect(command: &webby_render::DisplayCommand) -> Rect {
    match command {
        webby_render::DisplayCommand::FillRect { rect, .. }
        | webby_render::DisplayCommand::DrawText { rect, .. }
        | webby_render::DisplayCommand::StrokeRect { rect, .. } => *rect,
        _ => Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        },
    }
}

fn from_point(command: &webby_render::DisplayCommand) -> webby_render::Point {
    match command {
        webby_render::DisplayCommand::Line { from, .. } => *from,
        _ => webby_render::Point { x: 0.0, y: 0.0 },
    }
}

fn to_point(command: &webby_render::DisplayCommand) -> webby_render::Point {
    match command {
        webby_render::DisplayCommand::Line { to, .. } => *to,
        _ => webby_render::Point { x: 0.0, y: 0.0 },
    }
}

fn interpolate_rect(from: Rect, to: Rect, progress: f32) -> Rect {
    Rect {
        x: interpolate_float(from.x, to.x, progress),
        y: interpolate_float(from.y, to.y, progress),
        width: interpolate_float(from.width, to.width, progress),
        height: interpolate_float(from.height, to.height, progress),
    }
}

fn interpolate_point(
    from: webby_render::Point,
    to: webby_render::Point,
    progress: f32,
) -> webby_render::Point {
    webby_render::Point {
        x: interpolate_float(from.x, to.x, progress),
        y: interpolate_float(from.y, to.y, progress),
    }
}

fn interpolate_float(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress.clamp(0.0, 1.0)
}

fn interpolate_color(from: Color, to: Color, progress: f32) -> Color {
    Color {
        r: interpolate_channel(from.r, to.r, progress),
        g: interpolate_channel(from.g, to.g, progress),
        b: interpolate_channel(from.b, to.b, progress),
        a: interpolate_channel(from.a, to.a, progress),
    }
}

fn interpolate_channel(from: u8, to: u8, progress: f32) -> u8 {
    let value = from as f32 + (to as f32 - from as f32) * progress.clamp(0.0, 1.0);
    value.round().clamp(0.0, 255.0) as u8
}

fn draw_form_value_overlays(
    surface: &mut Surface,
    page: &RenderedPage,
    values: &BTreeMap<usize, String>,
    focused: Option<usize>,
    scroll_y: f32,
) {
    for control in &page.form_controls {
        let translated = webby_layout::Rect {
            x: control.rect.x,
            y: control.rect.y - scroll_y + CHROME_HEIGHT as f32,
            width: control.rect.width,
            height: control.rect.height,
        };
        if matches!(
            control.control_type,
            FormControlType::Checkbox | FormControlType::Radio
        ) {
            if control_checked_state(control, values) {
                let inset = translated.width.min(translated.height) / 3.0;
                fill_rect(
                    surface,
                    (translated.x + inset).max(0.0) as usize,
                    (translated.y + inset).max(0.0) as usize,
                    inset.max(1.0) as usize,
                    inset.max(1.0) as usize,
                    [20, 105, 210, 255],
                );
            }
            continue;
        }
        if !matches!(
            control.control_type,
            FormControlType::Text
                | FormControlType::Search
                | FormControlType::Password
                | FormControlType::Email
                | FormControlType::Textarea
                | FormControlType::Select
        ) || (!values.contains_key(&control.id) && focused != Some(control.id))
        {
            continue;
        }

        let mut value = values
            .get(&control.id)
            .cloned()
            .unwrap_or_else(|| control.value.clone());
        if control.control_type == FormControlType::Password {
            value = "*".repeat(value.chars().count());
        }
        let visual_state = if focused == Some(control.id) {
            FormControlVisualState::Focused
        } else {
            FormControlVisualState::Normal
        };
        draw_text_control_overlay(surface, translated, &value, visual_state);
    }
}

fn draw_keyboard_focus_ring(
    surface: &mut Surface,
    page: &RenderedPage,
    focus: Option<KeyboardFocusTarget>,
    scroll_y: f32,
) {
    let Some(focus) = focus else {
        return;
    };
    let rect = match focus {
        KeyboardFocusTarget::Link { index } => page.links.get(index).map(|link| link.rect),
        KeyboardFocusTarget::FormControl { id } => page
            .form_controls
            .iter()
            .find(|control| control.id == id)
            .map(|control| control.rect),
    };
    if let Some(rect) = rect {
        stroke_page_rect(surface, rect, scroll_y, [255, 180, 0, 255]);
    }
}

fn fill_rect(
    surface: &mut Surface,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    let x_end = x.saturating_add(width).min(surface.width);
    let y_end = y.saturating_add(height).min(surface.height);
    for row in y..y_end {
        for column in x..x_end {
            put_pixel(surface, column, row, color);
        }
    }
}

fn stroke_rect(
    surface: &mut Surface,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    if width == 0 || height == 0 {
        return;
    }
    fill_rect(surface, x, y, width, 1, color);
    fill_rect(
        surface,
        x,
        y.saturating_add(height.saturating_sub(1)),
        width,
        1,
        color,
    );
    fill_rect(surface, x, y, 1, height, color);
    fill_rect(
        surface,
        x.saturating_add(width.saturating_sub(1)),
        y,
        1,
        height,
        color,
    );
}

fn put_pixel(surface: &mut Surface, x: usize, y: usize, color: [u8; 4]) {
    let Some(index) = y
        .checked_mul(surface.width)
        .and_then(|row| row.checked_add(x))
        .and_then(|pixel| pixel.checked_mul(4))
    else {
        return;
    };
    if index + 3 >= surface.pixels.len() {
        return;
    }
    surface.pixels[index..index + 4].copy_from_slice(&color);
}

fn default_download_directory() -> std::path::PathBuf {
    std::env::temp_dir().join("webby-downloads")
}

fn sanitize_download_filename(filename: &str) -> String {
    let sanitized = filename
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim().trim_matches('.').trim();
    if sanitized.is_empty() {
        "download".to_string()
    } else {
        sanitized.to_string()
    }
}

fn unique_download_destination(directory: &std::path::Path, filename: &str) -> std::path::PathBuf {
    let direct = directory.join(filename);
    if !direct.exists() {
        return direct;
    }
    let path = std::path::Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("download");
    let extension = path.extension().and_then(|extension| extension.to_str());
    for suffix in 2..=usize::MAX {
        let candidate = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem}-{suffix}.{extension}"),
            _ => format!("{stem}-{suffix}"),
        };
        let destination = directory.join(candidate);
        if !destination.exists() {
            return destination;
        }
    }
    directory.join("download")
}

/// Creates the documented startup URL for tests and app startup.
pub fn startup_file_url() -> WebbyResult<url::Url> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(STARTUP_ADDRESS);
    let absolute = path.canonicalize().map_err(|source| WebbyError::Io {
        path: Some(path),
        source,
    })?;
    url::Url::from_file_path(&absolute).map_err(|()| WebbyError::Url {
        message: format!(
            "could not convert startup page to URL: {}",
            absolute.display()
        ),
    })
}

fn resolve_address_input(input: &str) -> WebbyResult<url::Url> {
    if input.trim() == STARTUP_ADDRESS {
        return startup_file_url();
    }

    Ok(resolve_input(input, &SearchEngine::default())?.url)
}

/// Simple in-memory loader useful for app tests.
#[derive(Debug, Clone)]
pub struct StaticHtmlLoader {
    html: String,
    final_url: url::Url,
}

impl StaticHtmlLoader {
    /// Creates a loader returning the provided HTML for any URL.
    pub fn new(html: impl Into<String>, final_url: url::Url) -> Self {
        Self {
            html: html.into(),
            final_url,
        }
    }
}

impl ResourceLoader for StaticHtmlLoader {
    fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
        Ok(ResourceResponse {
            requested_url: url.clone(),
            final_url: self.final_url.clone(),
            status: Some(200),
            content_type: Some("text/html; charset=utf-8".to_string()),
            headers: Vec::new(),
            bytes: self.html.as_bytes().to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ADDRESS_X, ADDRESS_Y, AccessibleRole, ActiveTransition, AppState, BOOKMARKS_PAGE_URL,
        BrowserChrome, CHROME_BUTTON_Y, CHROME_HEIGHT, ChromeAction, Dimensions, HoverTarget,
        InspectionInfo, KeyboardFocusTarget, LayoutBox, NavigationLifecycle, PagePipeline,
        PageStatus, PendingHistoryAction, Rect, RenderedPage, STARTUP_ADDRESS, StaticHtmlLoader,
        Surface, build_display_list, startup_file_url, tag_name_for_kind,
    };
    use base64::Engine;
    use image::{ImageBuffer, ImageFormat, Rgba};
    use std::cell::{Cell, RefCell};
    use webby_core::{WebbyError, WebbyResult};
    use webby_layout::FormControlType;
    use webby_net::{ResourceLoader, ResourceResponse};

    #[test]
    fn app_state_starts_with_expected_initial_address_input_state() {
        let state = AppState::new();

        assert_eq!(state.chrome.address_input, STARTUP_ADDRESS);
        assert!(state.chrome.address_focused);
        assert_eq!(state.status, PageStatus::Startup);
    }

    #[test]
    fn address_cursor_editing_is_unicode_safe() {
        let mut chrome = BrowserChrome::new();
        chrome.set_address_input("ab");
        chrome.move_cursor_left();
        chrome.type_character('å');
        assert_eq!(chrome.address_input, "aåb");

        chrome.backspace();
        assert_eq!(chrome.address_input, "ab");
        chrome.move_cursor_home();
        chrome.type_character('x');
        chrome.move_cursor_end();
        chrome.type_character('y');
        assert_eq!(chrome.address_input, "xaby");

        chrome.set_address_input("åb");
        chrome.address_cursor = 1;
        chrome.type_character('x');
        assert_eq!(chrome.address_input, "xåb");
        chrome.address_cursor = usize::MAX;
        chrome.paste("z");
        assert_eq!(chrome.address_input, "xåbz");
    }

    #[test]
    fn app_local_clipboard_copies_selection_or_current_url_and_pastes() -> WebbyResult<()> {
        let mut state = AppState::new();
        state.chrome.set_address_input("example.test");
        state.select_all_address();
        assert!(state.copy_address_or_current_url());
        assert_eq!(state.clipboard, "example.test");

        state.chrome.set_address_input("https://");
        state.clipboard = "webby.test".to_string();
        assert!(state.paste_address());
        assert_eq!(state.chrome.address_input, "https://webby.test");

        state.chrome.address_selected = false;
        state.navigation.current_url =
            Some(url::Url::parse("https://current.test/").map_err(url_error)?);
        assert!(state.copy_address_or_current_url());
        assert_eq!(state.clipboard, "https://current.test/");
        Ok(())
    }

    #[test]
    fn find_in_page_counts_visible_text_and_stays_per_tab() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(240, 120).render_html(
            "<title>Find page</title><body><p>Webby visible webby</p><script>var x = 'webby';</script></body>",
            base,
        )?;
        let mut state = AppState::with_window_size(240, 160);
        state.finish_navigation(Ok(page));
        state.open_find();
        for character in "webby".chars() {
            state.type_character(character);
        }

        assert_eq!(state.find_match_count, 2);
        assert!(state.find_active);
        state.new_tab();
        assert_eq!(state.find_query, "");
        assert_eq!(state.find_match_count, 0);
        assert!(!state.find_active);
        assert!(state.switch_tab(0));
        assert_eq!(state.find_query, "webby");
        assert_eq!(state.find_match_count, 2);
        assert!(state.find_active);
        state.validate_active_tab_sync()?;
        Ok(())
    }

    #[test]
    fn page_title_favicon_and_hover_status_are_exposed_by_shell_state() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(240, 120).render_html(
            "<head><title>Webby Page</title><link rel=\"icon\" href=\"favicon.ico\"></head><body><a href=\"/docs\">Docs</a></body>",
            base,
        )?;
        let mut state = AppState::with_window_size(240, 160);
        state.finish_navigation(Ok(page));

        assert_eq!(state.window_title(), "Webby Page");
        assert_eq!(
            state
                .page
                .as_ref()
                .and_then(|page| page.favicon_href.as_deref()),
            Some("favicon.ico")
        );
        let (x, y) = first_link_point(&state)?;
        state.update_hover_at(x, y + CHROME_HEIGHT as f32);
        assert_eq!(state.status_bar_text(), "/docs");
        Ok(())
    }

    #[test]
    fn open_file_and_download_resource_are_deterministic_shell_operations() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(240, 160);
        let loader = webby_net::DefaultResourceLoader::new()?;
        let startup = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(STARTUP_ADDRESS);
        assert!(state.open_file(&loader, &startup));
        assert!(matches!(state.status, PageStatus::Loaded { .. }));

        let download_url = url::Url::parse("https://example.test/download").map_err(url_error)?;
        let download_loader = StaticHtmlLoader::new("download bytes", download_url.clone());
        let destination =
            std::env::temp_dir().join(format!("webby-download-{}.txt", std::process::id()));
        let record = state.download_resource(&download_loader, &download_url, &destination)?;
        let bytes = std::fs::read(&destination).map_err(|source| WebbyError::Io {
            path: Some(destination.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(&destination);

        assert_eq!(bytes, b"download bytes");
        assert_eq!(record.url, download_url);
        assert_eq!(record.byte_len, bytes.len());
        assert_eq!(state.downloads, vec![record]);
        Ok(())
    }

    #[test]
    fn unsupported_navigation_downloads_safely_without_committing_history() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let download_url = url::Url::parse("https://example.test/export").map_err(url_error)?;
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(
            &StaticHtmlLoader::new("<body>home</body>", home.clone()),
            home.clone(),
        );
        let history = state.navigation.history.clone();
        let directory = temp_download_dir("navigation");
        let _ = std::fs::remove_dir_all(&directory);
        state.set_download_directory(&directory);
        let loader = DownloadResponseLoader::new(
            download_url.clone(),
            "application/pdf",
            vec![(
                "content-disposition".to_string(),
                "attachment; filename=\"../secret?.pdf\"".to_string(),
            )],
            b"%PDF".to_vec(),
        );

        state.navigate_to_url(&loader, download_url.clone());
        state.navigate_to_url(&loader, download_url);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert_eq!(state.navigation.history, history);
        assert!(state.navigation.pending_url.is_none());
        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        assert_eq!(state.downloads.len(), 2);
        assert_eq!(state.downloads[0].filename, "_secret_.pdf");
        assert_eq!(state.downloads[1].filename, "_secret_-2.pdf");
        assert_eq!(
            std::fs::read(&state.downloads[0].destination).map_err(|source| {
                WebbyError::Io {
                    path: Some(state.downloads[0].destination.clone()),
                    source,
                }
            })?,
            b"%PDF"
        );
        assert!(
            state
                .download_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("download complete"))
        );
        let _ = std::fs::remove_dir_all(directory);
        Ok(())
    }

    #[test]
    fn basic_auth_challenge_becomes_visible_error_without_credentials() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/private").map_err(url_error)?;
        let loader = BasicAuthLoader::new(url.clone(), "webby", "secret");
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, url.clone());

        assert!(matches!(state.status, PageStatus::Error { .. }));
        assert_eq!(state.navigation.failed_url.as_deref(), Some(url.as_str()));
        let challenge = state
            .auth_challenge
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing auth challenge"))?;
        assert_eq!(challenge.origin, "https://example.test:443");
        assert_eq!(challenge.challenge.realm.as_deref(), Some("Members"));
        assert!(state.auth_diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("HTTP Basic authentication required")
                && !diagnostic.contains("secret")
        }));
        assert!(state.navigation.history.is_empty());
        Ok(())
    }

    #[test]
    fn basic_auth_credentials_load_page_and_are_not_persisted() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/private").map_err(url_error)?;
        let loader = BasicAuthLoader::new(url.clone(), "webby", "secret");
        let mut state = AppState::with_window_size(240, 160);

        state.set_basic_auth_credentials(&url, "webby", "secret")?;
        state.navigate_to_url(&loader, url.clone());

        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&url));
        assert_eq!(state.navigation.history, vec![url.clone()]);
        assert!(state.auth_challenge.is_none());
        assert_eq!(loader.authorized_calls.get(), 1);

        let fresh = AppState::with_window_size(240, 160);
        assert!(fresh.basic_auth_credentials.is_empty());
        Ok(())
    }

    #[test]
    fn wrong_basic_auth_credentials_fail_gracefully_and_are_redacted() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/private").map_err(url_error)?;
        let loader = BasicAuthLoader::new(url.clone(), "webby", "secret");
        let mut state = AppState::with_window_size(240, 160);

        state.set_basic_auth_credentials(&url, "webby", "wrong-password")?;
        state.navigate_to_url(&loader, url.clone());

        assert!(matches!(
            state.status,
            PageStatus::Error { ref message } if message.contains("authentication failed")
        ));
        assert_eq!(state.navigation.failed_url.as_deref(), Some(url.as_str()));
        assert!(state.navigation.history.is_empty());
        let diagnostics = format!("{:?}", state.auth_diagnostics);
        assert!(diagnostics.contains("<redacted>"));
        assert!(!diagnostics.contains("wrong-password"));
        assert!(!diagnostics.contains("secret"));
        Ok(())
    }

    #[test]
    fn basic_auth_navigation_refreshes_past_cached_challenge() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/private").map_err(url_error)?;
        let loader = BasicAuthLoader::new(url.clone(), "webby", "secret");
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, url.clone());
        assert!(matches!(state.status, PageStatus::Error { .. }));
        state.set_basic_auth_credentials(&url, "webby", "secret")?;
        state.navigate_to_url(&loader, url.clone());

        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        assert_eq!(loader.authorized_calls.get(), 1);
        assert!(
            state
                .page
                .as_ref()
                .map(|page| webby_html::extract_visible_text(&page.document).contains("Private"))
                .unwrap_or(false)
        );
        Ok(())
    }

    #[test]
    fn failed_navigation_download_is_structured_app_error() -> WebbyResult<()> {
        let download_url =
            url::Url::parse("https://example.test/archive.bin").map_err(url_error)?;
        let destination_root = temp_download_dir("blocked");
        let _ = std::fs::remove_file(&destination_root);
        let _ = std::fs::remove_dir_all(&destination_root);
        std::fs::write(&destination_root, b"not a directory").map_err(|source| WebbyError::Io {
            path: Some(destination_root.clone()),
            source,
        })?;
        let mut state = AppState::with_window_size(240, 160);
        state.set_download_directory(&destination_root);
        let loader = DownloadResponseLoader::new(
            download_url.clone(),
            "application/octet-stream",
            Vec::new(),
            b"bytes".to_vec(),
        );

        state.navigate_to_url(&loader, download_url.clone());

        assert!(matches!(state.status, PageStatus::Error { .. }));
        assert_eq!(
            state.navigation.failed_url.as_deref(),
            Some(download_url.as_str())
        );
        assert!(state.navigation.history.is_empty());
        assert!(state.downloads.is_empty());
        let _ = std::fs::remove_file(destination_root);
        Ok(())
    }

    #[test]
    fn page_pipeline_rejects_unsupported_binary_top_level_response() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/archive.bin").map_err(url_error)?;
        let loader = DownloadResponseLoader::new(
            url.clone(),
            "application/octet-stream",
            Vec::new(),
            b"bytes".to_vec(),
        );

        let result = PagePipeline::new(240, 160).load_url(&loader, &url);

        assert!(matches!(result, Err(WebbyError::Unsupported { .. })));
        Ok(())
    }

    #[test]
    fn shortcut_help_toggle_changes_composed_shell_pixels() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(420, 220);
        let normal = state.compose_frame()?;
        state.toggle_shortcut_help();
        let help = state.compose_frame()?;

        assert!(state.shortcut_help_visible);
        assert_ne!(normal.pixels, help.pixels);
        Ok(())
    }

    #[test]
    fn debug_overlay_toggle_changes_app_state() {
        let mut state = AppState::new();

        state.toggle_debug_overlay();
        assert!(state.debug_overlay_enabled);
        state.selected_inspection = Some(dummy_inspection());

        state.toggle_debug_overlay();
        assert!(!state.debug_overlay_enabled);
        assert!(state.selected_inspection.is_none());
    }

    #[test]
    fn debug_overlay_rendering_does_not_change_page_layout_dimensions() -> WebbyResult<()> {
        let mut state = loaded_debug_state()?;
        let before_height = state
            .page
            .as_ref()
            .map(|page| page.content_height)
            .ok_or_else(|| WebbyError::invalid_input("test page missing"))?;
        let before_layout = state
            .page
            .as_ref()
            .map(|page| page.layout.clone())
            .ok_or_else(|| WebbyError::invalid_input("test page missing"))?;

        let normal = state.compose_frame()?;
        state.toggle_debug_overlay();
        let debug = state.compose_frame()?;

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("test page missing"))?;
        assert_eq!(page.content_height, before_height);
        assert_eq!(page.layout, before_layout);
        assert_eq!(normal.width, debug.width);
        assert_eq!(normal.height, debug.height);
        Ok(())
    }

    #[test]
    fn debug_overlay_draws_box_model_rectangles() -> WebbyResult<()> {
        let mut state = loaded_debug_state()?;
        let normal = state.compose_frame()?;

        state.toggle_debug_overlay();
        let debug = state.compose_frame()?;

        assert_ne!(normal.pixels, debug.pixels);
        Ok(())
    }

    #[test]
    fn inspect_click_selects_expected_layout_box_and_metadata() -> WebbyResult<()> {
        let mut state = loaded_debug_state()?;
        let rect = first_box_by_tag(&state, "p")?.dimensions.content;
        state.toggle_debug_overlay();

        let info = state
            .inspect_at_window_position(rect.x + 1.0, rect.y + CHROME_HEIGHT as f32 + 1.0)
            .ok_or_else(|| WebbyError::invalid_input("inspection did not select a box"))?;

        assert_eq!(info.tag_name, "p");
        assert_eq!(info.id.as_deref(), Some("intro"));
        assert_eq!(
            info.classes,
            vec!["card".to_string(), "highlighted".to_string()]
        );
        assert_eq!(info.dimensions.content, rect);
        assert!(info.computed_style.contains("background=rgba(0,255,0,255)"));
        assert!(info.computed_style.contains("padding=3.0/3.0/3.0/3.0"));
        assert!(info.paint_summary.contains("display-commands="));
        Ok(())
    }

    #[test]
    fn inspect_click_accounts_for_chrome_height_and_scroll_offset() -> WebbyResult<()> {
        let mut state = loaded_debug_state()?;
        let rect = first_box_by_tag(&state, "p")?.dimensions.content;
        state.scroll_by(4.0);
        state.toggle_debug_overlay();

        let info = state
            .inspect_at_window_position(rect.x + 1.0, rect.y - 4.0 + CHROME_HEIGHT as f32 + 1.0)
            .ok_or_else(|| WebbyError::invalid_input("inspection did not select scrolled box"))?;

        assert_eq!(info.tag_name, "p");
        assert_eq!(info.id.as_deref(), Some("intro"));
        Ok(())
    }

    #[test]
    fn inspect_form_control_exposes_form_metadata() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/search\"><input type=\"search\" name=\"q\"></form>",
        )?;
        let control = state
            .page
            .as_ref()
            .and_then(|page| page.form_controls.first())
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("test form control missing"))?;
        state.toggle_debug_overlay();

        let info = state
            .inspect_at_window_position(
                control.rect.x + 1.0,
                control.rect.y + CHROME_HEIGHT as f32 + 1.0,
            )
            .ok_or_else(|| WebbyError::invalid_input("inspection did not select form control"))?;

        assert_eq!(info.tag_name, "input");
        assert!(
            info.form_metadata
                .is_some_and(|value| value.contains("name=q"))
        );
        Ok(())
    }

    #[test]
    fn app_pipeline_debug_mode_uses_existing_layout_render_data() -> WebbyResult<()> {
        let mut state = loaded_debug_state()?;
        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("test page missing"))?;
        assert_eq!(page.display_list, build_display_list(&page.layout));

        state.toggle_debug_overlay();
        let rect = first_box_by_tag(&state, "p")?.dimensions.content;
        let info = state
            .inspect_at_window_position(rect.x + 1.0, rect.y + CHROME_HEIGHT as f32 + 1.0)
            .ok_or_else(|| WebbyError::invalid_input("inspection did not select a box"))?;

        assert_eq!(
            info.paint_summary,
            state
                .selected_inspection
                .as_ref()
                .map(|item| item.paint_summary.clone())
                .unwrap_or_default()
        );
        Ok(())
    }

    #[test]
    fn app_starts_with_one_tab() {
        let state = AppState::new();

        assert_eq!(state.tab_count(), 1);
        assert_eq!(state.active_tab_index, 0);
        assert_eq!(
            state.active_tab().map(|tab| tab.status.clone()),
            Some(PageStatus::Startup)
        );
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_new_tab() -> WebbyResult<()> {
        let mut state = AppState::new();

        state.new_tab();

        state.validate_active_tab_sync()
    }

    #[test]
    fn new_tab_creates_independent_browsing_context() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home.clone());

        let new_index = state.new_tab();

        assert_eq!(new_index, 1);
        assert_eq!(state.tab_count(), 2);
        assert_eq!(state.navigation.current_url, None);
        assert_eq!(state.status, PageStatus::Startup);
        assert_eq!(state.tabs[0].navigation.current_url.as_ref(), Some(&home));
        Ok(())
    }

    #[test]
    fn switching_tabs_preserves_url_page_scroll_and_history() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let tall_page = "<body><p>one</p><p>two</p><p>three</p><p>four</p><p>five</p><p>six</p><p>seven</p></body>";
        let loader = TestSiteLoader::new([(home.as_str(), tall_page), (next.as_str(), tall_page)]);
        let mut state = AppState::with_window_size(240, 100);
        state.navigate_to_url(&loader, home.clone());
        state.navigate_to_url(&loader, next.clone());
        state.scroll_by(24.0);
        state.new_tab();
        state.navigate_to_url(&loader, home.clone());
        state.scroll_by(7.0);

        assert!(state.switch_tab(0));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        assert_eq!(state.navigation.history, vec![home.clone(), next]);
        assert_eq!(state.scroll_y, 24.0);
        assert!(state.page.is_some());

        assert!(state.switch_tab(1));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert_eq!(state.navigation.history, vec![home]);
        assert_eq!(state.scroll_y, 7.0);
        Ok(())
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_switch_tab() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home);
        state.new_tab();

        assert!(state.switch_tab(0));

        state.validate_active_tab_sync()
    }

    #[test]
    fn closing_inactive_tab_preserves_active_tab() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home.clone());
        state.new_tab();
        assert!(state.switch_tab(0));

        assert!(state.close_tab(1));

        assert_eq!(state.tab_count(), 1);
        assert_eq!(state.active_tab_index, 0);
        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        Ok(())
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_close_inactive_tab() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home);
        state.new_tab();
        assert!(state.switch_tab(0));

        assert!(state.close_tab(1));

        state.validate_active_tab_sync()
    }

    #[test]
    fn closing_active_tab_selects_deterministic_next_tab() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let middle = url::Url::parse("https://example.test/middle").map_err(url_error)?;
        let last = url::Url::parse("https://example.test/last").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (home.as_str(), "<body>Home</body>"),
            (middle.as_str(), "<body>Middle</body>"),
            (last.as_str(), "<body>Last</body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home);
        state.new_tab();
        state.navigate_to_url(&loader, middle.clone());
        state.new_tab();
        state.navigate_to_url(&loader, last.clone());
        assert!(state.switch_tab(1));

        assert!(state.close_active_tab());

        assert_eq!(state.active_tab_index, 1);
        assert_eq!(state.navigation.current_url.as_ref(), Some(&last));
        assert_eq!(state.tab_count(), 2);
        assert_ne!(state.navigation.current_url.as_ref(), Some(&middle));
        Ok(())
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_close_active_tab() -> WebbyResult<()> {
        let first = url::Url::parse("https://example.test/first").map_err(url_error)?;
        let second = url::Url::parse("https://example.test/second").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (first.as_str(), "<body>First</body>"),
            (second.as_str(), "<body>Second</body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, first);
        state.new_tab();
        state.navigate_to_url(&loader, second);

        assert!(state.close_active_tab());

        state.validate_active_tab_sync()
    }

    #[test]
    fn cannot_close_last_tab() {
        let mut state = AppState::new();

        assert!(!state.close_active_tab());
        assert_eq!(state.tab_count(), 1);
        assert_eq!(state.active_tab_index, 0);
    }

    #[test]
    fn closing_last_tab_is_false_noop_and_stays_synchronized() -> WebbyResult<()> {
        let mut state = AppState::new();
        let before = state.clone();

        assert!(!state.close_active_tab());

        assert_eq!(state, before);
        state.validate_active_tab_sync()
    }

    #[test]
    fn navigation_affects_active_tab_only() -> WebbyResult<()> {
        let first = url::Url::parse("https://example.test/first").map_err(url_error)?;
        let second = url::Url::parse("https://example.test/second").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (first.as_str(), "<body>First</body>"),
            (second.as_str(), "<body>Second</body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, first.clone());
        state.new_tab();
        state.navigate_to_url(&loader, second.clone());

        assert_eq!(state.navigation.current_url.as_ref(), Some(&second));
        assert_eq!(state.tabs[0].navigation.current_url.as_ref(), Some(&first));
        assert_eq!(state.tabs[1].navigation.current_url.as_ref(), Some(&second));
        Ok(())
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_navigation() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home);

        state.validate_active_tab_sync()
    }

    #[test]
    fn back_forward_and_reload_affect_active_tab_only() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let other = url::Url::parse("https://example.test/other").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (home.as_str(), "<body>Home</body>"),
            (next.as_str(), "<body>Next</body>"),
            (other.as_str(), "<body>Other</body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home.clone());
        state.navigate_to_url(&loader, next.clone());
        state.new_tab();
        state.navigate_to_url(&loader, other.clone());

        assert!(!state.go_back(&loader));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&other));
        assert!(state.reload(&loader));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&other));

        assert!(state.switch_tab(0));
        assert!(state.go_back(&loader));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert!(state.go_forward(&loader));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));

        assert!(state.switch_tab(1));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&other));
        Ok(())
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_reload() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home);

        assert!(state.reload(&loader));

        state.validate_active_tab_sync()
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_back_and_forward() -> WebbyResult<()> {
        let (mut state, loader, _, _) = two_page_history_state()?;

        assert!(state.go_back(&loader));
        state.validate_active_tab_sync()?;
        assert!(state.go_forward(&loader));

        state.validate_active_tab_sync()
    }

    #[test]
    fn scroll_offset_is_per_tab() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            home.as_str(),
            "<body><p>one</p><p>two</p><p>three</p><p>four</p><p>five</p></body>",
        )]);
        let mut state = AppState::with_window_size(180, 90);
        state.navigate_to_url(&loader, home.clone());
        state.scroll_by(30.0);
        state.new_tab();
        state.navigate_to_url(&loader, home);
        state.scroll_by(5.0);

        assert!(state.switch_tab(0));
        assert_eq!(state.scroll_y, 30.0);
        assert!(state.switch_tab(1));
        assert_eq!(state.scroll_y, 5.0);
        Ok(())
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_scroll() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            home.as_str(),
            "<body><p>one</p><p>two</p><p>three</p><p>four</p><p>five</p></body>",
        )]);
        let mut state = AppState::with_window_size(180, 90);
        state.navigate_to_url(&loader, home);

        state.scroll_by(12.0);

        state.validate_active_tab_sync()
    }

    #[test]
    fn form_focus_and_edit_state_is_per_tab() -> WebbyResult<()> {
        let mut state =
            loaded_form_state("<form><input type=\"text\" name=\"q\" value=\"one\"></form>")?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;
        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));
        state.type_character('!');
        let first_control = state.focused_form_control;
        state.new_tab();
        let second =
            loaded_form_state("<form><input type=\"text\" name=\"q\" value=\"two\"></form>")?;
        state.finish_navigation(Ok(second
            .page
            .ok_or_else(|| WebbyError::invalid_input("test form page missing"))?));

        assert!(state.focused_form_control.is_none());
        assert!(state.form_values.is_empty());
        assert!(state.switch_tab(0));
        assert_eq!(state.focused_form_control, first_control);
        assert!(
            first_control
                .and_then(|id| state.form_values.get(&id))
                .is_some_and(|value| value == "one!")
        );
        Ok(())
    }

    #[test]
    fn active_tab_state_stays_synchronized_after_form_focus_and_edit() -> WebbyResult<()> {
        let mut state =
            loaded_form_state("<form><input type=\"text\" name=\"q\" value=\"one\"></form>")?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));
        state.type_character('!');

        state.validate_active_tab_sync()
    }

    #[test]
    fn failed_navigation_state_is_per_tab() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let missing = url::Url::parse("https://example.test/missing").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home.clone());
        state.new_tab();
        state.navigate_to_url(&FailingLoader, missing.clone());

        assert_eq!(
            state.navigation.failed_url.as_deref(),
            Some(missing.as_str())
        );
        assert!(matches!(state.status, PageStatus::Error { .. }));
        assert!(state.switch_tab(0));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert_eq!(state.navigation.failed_url, None);
        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        Ok(())
    }

    #[test]
    fn typing_updates_address_bar_state() {
        let mut state = AppState::new();
        state.chrome.set_address_input("");
        state.type_character('r');
        state.type_character('s');
        state.backspace();
        state.type_character('t');

        assert_eq!(state.chrome.address_input, "rt");
    }

    #[test]
    fn enter_triggers_url_search_resolution() -> WebbyResult<()> {
        let url = url::Url::parse("https://www.google.com/search?q=rust+browser+engine")
            .map_err(url_error)?;
        let loader = StaticHtmlLoader::new("<body>Search</body>", url.clone());
        let mut state = AppState::with_window_size(320, 240);
        state.chrome.set_address_input("rust browser engine");

        state.submit_address(&loader);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&url));
        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        Ok(())
    }

    #[test]
    fn invalid_input_produces_error_state_not_panic() {
        let loader = FailingLoader;
        let mut state = AppState::new();
        state.chrome.set_address_input("   ");

        state.submit_address(&loader);

        assert!(matches!(state.status, PageStatus::Error { .. }));
    }

    #[test]
    fn successful_page_load_updates_current_url_and_content_state() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = StaticHtmlLoader::new("<body><h1>Loaded</h1></body>", url.clone());
        let mut state = AppState::with_window_size(320, 240);
        state.chrome.set_address_input("example.test");

        state.submit_address(&loader);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&url));
        assert!(state.page.is_some());
        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        Ok(())
    }

    #[test]
    fn begin_navigation_sets_loading_and_pending_url() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(160, 120);
        let pending_url = begin_url(&mut state, "example.test")?;

        assert_eq!(state.navigation.pending_url.as_ref(), Some(&pending_url));
        assert_eq!(
            state.navigation.lifecycle,
            NavigationLifecycle::LoadingMainResource
        );
        assert_eq!(state.navigation.generation, 1);
        assert!(matches!(
            state.status,
            PageStatus::Loading { ref url } if url == pending_url.as_str()
        ));
        Ok(())
    }

    #[test]
    fn starting_new_navigation_cancels_previous_pending_generation() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(160, 120);
        let first = begin_url(&mut state, "first.example")?;
        let first_generation = state.navigation.generation;

        state.chrome.set_address_input("second.example");
        let second = state
            .begin_navigation()
            .ok_or_else(|| WebbyError::invalid_input("second navigation did not resolve"))?;

        assert_ne!(first, second);
        assert_eq!(state.navigation.pending_url.as_ref(), Some(&second));
        assert_eq!(state.navigation.generation, first_generation + 1);
        assert!(state.navigation.lifecycle_diagnostics.iter().any(|item| {
            item.contains("navigation cancelled")
                && item.contains(first.as_str())
                && item.contains(second.as_str())
        }));
        Ok(())
    }

    #[test]
    fn stale_navigation_generation_cannot_replace_current_page() -> WebbyResult<()> {
        let stale_url = url::Url::parse("https://stale.example/").map_err(url_error)?;
        let current_url = url::Url::parse("https://current.example/").map_err(url_error)?;
        let stale_page =
            PagePipeline::new(160, 80).render_html("<body>Stale</body>", stale_url.clone())?;
        let current_page =
            PagePipeline::new(160, 80).render_html("<body>Current</body>", current_url.clone())?;
        let mut state = AppState::with_window_size(160, 120);

        state.begin_navigation_to_url(stale_url, PendingHistoryAction::Push);
        let stale_generation = state.navigation.generation;
        state.begin_navigation_to_url(current_url.clone(), PendingHistoryAction::Push);
        let current_generation = state.navigation.generation;
        state.finish_navigation_for_generation(current_generation, Ok(current_page));
        state.finish_navigation_for_generation(stale_generation, Ok(stale_page));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&current_url));
        assert_eq!(state.navigation.history, vec![current_url]);
        assert!(
            state
                .navigation
                .lifecycle_diagnostics
                .iter()
                .any(|item| item.contains("late navigation result ignored"))
        );
        Ok(())
    }

    #[test]
    fn scroll_offset_changes_and_clamps_safely() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = StaticHtmlLoader::new(
            "<body><p>one</p><p>two</p><p>three</p><p>four</p><p>five</p></body>",
            url,
        );
        let mut state = AppState::with_window_size(180, 90);
        state.chrome.set_address_input("example.test");
        state.submit_address(&loader);

        state.scroll_by(10_000.0);
        let high_scroll = state.scroll_y;
        state.scroll_by(-10_000.0);

        assert!(high_scroll >= 0.0);
        assert_eq!(state.scroll_y, 0.0);
        Ok(())
    }

    #[test]
    fn nested_wheel_scroll_is_per_tab_and_does_not_change_page_scroll() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/overflow").map_err(url_error)?;
        let html = "<style>.clip { height: 24px; overflow-y: auto; } .spacer { height: 90px; }</style><body><div class=\"clip\"><div class=\"spacer\"></div><a href=\"/shown\">Shown</a></div></body>";
        let page = PagePipeline::new(240, 120).render_html(html, url)?;
        let container = page
            .layout
            .scroll_containers()
            .first()
            .copied()
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("missing nested scroll container"))?;
        let mut state = AppState::with_window_size(240, 168);
        state.finish_navigation(Ok(page));

        state.scroll_at_window_position(
            container.viewport.x + 1.0,
            CHROME_HEIGHT as f32 + container.viewport.y + 1.0,
            48.0,
        );

        assert_eq!(state.scroll_y, 0.0);
        assert!(
            state
                .scroll_offsets
                .get(&container.id)
                .is_some_and(|offset| offset.y > 0.0)
        );
        state.resize(241, 168);
        assert!(
            state
                .scroll_offsets
                .get(&container.id)
                .is_some_and(|offset| offset.y > 0.0)
        );
        state.validate_active_tab_sync()?;

        state.new_tab();
        assert!(state.scroll_offsets.is_empty());
        assert!(state.switch_tab(0));
        assert!(
            state
                .scroll_offsets
                .get(&container.id)
                .is_some_and(|offset| offset.y > 0.0)
        );
        Ok(())
    }

    #[test]
    fn wheel_scroll_prefers_deepest_visible_nested_container() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/nested-overflow").map_err(url_error)?;
        let html = "<style>.outer { height: 60px; overflow-y: auto; } .inner { height: 24px; overflow-y: auto; } .spacer { height: 90px; }</style><body><div class=\"outer\"><div class=\"inner\"><div class=\"spacer\"></div><a href=\"/inner\">Inner</a></div><div class=\"spacer\"></div></div></body>";
        let page = PagePipeline::new(240, 120).render_html(html, url)?;
        let containers = page.layout.scroll_containers();
        let outer = containers
            .first()
            .copied()
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("missing outer scroll container"))?;
        let inner = containers
            .get(1)
            .copied()
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("missing inner scroll container"))?;
        let mut state = AppState::with_window_size(240, 168);
        state.finish_navigation(Ok(page));

        state.scroll_at_window_position(
            inner.viewport.x + 1.0,
            CHROME_HEIGHT as f32 + inner.viewport.y + 1.0,
            48.0,
        );

        assert!(!state.scroll_offsets.contains_key(&outer.id));
        assert!(
            state
                .scroll_offsets
                .get(&inner.id)
                .is_some_and(|offset| offset.y > 0.0)
        );
        state.validate_active_tab_sync()?;
        Ok(())
    }

    #[test]
    fn browser_chrome_height_is_respected_for_page_content() {
        let state = AppState::with_window_size(320, 240);

        assert_eq!(state.page_viewport_height(), 240 - CHROME_HEIGHT);
    }

    #[test]
    fn page_render_uses_display_list_and_software_renderer_output() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let pipeline = PagePipeline::new(160, 120);

        assert_eq!(pipeline.render_backend_name(), "software");

        let page = pipeline.render_html("<body>Hello</body>", url)?;

        assert!(!page.display_list.commands.is_empty());
        assert_eq!(page.surface.width, 160);
        assert!(!page.surface.pixels.iter().all(|value| *value == 255));
        Ok(())
    }

    #[test]
    fn page_pipeline_executes_inline_javascript_and_captures_console() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(160, 120).render_html(
            "<body><script>console.log('page', 31);</script><p>Visible</p></body>",
            url,
        )?;

        assert_eq!(
            page.diagnostics,
            vec!["JavaScript console: page 31".to_string()]
        );
        assert!(page.content_height > 0.0);
        Ok(())
    }

    #[test]
    fn javascript_text_content_mutation_changes_rendered_output() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(180, 120).render_html(
            "<body><p id=\"intro\">Old</p><script>document.getElementById('intro').textContent = 'New text';</script></body>",
            url,
        )?;

        let rendered_text = display_texts(&page).join(" ");
        assert!(rendered_text.contains("New"));
        assert!(rendered_text.contains("text"));
        assert!(!rendered_text.contains("Old"));
        Ok(())
    }

    #[test]
    fn javascript_append_child_mutation_changes_rendered_output() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(180, 120).render_html(
            "<body><main id=\"app\"></main><script>var p = document.createElement('p'); p.textContent = 'Added'; document.getElementById('app').appendChild(p);</script></body>",
            url,
        )?;

        assert!(display_texts(&page).join(" ").contains("Added"));
        Ok(())
    }

    #[test]
    fn shadow_root_content_renders_instead_of_light_dom() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(220, 140).render_html(
            "<body><x-card id=\"card\">Light fallback</x-card><script>var root = document.getElementById('card').attachShadow({ mode: 'open' }); var p = document.createElement('p'); p.textContent = 'Shadow content'; root.appendChild(p);</script></body>",
            url,
        )?;

        let text = display_texts(&page).join(" ");
        assert!(text.contains("Shadow"));
        assert!(text.contains("content"));
        assert!(!text.contains("Light"));
        Ok(())
    }

    #[test]
    fn custom_element_fixture_renders_shadow_content_and_styles() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(220, 140).render_html(
            "<body><x-greeting></x-greeting><script>function Greeting() {} Greeting.prototype.connectedCallback = function() { var root = this.attachShadow({ mode: 'open' }); var style = document.createElement('style'); style.textContent = 'span { color: red; }'; var span = document.createElement('span'); span.textContent = 'Hello shadow'; root.appendChild(style); root.appendChild(span); }; customElements.define('x-greeting', Greeting);</script></body>",
            url,
        )?;

        let text = display_texts(&page).join(" ");
        assert!(text.contains("Hello"));
        assert!(text.contains("shadow"));
        assert!(page.display_list.commands.iter().any(|command| matches!(
            command,
            webby_render::DisplayCommand::DrawText { text, color, .. }
                if text.starts_with("Hello") && color.r == 255 && color.g == 0 && color.b == 0
        )));
        Ok(())
    }

    #[test]
    fn unsupported_custom_element_features_are_diagnostics() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(220, 140).render_html(
            "<body><x-card id=\"card\"></x-card><script>document.getElementById('card').attachShadow({ mode: 'closed' });</script></body>",
            url,
        )?;

        assert!(page.diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("JavaScript Shadow DOM unsupported attachShadow mode")
        }));
        Ok(())
    }

    #[test]
    fn javascript_set_attribute_triggers_css_rematching() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(180, 120).render_html(
            "<style>.hot { color: red; }</style><body><p id=\"intro\">Text</p><script>document.getElementById('intro').className = 'hot';</script></body>",
            url,
        )?;

        assert!(page.display_list.commands.iter().any(|command| matches!(
            command,
            webby_render::DisplayCommand::DrawText { text, color, .. }
                if text == "Text" && color.r == 255 && color.g == 0 && color.b == 0
        )));
        Ok(())
    }

    #[test]
    fn javascript_query_selector_mutates_supported_selector_target() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(180, 120).render_html(
            "<body><p class=\"note\">Old</p><p>Other</p><script>document.querySelector('p.note').textContent = 'Hit';</script></body>",
            url,
        )?;

        let rendered_text = display_texts(&page).join(" ");
        assert!(rendered_text.contains("Hit"));
        assert!(rendered_text.contains("Other"));
        assert!(!rendered_text.contains("Old"));
        Ok(())
    }

    #[test]
    fn click_event_handler_mutates_dom_and_prevents_link_navigation() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<body><p id=\"out\">Old</p><a id=\"link\" href=\"/next\">Go</a><script>document.getElementById('link').addEventListener('click', function(event) { document.getElementById('out').textContent = 'Clicked'; event.preventDefault(); });</script></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url.clone())?;
        let mut state = AppState::with_window_size(220, 188);
        state.finish_navigation(Ok(page));
        let (x, y) = first_link_point(&state)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&url));
        assert!(
            display_texts(
                state
                    .page
                    .as_ref()
                    .ok_or_else(|| WebbyError::invalid_input("missing page"))?
            )
            .join(" ")
            .contains("Clicked")
        );
        Ok(())
    }

    #[test]
    fn click_events_bubble_in_deterministic_order() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<body><div id=\"outer\"><a id=\"link\" href=\"/next\">Go</a></div><script>document.getElementById('link').addEventListener('click', function(event) { console.log('target', event.target.id, event.currentTarget.id); event.preventDefault(); }); document.getElementById('outer').addEventListener('click', function(event) { console.log('outer', event.target.id, event.currentTarget.id); });</script></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url)?;
        let mut state = AppState::with_window_size(220, 188);
        state.finish_navigation(Ok(page));
        let (x, y) = first_link_point(&state)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));

        let diagnostics = &state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?
            .diagnostics;
        assert!(diagnostics.ends_with(&[
            "JavaScript console: target link link".to_string(),
            "JavaScript console: outer link outer".to_string()
        ]));
        Ok(())
    }

    #[test]
    fn event_handlers_see_current_location_href() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/events").map_err(url_error)?;
        let html = "<body><button id=\"b\">Go</button><script>document.getElementById('b').addEventListener('click', function() { console.log(location.href); });</script></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url.clone())?;
        let mut state = AppState::with_window_size(220, 188);
        state.finish_navigation(Ok(page));
        let button = state
            .page
            .as_ref()
            .and_then(|page| page.form_controls.first())
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("missing button"))?;

        assert!(state.click_at(
            button.rect.x + 1.0,
            button.rect.y + CHROME_HEIGHT as f32 + 1.0,
            &FailingLoader
        ));

        let diagnostics = state
            .page
            .as_ref()
            .map(|page| page.diagnostics.clone())
            .unwrap_or_default();
        assert!(diagnostics.contains(&format!("JavaScript console: {url}")));
        Ok(())
    }

    #[test]
    fn event_handlers_receive_layout_backed_geometry() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/events").map_err(url_error)?;
        let html = "<body><button id=\"b\" style=\"width: 72px; height: 24px\">Go</button><script>document.getElementById('b').addEventListener('click', function(event) { var rect = event.currentTarget.getBoundingClientRect(); console.log(rect.width, rect.height, event.currentTarget.clientWidth, window.innerWidth); });</script></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url)?;
        let mut state = AppState::with_window_size(220, 188);
        state.finish_navigation(Ok(page));
        let button = state
            .page
            .as_ref()
            .and_then(|page| page.form_controls.first())
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("missing button"))?;

        assert!(state.click_at(
            button.rect.x + 1.0,
            button.rect.y + CHROME_HEIGHT as f32 + 1.0,
            &FailingLoader
        ));

        let diagnostics = state
            .page
            .as_ref()
            .map(|page| page.diagnostics.clone())
            .unwrap_or_default();
        assert!(
            diagnostics
                .iter()
                .any(|entry| { entry == "JavaScript console: 90 32 90 220" }),
            "{diagnostics:?}"
        );
        Ok(())
    }

    #[test]
    fn click_handler_location_href_navigates_once_through_app_state() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/events").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><button id=\"b\">Go</button><script>document.getElementById('b').addEventListener('click', function() { location.href = '/next'; });</script></body>",
            ),
            (next.as_str(), "<body><p>Next</p></body>"),
        ]);
        let mut state = AppState::with_window_size(240, 180);
        state.navigate_to_url(&loader, home.clone());
        let button = state
            .page
            .as_ref()
            .and_then(|page| page.form_controls.first())
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("missing button"))?;

        assert!(state.click_at(
            button.rect.x + 1.0,
            button.rect.y + CHROME_HEIGHT as f32 + 1.0,
            &loader
        ));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        assert_eq!(state.navigation.history, vec![home, next]);
        Ok(())
    }

    #[test]
    fn click_event_uses_hit_tested_link_node_when_hrefs_match() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<body><p id=\"out\">Old</p><a id=\"first\" href=\"/same\">First</a> <a id=\"second\" href=\"/same\">Second</a><script>document.getElementById('first').addEventListener('click', function(event) { document.getElementById('out').textContent = 'First clicked'; event.preventDefault(); }); document.getElementById('second').addEventListener('click', function(event) { document.getElementById('out').textContent = 'Second clicked'; event.preventDefault(); });</script></body>";
        let page = PagePipeline::new(260, 140).render_html(html, url)?;
        let mut state = AppState::with_window_size(260, 188);
        state.finish_navigation(Ok(page));
        let second = state
            .page
            .as_ref()
            .and_then(|page| page.links.last())
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("missing second link"))?;

        assert!(state.click_at(
            second.rect.x + 1.0,
            second.rect.y + CHROME_HEIGHT as f32 + 1.0,
            &FailingLoader
        ));

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert!(display_texts(page).join("").contains("Second clicked"));
        assert!(!display_texts(page).join("").contains("First clicked"));
        Ok(())
    }

    #[test]
    fn input_event_fires_when_form_value_changes_and_mutates_dom() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<p id=\"out\">Old</p><form id=\"f\"><input id=\"q\" type=\"text\" name=\"q\"><script>document.getElementById('f').addEventListener('input', function(event) { document.getElementById('out').textContent = 'Input fired'; });</script></form>",
        )?;
        assert!(
            state
                .page
                .as_ref()
                .is_some_and(|page| !page.event_handlers.is_empty())
        );
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));
        assert!(state.focused_form_control.is_some());
        assert!(!state.chrome.address_focused);
        assert!(
            state
                .page
                .as_ref()
                .is_some_and(|page| !page.event_handlers.is_empty())
        );
        state.type_character('a');

        assert_eq!(state.form_values.get(&0).map(String::as_str), Some("a"));
        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert!(
            display_texts(page).join("").contains("Input fired"),
            "text={:?} display={:?} diagnostics={:?} handlers={:?}",
            webby_html::extract_visible_text(&page.document),
            display_texts(page),
            page.diagnostics,
            page.event_handlers
        );
        Ok(())
    }

    #[test]
    fn submit_event_prevent_default_stops_form_navigation() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/form").map_err(url_error)?;
        let html = "<body><p id=\"out\">Old</p><form id=\"f\" action=\"/next\"><input type=\"text\" name=\"q\"><script>document.getElementById('f').addEventListener('submit', function(event) { document.getElementById('out').textContent = 'Stopped'; event.preventDefault(); });</script></form></body>";
        let page = PagePipeline::new(240, 160).render_html(html, url.clone())?;
        let mut state = AppState::with_window_size(240, 208);
        state.finish_navigation(Ok(page));
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));
        assert!(state.submit_focused_form(&FailingLoader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&url));
        assert!(
            display_texts(
                state
                    .page
                    .as_ref()
                    .ok_or_else(|| WebbyError::invalid_input("missing page"))?
            )
            .join(" ")
            .contains("Stopped")
        );
        Ok(())
    }

    #[test]
    fn javascript_location_href_returns_current_url() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            home.as_str(),
            "<body><script>console.log(location.href);</script><p>Home</p></body>",
        )]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home.clone());

        let diagnostics = state
            .page
            .as_ref()
            .map(|page| page.diagnostics.clone())
            .unwrap_or_default();
        assert!(diagnostics.contains(&format!("JavaScript console: {home}")));
        Ok(())
    }

    #[test]
    fn javascript_location_href_set_navigates_through_app_state() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><script>location.href = '/next';</script><p>Home</p></body>",
            ),
            (next.as_str(), "<body><p>Next</p></body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home.clone());

        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        assert_eq!(state.navigation.history, vec![home, next]);
        Ok(())
    }

    #[test]
    fn javascript_location_assign_navigates_relative_url() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/dir/home").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/dir/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><script>location.assign('next');</script><p>Home</p></body>",
            ),
            (next.as_str(), "<body><p>Next</p></body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        Ok(())
    }

    #[test]
    fn javascript_history_back_uses_existing_history_model() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (home.as_str(), "<body><p>Home</p></body>"),
            (
                next.as_str(),
                "<body><script>history.back();</script><p>Next</p></body>",
            ),
        ]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home.clone());
        state.navigate_to_url(&loader, next.clone());

        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert_eq!(state.navigation.history, vec![home, next]);
        assert_eq!(state.navigation.history_index, Some(0));
        Ok(())
    }

    #[test]
    fn javascript_history_forward_uses_existing_history_model() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><script>history.forward();</script><p>Home</p></body>",
            ),
            (next.as_str(), "<body><p>Next</p></body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home.clone());
        state.navigate_to_url(&loader, next.clone());
        assert!(state.go_back(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        assert_eq!(state.navigation.history, vec![home, next]);
        assert_eq!(state.navigation.history_index, Some(1));
        Ok(())
    }

    #[test]
    fn set_timeout_callback_mutates_dom_and_rerenders() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            home.as_str(),
            "<body><p id=\"out\">Old</p><script>setTimeout(function() { document.getElementById('out').textContent = 'Timer'; }, 0);</script></body>",
        )]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home);

        assert_eq!(state.timers.len(), 1);
        assert_eq!(state.active_tab().map(|tab| tab.timers.len()), Some(1));
        assert!(state.run_due_timers(&loader));
        let text = state
            .page
            .as_ref()
            .map(display_texts)
            .unwrap_or_default()
            .join("");
        assert!(text.contains("Timer"));
        assert!(state.timers.is_empty());
        state.validate_active_tab_sync()?;
        Ok(())
    }

    #[test]
    fn clear_timeout_cancels_callback() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            home.as_str(),
            "<body><p id=\"out\">Old</p><script>var timer = setTimeout(function() { document.getElementById('out').textContent = 'Timer'; }, 0); clearTimeout(timer);</script></body>",
        )]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home);

        assert!(state.timers.is_empty());
        assert!(!state.run_due_timers(&loader));
        let text = state
            .page
            .as_ref()
            .map(display_texts)
            .unwrap_or_default()
            .join("");
        assert!(text.contains("Old"));
        Ok(())
    }

    #[test]
    fn navigation_cancels_old_page_timers() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><p id=\"out\">Old</p><script>setTimeout(function() { document.getElementById('out').textContent = 'Timer'; }, 0);</script></body>",
            ),
            (next.as_str(), "<body><p>Next</p></body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home);
        assert_eq!(state.timers.len(), 1);
        state.navigate_to_url(&loader, next.clone());

        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        assert!(state.timers.is_empty());
        assert!(!state.run_due_timers(&loader));
        Ok(())
    }

    #[test]
    fn stale_timer_generation_cannot_mutate_replaced_page() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><p id=\"out\">Old</p><script>setTimeout(function() { document.getElementById('out').textContent = 'Timer'; }, 0);</script></body>",
            ),
            (next.as_str(), "<body><p>Next</p></body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home);
        let stale_timer = state
            .timers
            .first()
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("test timer was not scheduled"))?;
        state.navigate_to_url(&loader, next);
        state.timers.push(stale_timer);

        assert!(!state.run_due_timers(&loader));
        let text = state
            .page
            .as_ref()
            .map(display_texts)
            .unwrap_or_default()
            .join("");
        assert!(text.contains("Next"));
        assert!(!text.contains("Timer"));
        assert!(
            state
                .navigation
                .lifecycle_diagnostics
                .iter()
                .any(|item| item.contains("late timer ignored"))
        );
        Ok(())
    }

    #[test]
    fn javascript_fetch_loads_same_origin_text_and_mutates_dom() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let data = url::Url::parse("https://example.test/data.txt").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><p id=\"out\">Old</p><script>var response = fetch('/data.txt'); document.getElementById('out').textContent = response.status + ':' + response.ok + ':' + response.text();</script></body>",
            ),
            (data.as_str(), "Fetched body"),
        ]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, home);

        let text = state
            .page
            .as_ref()
            .map(display_texts)
            .unwrap_or_default()
            .join("");
        assert!(text.contains("200:true:Fetched body"));
        Ok(())
    }

    #[test]
    fn javascript_fetch_relative_url_resolves_against_current_page() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/posts/home").map_err(url_error)?;
        let data = url::Url::parse("https://example.test/posts/data.txt").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><p id=\"out\">Old</p><script>document.getElementById('out').textContent = fetch('data.txt').url;</script></body>",
            ),
            (data.as_str(), "Fetched body"),
        ]);
        let mut state = AppState::with_window_size(300, 180);

        state.navigate_to_url(&loader, home);

        let text = state
            .page
            .as_ref()
            .map(display_texts)
            .unwrap_or_default()
            .join("");
        assert!(text.contains(data.as_str()));
        Ok(())
    }

    #[test]
    fn javascript_fetch_cross_origin_is_blocked_by_default() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let other = url::Url::parse("https://other.test/data.txt").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><p id=\"out\">Old</p><script>document.getElementById('out').textContent = fetch('https://other.test/data.txt').text();</script></body>",
            ),
            (other.as_str(), "Cross origin"),
        ]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, home);

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert!(display_texts(page).join("").contains("Old"));
        assert!(
            page.diagnostics
                .iter()
                .any(|item| item.contains("CORS blocked"))
        );
        Ok(())
    }

    #[test]
    fn javascript_fetch_cross_origin_with_cors_header_is_allowed() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let other = url::Url::parse("https://other.test/data.txt").map_err(url_error)?;
        let loader = HeaderSiteLoader::new([
            (
                home.as_str(),
                "<body><p id=\"out\">Old</p><script>document.getElementById('out').textContent = fetch('https://other.test/data.txt').text();</script></body>",
                Vec::new(),
            ),
            (
                other.as_str(),
                "Cross origin",
                vec![("Access-Control-Allow-Origin".to_string(), "*".to_string())],
            ),
        ]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, home);

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert!(display_texts(page).join("").contains("Cross origin"));
        assert!(
            !page
                .diagnostics
                .iter()
                .any(|item| item.contains("CORS blocked"))
        );
        Ok(())
    }

    #[test]
    fn mixed_content_and_csp_diagnostics_are_deterministic() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            home.as_str(),
            "<head><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'self'\"><link rel=\"stylesheet\" href=\"http://cdn.example.test/site.css\"></head><body><img src=\"http://cdn.example.test/pic.png\"></body>",
        )]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, home);

        let diagnostics = state
            .page
            .as_ref()
            .map(|page| page.diagnostics.clone())
            .unwrap_or_default();
        assert!(diagnostics.iter().any(|item| {
            item == "mixed content diagnostic: HTTPS page https://example.test/home referenced insecure resource http://cdn.example.test/site.css"
        }));
        assert!(diagnostics.iter().any(|item| {
            item == "mixed content diagnostic: HTTPS page https://example.test/home referenced insecure resource http://cdn.example.test/pic.png"
        }));
        assert!(diagnostics.iter().any(|item| {
            item == "Content-Security-Policy diagnostic: policy observed but not enforced"
        }));
        Ok(())
    }

    #[test]
    fn javascript_fetch_failed_load_is_controlled_diagnostic() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            home.as_str(),
            "<body><p id=\"out\">Old</p><script>document.getElementById('out').textContent = fetch('/missing.txt').text();</script></body>",
        )]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, home);

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert!(display_texts(page).join("").contains("Old"));
        assert!(page.diagnostics.iter().any(|item| {
            item.contains("JavaScript request https://example.test/missing.txt failed to load")
        }));
        Ok(())
    }

    #[test]
    fn javascript_xhr_loads_same_origin_text_and_runs_onload() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let data = url::Url::parse("https://example.test/data.txt").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><p id=\"out\">Old</p><script>var xhr = new XMLHttpRequest(); xhr.onload = function() { document.getElementById('out').textContent = xhr.status + ':' + xhr.responseText; }; xhr.open('GET', '/data.txt'); xhr.send();</script></body>",
            ),
            (data.as_str(), "XHR body"),
        ]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, home);

        let text = state
            .page
            .as_ref()
            .map(display_texts)
            .unwrap_or_default()
            .join("");
        assert!(text.contains("200:XHR body"));
        Ok(())
    }

    #[test]
    fn href_mutation_updates_link_hit_metadata_without_resource_reload() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/home").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/changed").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><a id=\"link\" href=\"/old\">Go</a><script>document.getElementById('link').setAttribute('href', '/changed');</script></body>",
            ),
            (next.as_str(), "<body>Changed</body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home);

        let link = state
            .page
            .as_ref()
            .and_then(|page| page.links.first())
            .cloned()
            .ok_or_else(|| WebbyError::invalid_input("missing link"))?;
        assert_eq!(link.href, "/changed");
        assert!(
            state
                .page
                .as_ref()
                .is_some_and(|page| page.dirty.dom && page.dirty.layout && page.dirty.render)
        );
        assert!(state.click_at(
            link.rect.x + 1.0,
            link.rect.y + CHROME_HEIGHT as f32 + 1.0,
            &loader
        ));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        Ok(())
    }

    #[test]
    fn form_action_mutation_updates_submission_metadata() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/form").map_err(url_error)?;
        let changed = url::Url::parse("https://example.test/changed?q=webby").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                home.as_str(),
                "<body><form id=\"f\" action=\"/old\"><input name=\"q\" value=\"webby\"><script>document.getElementById('f').setAttribute('action', '/changed');</script></form></body>",
            ),
            (changed.as_str(), "<body>Changed</body>"),
        ]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, home);
        assert!(state.submit_form_for_control(&loader, 0, None));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&changed));
        Ok(())
    }

    #[test]
    fn hidden_elements_do_not_receive_click_events() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>#hidden { display: none; }</style><body><p id=\"out\">Old</p><a id=\"hidden\" href=\"/next\">Hidden</a><script>document.getElementById('hidden').addEventListener('click', function(event) { document.getElementById('out').textContent = 'Clicked'; event.preventDefault(); });</script></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url)?;
        let mut state = AppState::with_window_size(220, 188);
        state.finish_navigation(Ok(page));

        assert!(!state.click_at(10.0, CHROME_HEIGHT as f32 + 10.0, &FailingLoader));

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert!(webby_html::extract_visible_text(&page.document).contains("Old"));
        assert!(!display_texts(page).join("").contains("Clicked"));
        Ok(())
    }

    #[test]
    fn javascript_errors_do_not_prevent_page_rendering() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(160, 120).render_html(
            "<body><script>document.querySelector('div + p')</script><p>Still rendered</p></body>",
            url,
        )?;

        assert!(
            page.diagnostics
                .iter()
                .any(|item| item.contains("JavaScript error in inline script 1"))
        );
        assert!(page.display_list.commands.iter().any(|command| matches!(
            command,
            webby_render::DisplayCommand::DrawText { text, .. } if text.contains("Still")
        )));
        Ok(())
    }

    #[test]
    fn render_html_reports_external_script_loader_limitation() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(160, 120).render_html(
            "<body><script src=\"app.js\"></script><p>Visible</p></body>",
            url,
        )?;

        assert_eq!(
            page.diagnostics,
            vec!["JavaScript skipped external script 1 app.js: external script loading requires a resource loader".to_string()]
        );
        assert!(page.display_list.commands.iter().any(|command| matches!(
            command,
            webby_render::DisplayCommand::DrawText { text, .. } if text.contains("Visible")
        )));
        Ok(())
    }

    #[test]
    fn external_scripts_execute_with_inline_defer_and_async_order() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/page").map_err(url_error)?;
        let first = url::Url::parse("https://example.test/one.js").map_err(url_error)?;
        let defer = url::Url::parse("https://example.test/defer.js").map_err(url_error)?;
        let async_url = url::Url::parse("https://example.test/async.js").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (first.as_str(), "var order = 'one';"),
            (defer.as_str(), "order = order + ' defer';"),
            (
                async_url.as_str(),
                "order = order + ' async'; document.getElementById('out').textContent = order;",
            ),
        ]);
        let html = "<body><p id=\"out\">start</p><script src=\"/one.js\"></script><script>order = order + ' inline';</script><script defer src=\"/defer.js\"></script><script async src=\"/async.js\"></script></body>";

        let page = PagePipeline::new(320, 180).render_html_with_loader(html, page_url, &loader)?;

        assert!(
            display_texts(&page)
                .join("")
                .contains("one inline defer async")
        );
        assert!(page.diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn external_script_src_resolves_relative_to_page_url() -> WebbyResult<()> {
        let page_url =
            url::Url::parse("https://example.test/pages/index.html").map_err(url_error)?;
        let script_url =
            url::Url::parse("https://example.test/assets/app.js").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            script_url.as_str(),
            "document.getElementById('out').textContent = 'relative script ran';",
        )]);
        let html = "<body><p id=\"out\">old</p><script src=\"../assets/app.js\"></script></body>";

        let page = PagePipeline::new(320, 180).render_html_with_loader(html, page_url, &loader)?;

        assert!(
            display_texts(&page)
                .join("")
                .contains("relative script ran")
        );
        Ok(())
    }

    #[test]
    fn failed_external_script_load_is_diagnostic_not_page_failure() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/page").map_err(url_error)?;
        let loader = TestSiteLoader::new([]);
        let html = "<body><script src=\"missing.js\"></script><p>Visible</p></body>";

        let page = PagePipeline::new(220, 140).render_html_with_loader(html, page_url, &loader)?;

        assert!(display_texts(&page).join("").contains("Visible"));
        assert!(
            page.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("failed to load script"))
        );
        Ok(())
    }

    #[test]
    fn unsupported_script_type_is_ignored_safely() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/page").map_err(url_error)?;
        let loader = TestSiteLoader::new([]);
        let html = "<body><p id=\"out\">Visible</p><script type=\"application/json\">bad()</script></body>";

        let page = PagePipeline::new(220, 140).render_html_with_loader(html, page_url, &loader)?;

        assert!(display_texts(&page).join("").contains("Visible"));
        assert_eq!(
            page.diagnostics,
            vec![
                "JavaScript skipped inline script 1: unsupported script type application/json"
                    .to_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn module_script_with_relative_import_runs_dependency_first() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/app/index.html").map_err(url_error)?;
        let dep = url::Url::parse("https://example.test/app/dep.js").map_err(url_error)?;
        let main = url::Url::parse("https://example.test/app/main.js").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (dep.as_str(), "var moduleOrder = 'dep';"),
            (
                main.as_str(),
                "import './dep.js';\ndocument.getElementById('out').textContent = moduleOrder + ' main';",
            ),
        ]);
        let html =
            "<body><p id=\"out\">old</p><script type=\"module\" src=\"main.js\"></script></body>";

        let page = PagePipeline::new(260, 160).render_html_with_loader(html, page_url, &loader)?;

        assert!(display_texts(&page).join("").contains("dep main"));
        assert!(page.diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn inline_module_static_import_runs() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/app/index.html").map_err(url_error)?;
        let dep = url::Url::parse("https://example.test/app/dep.js").map_err(url_error)?;
        let loader = TestSiteLoader::new([(dep.as_str(), "var inlineOrder = 'inline dep';")]);
        let html = "<body><p id=\"out\">old</p><script type=\"module\">import './dep.js';\ndocument.getElementById('out').textContent = inlineOrder + ' main';</script></body>";

        let page = PagePipeline::new(260, 160).render_html_with_loader(html, page_url, &loader)?;

        assert!(display_texts(&page).join("").contains("inline dep main"));
        Ok(())
    }

    #[test]
    fn module_cycle_becomes_diagnostic_without_looping() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/app/index.html").map_err(url_error)?;
        let first = url::Url::parse("https://example.test/app/first.js").map_err(url_error)?;
        let second = url::Url::parse("https://example.test/app/second.js").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                first.as_str(),
                "import './second.js';\ncycleOrder = cycleOrder + ' first'; document.getElementById('out').textContent = cycleOrder;",
            ),
            (
                second.as_str(),
                "import './first.js';\nvar cycleOrder = 'old second';",
            ),
        ]);
        let html =
            "<body><p id=\"out\">old</p><script type=\"module\" src=\"first.js\"></script></body>";

        let page = PagePipeline::new(260, 160).render_html_with_loader(html, page_url, &loader)?;

        assert!(
            page.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("module cycle detected"))
        );
        assert!(display_texts(&page).join("").contains("old second first"));
        Ok(())
    }

    #[test]
    fn unsupported_module_import_is_diagnostic_not_page_failure() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/app/index.html").map_err(url_error)?;
        let loader = TestSiteLoader::new([]);
        let html =
            "<body><p>Visible</p><script type=\"module\">import value from source;</script></body>";

        let page = PagePipeline::new(220, 140).render_html_with_loader(html, page_url, &loader)?;

        assert!(display_texts(&page).join("").contains("Visible"));
        assert!(
            page.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("unsupported import syntax"))
        );
        Ok(())
    }

    #[test]
    fn app_profile_can_disable_javascript() -> WebbyResult<()> {
        let profile = webby_state::BrowserProfile {
            config: webby_state::BrowserConfig {
                javascript_enabled: false,
                ..webby_state::BrowserConfig::default()
            },
            ..webby_state::BrowserProfile::default()
        };
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = StaticHtmlLoader::new(
            "<body><script>console.log('hidden')</script><p>Visible</p></body>",
            url,
        );
        let mut state = AppState::with_profile_and_window_size(&profile, 160, 120)?;

        state.chrome.set_address_input("https://example.test/");
        state.submit_address(&loader);

        let Some(page) = state.page.as_ref() else {
            return Err(WebbyError::invalid_input("expected rendered page"));
        };
        assert!(
            page.diagnostics
                .iter()
                .any(|item| item == "JavaScript disabled by configuration")
        );
        assert!(
            !page
                .diagnostics
                .iter()
                .any(|item| item.contains("JavaScript console: hidden"))
        );
        Ok(())
    }

    #[test]
    fn app_pipeline_renders_mixed_inline_fixture() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(220, 140).render_html(
            "<body><p>before <span>span</span> <a href=\"/next\">link<img width=\"10\" height=\"8\"></a><br>after</p></body>",
            url,
        )?;
        let ordered = page
            .display_list
            .commands
            .iter()
            .filter_map(|command| match command {
                webby_render::DisplayCommand::DrawText { text, .. } if !text.trim().is_empty() => {
                    Some(format!("text:{}", text.trim()))
                }
                webby_render::DisplayCommand::ImagePlaceholder { .. }
                | webby_render::DisplayCommand::DrawImage { .. } => Some("image".to_string()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ordered,
            vec![
                "text:before",
                "text:span",
                "text:link",
                "image",
                "text:after"
            ]
        );
        assert!(page.links.iter().all(|link| link.href == "/next"));
        Ok(())
    }

    #[test]
    fn page_pipeline_applies_css_from_html_string() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(160, 120).render_html(
            "<style>body { background-color: red; } p { color: blue; font-size: 24px; }</style><body><p>Hello</p></body>",
            url,
        )?;

        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::FillRect {
                    color: webby_render::Color {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                    ..
                }
            )
        }));
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    color: webby_render::Color {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255
                    },
                    font_size,
                    ..
                } if *font_size == 24.0
            )
        }));
        Ok(())
    }

    #[test]
    fn page_pipeline_applies_media_queries_from_html_string() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>
            p { font-size: 12px; }
            @media screen and (max-width: 200px) { p { font-size: 30px; } }
        </style><body><p>Responsive</p></body>";
        let narrow = PagePipeline::new(160, 120).render_html(html, url.clone())?;
        let wide = PagePipeline::new(320, 120).render_html(html, url)?;

        assert!(narrow.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    text,
                    font_size,
                    ..
                } if text == "Responsive" && *font_size == 30.0
            )
        }));
        assert!(wide.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    text,
                    font_size,
                    ..
                } if text == "Responsive" && *font_size == 12.0
            )
        }));
        Ok(())
    }

    #[test]
    fn app_resize_rerenders_loaded_page_for_media_queries() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>
            p { font-size: 12px; }
            @media screen and (max-width: 200px) { p { font-size: 30px; } }
        </style><body><p>Responsive</p></body>";
        let page = PagePipeline::new(320, 120).render_html(html, url)?;
        let mut state = AppState::with_window_size(320, 220);
        state.finish_navigation(Ok(page));

        state.resize(160, 220);

        let page = state.page.as_ref().ok_or_else(|| WebbyError::Render {
            message: "expected page after resize".to_string(),
        })?;
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    text,
                    font_size,
                    ..
                } if text == "Responsive" && *font_size == 30.0
            )
        }));
        Ok(())
    }

    #[test]
    fn app_resize_keeps_media_diagnostics_deterministic() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>
            @media print and (min-width: 1px) { p { color: red; } }
            p { font-size: 12px; }
        </style><body><p>Responsive</p></body>";
        let page = PagePipeline::new(320, 120).render_html(html, url)?;
        let mut state = AppState::with_window_size(320, 220);
        state.finish_navigation(Ok(page));

        state.resize(160, 220);
        state.resize(320, 220);

        let page = state.page.as_ref().ok_or_else(|| WebbyError::Render {
            message: "expected page after resize".to_string(),
        })?;
        let media_diagnostic_count = page
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.contains("unsupported media query"))
            .count();
        assert_eq!(media_diagnostic_count, 1);
        Ok(())
    }

    #[test]
    fn hovered_link_restyles_through_app_interaction_state() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>a { color: blue; } a:hover { color: red; }</style><body><a href=\"/next\">Hover</a></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url)?;
        let mut state = AppState::with_window_size(220, 220);
        state.finish_navigation(Ok(page));
        let (x, y) = first_link_point(&state)?;

        state.update_hover_at(x, y + CHROME_HEIGHT as f32);

        let page = state.page.as_ref().ok_or_else(|| WebbyError::Render {
            message: "expected hovered page".to_string(),
        })?;
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    text,
                    color: webby_render::Color {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                    ..
                } if text == "Hover"
            )
        }));
        Ok(())
    }

    #[test]
    fn focused_input_restyles_through_app_interaction_state() -> WebbyResult<()> {
        let html = "<style>input:focus { background-color: red; }</style><form><input type=\"text\" name=\"q\"></form>";
        let mut state = loaded_form_state(html)?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));

        let page = state.page.as_ref().ok_or_else(|| WebbyError::Render {
            message: "expected focused page".to_string(),
        })?;
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::FillRect {
                    color: webby_render::Color {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                    ..
                }
            )
        }));
        Ok(())
    }

    #[test]
    fn hover_color_transition_uses_deterministic_animation_clock() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>
            a { color: blue; transition: color 100ms linear; }
            a:hover { color: red; }
        </style><body><a href=\"/next\">Hover</a></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url)?;
        let mut state = AppState::with_window_size(220, 220);
        state.finish_navigation(Ok(page));
        let (x, y) = first_link_point(&state)?;

        state.update_hover_at(x, y + CHROME_HEIGHT as f32);
        let transition = state
            .active_transition
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("expected active transition"))?;
        assert_eq!(transition.duration_ms, 100);
        assert_eq!(animated_text_color(transition, "Hover")?, [0, 0, 255, 255]);

        assert!(state.tick_animations(50));
        let transition = state
            .active_transition
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("expected active transition at midpoint"))?;
        assert_eq!(
            animated_text_color(transition, "Hover")?,
            [128, 0, 128, 255]
        );

        assert!(state.tick_animations(50));
        assert!(state.active_transition.is_none());
        Ok(())
    }

    #[test]
    fn hover_left_transition_interpolates_display_geometry() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>
            a { display: block; position: relative; left: 0px; transition: left 100ms linear; }
            a:hover { left: 20px; }
        </style><body><a href=\"/next\">Move</a></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url)?;
        let mut state = AppState::with_window_size(220, 220);
        state.finish_navigation(Ok(page));
        let (x, y) = first_link_point(&state)?;

        state.update_hover_at(x, y + CHROME_HEIGHT as f32);
        let transition = state
            .active_transition
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("expected active transition"))?;
        let start = animated_text_rect(transition, "Move")?;
        assert!(state.tick_animations(50));
        let transition = state
            .active_transition
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("expected active transition at midpoint"))?;
        let midpoint = animated_text_rect(transition, "Move")?;

        assert!((midpoint.x - start.x - 10.0).abs() < 0.5);
        Ok(())
    }

    #[test]
    fn animations_can_be_disabled_for_deterministic_tests() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>
            a { color: blue; transition: color 100ms linear; }
            a:hover { color: red; }
        </style><body><a href=\"/next\">Hover</a></body>";
        let page = PagePipeline::new(220, 140).render_html(html, url)?;
        let mut state = AppState::with_window_size(220, 220);
        state.animations_enabled = false;
        state.finish_navigation(Ok(page));
        let (x, y) = first_link_point(&state)?;

        state.update_hover_at(x, y + CHROME_HEIGHT as f32);

        assert!(state.active_transition.is_none());
        let page = state.page.as_ref().ok_or_else(|| WebbyError::Render {
            message: "expected page after hover".to_string(),
        })?;
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    text,
                    color: webby_render::Color { r: 255, g: 0, b: 0, a: 255 },
                    ..
                } if text == "Hover"
            )
        }));
        Ok(())
    }

    #[test]
    fn page_pipeline_applies_milestone_13_selector_behavior() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(160, 120).render_html(
            "<style>.card > p.highlighted[name=\"q\"] { color: blue; font-size: 24px; }</style><body><div class=\"card\"><p class=\"highlighted\" name=\"q\">Selected</p></div></body>",
            url,
        )?;

        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    text,
                    color: webby_render::Color {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255
                    },
                    font_size,
                    ..
                } if text == "Selected" && *font_size == 24.0
            )
        }));
        Ok(())
    }

    #[test]
    fn relative_stylesheet_urls_resolve_for_file_and_http_pages() -> WebbyResult<()> {
        let file = url::Url::parse("file:///tmp/webby/pages/index.html").map_err(url_error)?;
        let http = url::Url::parse("https://example.test/docs/index.html").map_err(url_error)?;

        assert_eq!(
            webby_stylesheet::resolve_stylesheet_url(&file, "../assets/site.css")?.as_str(),
            "file:///tmp/webby/assets/site.css"
        );
        assert_eq!(
            webby_stylesheet::resolve_stylesheet_url(&http, "../assets/site.css")?.as_str(),
            "https://example.test/assets/site.css"
        );
        Ok(())
    }

    #[test]
    fn page_pipeline_applies_external_stylesheets_and_records_css_diagnostics() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let css_url = url::Url::parse("https://example.test/site.css").map_err(url_error)?;
        let loader = ByteResourceLoader::new([
            (
                page_url.as_str(),
                "text/html; charset=utf-8",
                b"<link rel=\"stylesheet\" href=\"site.css\"><body><p>External</p></body>".to_vec(),
            ),
            (
                css_url.as_str(),
                "text/css",
                b"body { background-color: red; } p { color: blue; } broken".to_vec(),
            ),
        ]);

        let page = PagePipeline::new(160, 120).load_url(&loader, &page_url)?;

        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::FillRect {
                    color: webby_render::Color {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                    ..
                }
            )
        }));
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    color: webby_render::Color {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255
                    },
                    ..
                }
            )
        }));
        assert_eq!(page.diagnostics.len(), 1);
        assert!(
            page.diagnostics[0]
                .starts_with("stylesheet https://example.test/site.css CSS diagnostic at byte ")
        );
        assert!(page.diagnostics[0].contains("skipped CSS without declaration block"));
        Ok(())
    }

    #[test]
    fn failed_external_stylesheet_load_does_not_fail_page() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let loader = ByteResourceLoader::new([(
            page_url.as_str(),
            "text/html; charset=utf-8",
            b"<link rel=\"stylesheet\" href=\"missing.css\"><body>Visible</body>".to_vec(),
        )]);

        let page = PagePipeline::new(160, 120).load_url(&loader, &page_url)?;

        assert_eq!(
            page.diagnostics,
            vec![
                "stylesheet https://example.test/missing.css could not be loaded: network error: no test resource for https://example.test/missing.css".to_string()
            ]
        );
        assert!(!page.display_list.commands.is_empty());
        Ok(())
    }

    #[test]
    fn render_html_does_not_fetch_linked_external_stylesheets() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let page = PagePipeline::new(160, 120).render_html(
            "<link rel=\"stylesheet\" href=\"site.css\"><body><p>Direct</p></body>",
            url,
        )?;

        assert!(page.diagnostics.is_empty());
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    color: webby_render::Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                    ..
                }
            )
        }));
        Ok(())
    }

    #[test]
    fn pipeline_coordinator_renders_html_string_to_buffer() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(80, 60).render_html("<body><p>Buffer</p></body>", url)?;

        assert_eq!(
            page.surface.pixels.len(),
            page.surface.width * page.surface.height * 4
        );
        Ok(())
    }

    #[test]
    fn page_pipeline_loads_and_renders_local_image_fixture() -> WebbyResult<()> {
        let base = url::Url::parse("file:///tmp/webby/index.html").map_err(url_error)?;
        let image_url = url::Url::parse("file:///tmp/webby/pixel.png").map_err(url_error)?;
        let loader = ByteResourceLoader::new([
            (
                base.as_str(),
                "text/html; charset=utf-8",
                b"<body><img src=\"pixel.png\"></body>".to_vec(),
            ),
            (
                image_url.as_str(),
                "image/png",
                encoded_test_image(ImageFormat::Png)?,
            ),
        ]);

        let page = PagePipeline::new(64, 64).load_url(&loader, &base)?;

        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawImage {
                    image_width: 2,
                    image_height: 1,
                    ..
                }
            )
        }));
        assert!(
            page.surface
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel == [255, 0, 0, 255])
        );
        Ok(())
    }

    #[test]
    fn page_pipeline_renders_data_url_image_pixels() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/data-image").map_err(url_error)?;
        let image =
            base64::engine::general_purpose::STANDARD.encode(encoded_test_image(ImageFormat::Png)?);
        let html = format!("<body><img src=\"data:image/png;base64,{image}\"></body>");

        let page =
            PagePipeline::new(64, 64).render_html_with_loader(&html, base, &FailingLoader)?;

        assert!(surface_has_color(&page.surface, [255, 0, 0, 255]));
        Ok(())
    }

    #[test]
    fn invalid_data_url_image_falls_back_safely() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/bad-data-image").map_err(url_error)?;
        let page = PagePipeline::new(64, 64).render_html_with_loader(
            "<body><img src=\"data:image/png;base64,not-valid!\"></body>",
            base,
            &FailingLoader,
        )?;

        assert!(
            page.display_list.commands.iter().all(|command| {
                !matches!(command, webby_render::DisplayCommand::DrawImage { .. })
            })
        );
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::ImagePlaceholder { .. }
            )
        }));
        Ok(())
    }

    #[test]
    fn video_poster_data_url_renders_image_pixels() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/video").map_err(url_error)?;
        let image =
            base64::engine::general_purpose::STANDARD.encode(encoded_test_image(ImageFormat::Png)?);
        let html = format!("<body><video poster=\"data:image/png;base64,{image}\"></video></body>");

        let page =
            PagePipeline::new(64, 64).render_html_with_loader(&html, base, &FailingLoader)?;

        assert!(surface_has_color(&page.surface, [255, 0, 0, 255]));
        Ok(())
    }

    #[test]
    fn resource_hints_are_reported_as_noop_diagnostics() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/hints").map_err(url_error)?;
        let page = PagePipeline::new(64, 64).render_html(
            "<link rel=\"preload\" href=\"hero.png\"><link rel=\"preconnect\" href=\"https://cdn.example\"><body>Hints</body>",
            base,
        )?;

        assert_eq!(
            page.diagnostics,
            vec![
                "resource hint rel=preload href=hero.png ignored".to_string(),
                "resource hint rel=preconnect href=https://cdn.example ignored".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn iframe_loads_renders_and_keeps_parent_url() -> WebbyResult<()> {
        let parent = url::Url::parse("https://example.test/parent").map_err(url_error)?;
        let child = url::Url::parse("https://example.test/child").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                parent.as_str(),
                "<body><p>Parent</p><iframe src=\"/child\" width=\"180\" height=\"90\" name=\"child\"></iframe></body>",
            ),
            (child.as_str(), "<body><p>Child frame</p></body>"),
        ]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, parent.clone());

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert_eq!(state.navigation.current_url.as_ref(), Some(&parent));
        assert_eq!(page.iframes.len(), 1);
        assert_eq!(page.iframes[0].url, child);
        assert!(
            page.display_list.commands.iter().any(|command| {
                matches!(command, webby_render::DisplayCommand::DrawImage { .. })
            })
        );
        Ok(())
    }

    #[test]
    fn iframe_link_navigation_updates_nested_context_only() -> WebbyResult<()> {
        let parent = url::Url::parse("https://example.test/parent").map_err(url_error)?;
        let child = url::Url::parse("https://example.test/child").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (
                parent.as_str(),
                "<body><p>Parent</p><iframe src=\"/child\" width=\"180\" height=\"90\"></iframe></body>",
            ),
            (
                child.as_str(),
                "<body><a href=\"/next\">Next frame</a></body>",
            ),
            (next.as_str(), "<body><p>Nested next</p></body>"),
        ]);
        let mut state = AppState::with_window_size(260, 180);
        state.navigate_to_url(&loader, parent.clone());
        let (x, y) = iframe_link_window_point(&state)?;

        assert!(state.click_at(x, y, &loader));

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert_eq!(state.navigation.current_url.as_ref(), Some(&parent));
        assert_eq!(page.iframes.first().map(|iframe| &iframe.url), Some(&next));
        assert!(
            display_texts(&page.iframes[0].page)
                .join("")
                .contains("Nested next")
        );
        Ok(())
    }

    #[test]
    fn failed_iframe_load_reports_diagnostic_and_uses_placeholder() -> WebbyResult<()> {
        let parent = url::Url::parse("https://example.test/parent").map_err(url_error)?;
        let loader = TestSiteLoader::new([(
            parent.as_str(),
            "<body><iframe src=\"/missing\" width=\"120\" height=\"70\" sandbox></iframe></body>",
        )]);
        let mut state = AppState::with_window_size(260, 180);

        state.navigate_to_url(&loader, parent);

        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("missing page"))?;
        assert!(page.iframes.is_empty());
        assert!(page.diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("iframe https://example.test/missing failed to load")
        }));
        assert!(
            page.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("iframe sandbox diagnostic"))
        );
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::ImagePlaceholder { .. }
            )
        }));
        Ok(())
    }

    #[test]
    fn page_pipeline_uses_cache_for_repeated_page_loads() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let loader = CountingByteResourceLoader::new([(
            page_url.as_str(),
            "text/html; charset=utf-8",
            b"<body>Cached</body>".to_vec(),
        )]);
        let cache = webby_cache::ResourceCache::new();
        let pipeline = PagePipeline::new(120, 80);

        let first = pipeline.load_url_with_cache(
            &loader,
            &cache,
            &page_url,
            webby_cache::CacheMode::Use,
        )?;
        let second = pipeline.load_url_with_cache(
            &loader,
            &cache,
            &page_url,
            webby_cache::CacheMode::Use,
        )?;

        assert_eq!(loader.calls.get(), 1);
        assert!(
            first
                .diagnostics
                .iter()
                .any(|item| item.contains("cache miss"))
        );
        assert!(
            second
                .diagnostics
                .iter()
                .any(|item| item == "cache hit https://example.test/index.html")
        );
        Ok(())
    }

    #[test]
    fn app_pipeline_reuses_disk_cache_across_state_restart() -> WebbyResult<()> {
        let root = std::env::temp_dir().join(format!(
            "webby-app-disk-cache-restart-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let page_url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let first_loader = CountingByteResourceLoader::new([(
            page_url.as_str(),
            "text/html; charset=utf-8",
            b"<body>Persisted page</body>".to_vec(),
        )]);
        let mut first = AppState::with_window_size(160, 100);
        first.enable_disk_cache(&root)?;
        first.chrome.set_address_input(page_url.to_string());
        first.submit_address(&first_loader);
        assert!(matches!(first.status, PageStatus::Loaded { .. }));

        let second_loader = CountingByteResourceLoader::new([]);
        let mut second = AppState::with_window_size(160, 100);
        second.enable_disk_cache(&root)?;
        second.chrome.set_address_input(page_url.to_string());
        second.submit_address(&second_loader);

        assert!(matches!(second.status, PageStatus::Loaded { .. }));
        assert_eq!(second_loader.calls.get(), 0);
        assert!(second.page.as_ref().is_some_and(|page| {
            page.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("disk-hit"))
        }));
        Ok(())
    }

    #[test]
    fn app_navigation_stores_and_sends_cookies() -> WebbyResult<()> {
        let login = url::Url::parse("https://example.test/login").map_err(url_error)?;
        let account = url::Url::parse("https://example.test/account").map_err(url_error)?;
        let loader = CookieSessionLoader::new(login.clone(), account.clone());
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, login);
        state.navigate_to_url(&loader, account.clone());

        assert_eq!(state.navigation.current_url.as_ref(), Some(&account));
        assert_eq!(state.cookie_jar.cookie_header_value(&account), "sid=abc");
        assert!(loader.account_cookie_seen.get());
        Ok(())
    }

    #[test]
    fn cookies_disabled_do_not_store_or_send_cookie_headers() -> WebbyResult<()> {
        let login = url::Url::parse("https://example.test/login").map_err(url_error)?;
        let account = url::Url::parse("https://example.test/account").map_err(url_error)?;
        let loader = CookieSessionLoader::new(login.clone(), account.clone());
        let mut state = AppState::with_window_size(240, 160);
        state.cookies_enabled = false;

        state.navigate_to_url(&loader, login);
        state.navigate_to_url(&loader, account);

        assert!(state.cookie_jar.cookies.is_empty());
        assert!(!loader.account_cookie_seen.get());
        Ok(())
    }

    #[test]
    fn persistent_cookies_are_saved_with_successful_navigation() -> WebbyResult<()> {
        let store = temp_profile_store("app-persistent-cookies");
        let mut profile = webby_state::BrowserProfile {
            config: webby_state::BrowserConfig {
                persist_cookies: true,
                ..webby_state::BrowserConfig::default()
            },
            ..webby_state::BrowserProfile::default()
        };
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let mut state = AppState::with_profile_and_window_size(&profile, 240, 160)?;
        state.cookie_jar.store_from_headers(
            &url,
            &[(
                "set-cookie".to_string(),
                "persist=yes; Path=/; Max-Age=60".to_string(),
            )],
        );
        state.finish_navigation(PagePipeline::new(240, 112).render_html("<body>ok</body>", url));

        state.record_successful_navigation(&store, &mut profile)?;

        assert_eq!(
            store
                .load()?
                .cookies
                .cookie_header_value(&url::Url::parse("https://example.test/").map_err(url_error)?),
            "persist=yes"
        );
        Ok(())
    }

    #[test]
    fn external_stylesheet_loads_use_cache() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let css_url = url::Url::parse("https://example.test/site.css").map_err(url_error)?;
        let loader = CountingByteResourceLoader::new([
            (
                page_url.as_str(),
                "text/html; charset=utf-8",
                b"<link rel=\"stylesheet\" href=\"site.css\"><body><p>Cached CSS</p></body>"
                    .to_vec(),
            ),
            (css_url.as_str(), "text/css", b"p { color: blue; }".to_vec()),
        ]);
        let cache = webby_cache::ResourceCache::new();
        let pipeline = PagePipeline::new(120, 80);

        pipeline.load_url_with_cache(&loader, &cache, &page_url, webby_cache::CacheMode::Use)?;
        let second = pipeline.load_url_with_cache(
            &loader,
            &cache,
            &page_url,
            webby_cache::CacheMode::Use,
        )?;

        assert_eq!(loader.calls.get(), 2);
        assert!(cache.contains_url(&page_url));
        assert!(cache.contains_url(&css_url));
        assert!(
            second
                .diagnostics
                .iter()
                .any(|item| item == "cache hit https://example.test/site.css")
        );
        Ok(())
    }

    #[test]
    fn image_loads_use_cache() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let image_url = url::Url::parse("https://example.test/pixel.png").map_err(url_error)?;
        let loader = CountingByteResourceLoader::new([
            (
                page_url.as_str(),
                "text/html; charset=utf-8",
                b"<body><img src=\"pixel.png\"></body>".to_vec(),
            ),
            (
                image_url.as_str(),
                "image/png",
                encoded_test_image(ImageFormat::Png)?,
            ),
        ]);
        let cache = webby_cache::ResourceCache::new();
        let pipeline = PagePipeline::new(120, 80);

        pipeline.load_url_with_cache(&loader, &cache, &page_url, webby_cache::CacheMode::Use)?;
        let second = pipeline.load_url_with_cache(
            &loader,
            &cache,
            &page_url,
            webby_cache::CacheMode::Use,
        )?;

        assert_eq!(loader.calls.get(), 2);
        assert!(cache.contains_url(&page_url));
        assert!(cache.contains_url(&image_url));
        assert!(
            second
                .diagnostics
                .iter()
                .any(|item| item == "cache hit https://example.test/pixel.png")
        );
        Ok(())
    }

    #[test]
    fn manual_reload_refreshes_cache_entry() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let loader = SequenceHtmlLoader::default();
        let mut state = AppState::with_window_size(160, 120);

        state.navigate_to_url(&loader, page_url.clone());
        assert!(state
            .page
            .as_ref()
            .is_some_and(|page| page.display_list.commands.iter().any(|command| {
                matches!(command, webby_render::DisplayCommand::DrawText { text, .. } if text == "v1")
            })));

        assert!(state.reload(&loader));
        assert_eq!(loader.calls.get(), 2);
        assert!(state
            .page
            .as_ref()
            .is_some_and(|page| page.display_list.commands.iter().any(|command| {
                matches!(command, webby_render::DisplayCommand::DrawText { text, .. } if text == "v2")
            })));

        state.navigate_to_url(&loader, page_url);
        assert_eq!(loader.calls.get(), 2);
        Ok(())
    }

    #[test]
    fn app_state_uses_cached_loading_for_normal_navigation() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let loader = CountingByteResourceLoader::new([(
            page_url.as_str(),
            "text/html; charset=utf-8",
            b"<body>Cached app</body>".to_vec(),
        )]);
        let mut state = AppState::with_window_size(160, 120);

        state.navigate_to_url(&loader, page_url.clone());
        state.navigate_to_url(&loader, page_url);

        assert_eq!(loader.calls.get(), 1);
        assert_eq!(state.resource_cache.len(), 1);
        Ok(())
    }

    #[test]
    fn loader_errors_are_represented_as_app_state_not_panics() {
        let loader = FailingLoader;
        let mut state = AppState::new();
        state.chrome.set_address_input("example.test");

        state.submit_address(&loader);

        assert!(matches!(
            state.status,
            PageStatus::Error { ref message } if message.contains("network")
        ));
    }

    #[test]
    fn failed_navigation_after_success_preserves_last_successful_current_url() -> WebbyResult<()> {
        let success_url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = StaticHtmlLoader::new("<body>ok</body>", success_url.clone());
        let mut state = AppState::with_window_size(160, 120);
        state.chrome.set_address_input("example.test");
        state.submit_address(&loader);

        let failed_url = begin_url(&mut state, "missing.test")?;
        state.finish_navigation(Err(WebbyError::Network {
            message: "offline".to_string(),
        }));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&success_url));
        assert_eq!(
            state.navigation.failed_url.as_deref(),
            Some(failed_url.as_str())
        );
        assert!(state.page.is_some());
        assert!(matches!(state.status, PageStatus::Error { .. }));
        Ok(())
    }

    #[test]
    fn failed_url_records_failed_target() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(160, 120);
        let failed_url = begin_url(&mut state, "broken.test")?;

        state.finish_navigation(Err(WebbyError::Network {
            message: "network failed".to_string(),
        }));

        assert_eq!(
            state.navigation.failed_url.as_deref(),
            Some(failed_url.as_str())
        );
        Ok(())
    }

    #[test]
    fn finish_navigation_clears_pending_url_on_success() -> WebbyResult<()> {
        let page_url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let mut state = AppState::with_window_size(160, 120);
        let pending_url = begin_url(&mut state, "example.test")?;
        let page = PagePipeline::new(160, 72).render_html("<body>ok</body>", page_url)?;

        assert_eq!(state.navigation.pending_url.as_ref(), Some(&pending_url));
        state.finish_navigation(Ok(page));

        assert!(state.navigation.pending_url.is_none());
        assert!(state.navigation.failed_url.is_none());
        Ok(())
    }

    #[test]
    fn finish_navigation_clears_pending_url_on_failure() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(160, 120);
        let pending_url = begin_url(&mut state, "failure.test")?;

        state.finish_navigation(Err(WebbyError::Render {
            message: "render failed".to_string(),
        }));

        assert!(state.navigation.pending_url.is_none());
        assert_eq!(state.navigation.lifecycle, NavigationLifecycle::Failed);
        assert_eq!(
            state.navigation.failed_url.as_deref(),
            Some(pending_url.as_str())
        );
        Ok(())
    }

    #[test]
    fn parser_layout_render_errors_flow_into_page_error_state() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(160, 120);
        let failed_url = begin_url(&mut state, "parse-error.test")?;

        state.finish_navigation(Err(WebbyError::Parse {
            message: "synthetic parser failure".to_string(),
        }));

        assert!(matches!(
            state.status,
            PageStatus::Error { ref message } if message.contains("parse error")
        ));
        assert_eq!(
            state.navigation.failed_url.as_deref(),
            Some(failed_url.as_str())
        );
        Ok(())
    }

    #[test]
    fn startup_page_helper_resolves_documented_startup_path() -> WebbyResult<()> {
        let url = startup_file_url()?;

        assert_eq!(url.scheme(), "file");
        assert!(url.to_string().contains("examples/simple.html"));
        Ok(())
    }

    #[test]
    fn homepage_config_is_loaded_and_used_by_app_startup() -> WebbyResult<()> {
        let profile = webby_state::BrowserProfile {
            config: webby_state::BrowserConfig {
                homepage: "https://example.test/start".to_string(),
                ..webby_state::BrowserConfig::default()
            },
            ..webby_state::BrowserProfile::default()
        };

        let state = AppState::with_profile_and_window_size(&profile, 160, 120)?;

        assert_eq!(state.chrome.address_input, "https://example.test/start");
        assert_eq!(
            AppState::homepage_from_profile(&profile)?,
            "https://example.test/start"
        );
        Ok(())
    }

    #[test]
    fn invalid_homepage_url_is_handled_safely() -> WebbyResult<()> {
        let profile = webby_state::BrowserProfile {
            config: webby_state::BrowserConfig {
                homepage: "http://[::1".to_string(),
                ..webby_state::BrowserConfig::default()
            },
            ..webby_state::BrowserProfile::default()
        };
        let mut state = AppState::with_profile_and_window_size(&profile, 160, 120)?;

        state.submit_address(&FailingLoader);

        assert!(matches!(state.status, PageStatus::Error { .. }));
        assert!(state.navigation.current_url.is_none());
        Ok(())
    }

    #[test]
    fn app_records_successful_navigation_in_persistent_history() -> WebbyResult<()> {
        let store = temp_profile_store("app-history");
        let mut profile = webby_state::BrowserProfile::default();
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(160, 80).render_html("<body>ok</body>", url.clone())?;
        let mut state = AppState::with_window_size(160, 120);

        state.finish_navigation(Ok(page));
        assert!(state.record_successful_navigation(&store, &mut profile)?);

        assert_eq!(store.load()?.history.entries, vec![url.to_string()]);
        Ok(())
    }

    #[test]
    fn successful_navigation_from_any_tab_records_persistent_history_once() -> WebbyResult<()> {
        let store = temp_profile_store("app-tab-history");
        let mut profile = webby_state::BrowserProfile::default();
        let first = url::Url::parse("https://example.test/first").map_err(url_error)?;
        let second = url::Url::parse("https://example.test/second").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (first.as_str(), "<body>First</body>"),
            (second.as_str(), "<body>Second</body>"),
        ]);
        let mut state = AppState::with_window_size(160, 120);

        state.navigate_to_url(&loader, first.clone());
        assert!(state.record_successful_navigation(&store, &mut profile)?);
        state.new_tab();
        state.navigate_to_url(&loader, second.clone());
        assert!(state.record_successful_navigation(&store, &mut profile)?);

        assert_eq!(
            store.load()?.history.entries,
            vec![first.to_string(), second.to_string()]
        );
        Ok(())
    }

    #[test]
    fn failed_navigation_is_not_persisted_as_successful_history() -> WebbyResult<()> {
        let store = temp_profile_store("app-failed-history");
        let mut profile = webby_state::BrowserProfile::default();
        let mut state = AppState::with_window_size(160, 120);
        let _ = begin_url(&mut state, "missing.test")?;

        state.finish_navigation(Err(WebbyError::Network {
            message: "offline".to_string(),
        }));
        assert!(!state.record_successful_navigation(&store, &mut profile)?);

        assert!(store.load()?.history.entries.is_empty());
        Ok(())
    }

    #[test]
    fn app_bookmark_apis_add_list_and_remove() -> WebbyResult<()> {
        let store = temp_profile_store("app-bookmarks");
        let mut profile = webby_state::BrowserProfile::default();
        let state = AppState::with_window_size(160, 120);
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;

        state.add_bookmark(&store, &mut profile, &url)?;
        state.add_bookmark(&store, &mut profile, &url)?;

        assert_eq!(state.list_bookmarks(&profile).len(), 1);
        assert!(state.remove_bookmark(&store, &mut profile, &url)?);
        assert!(state.list_bookmarks(&profile).is_empty());
        Ok(())
    }

    #[test]
    fn bookmark_operations_remain_profile_level_across_tabs() -> WebbyResult<()> {
        let store = temp_profile_store("app-tab-bookmarks");
        let mut profile = webby_state::BrowserProfile::default();
        let mut state = AppState::with_window_size(160, 120);
        let first = url::Url::parse("https://example.test/first").map_err(url_error)?;
        let second = url::Url::parse("https://example.test/second").map_err(url_error)?;

        state.add_bookmark(&store, &mut profile, &first)?;
        state.new_tab();
        state.add_bookmark(&store, &mut profile, &second)?;

        let bookmarks = state
            .list_bookmarks(&profile)
            .iter()
            .map(|bookmark| bookmark.url.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            bookmarks,
            vec!["https://example.test/first", "https://example.test/second"]
        );
        Ok(())
    }

    #[test]
    fn app_can_open_persisted_bookmark() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(160, 120);
        let bookmark = webby_state::Bookmark {
            url: "https://example.test/bookmarked".to_string(),
        };
        let target = url::Url::parse(&bookmark.url).map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Bookmarked</body>")]);

        assert!(state.open_bookmark(&loader, &bookmark));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn app_can_open_generated_bookmarks_page() -> WebbyResult<()> {
        let store = temp_profile_store("app-generated-bookmarks");
        let mut profile = store.load()?;
        let url = url::Url::parse("https://example.test/saved").map_err(url_error)?;
        let mut state = AppState::with_window_size(240, 160);
        state.add_bookmark(&store, &mut profile, &url)?;

        state.open_bookmarks_page(&profile)?;

        assert_eq!(
            state.navigation.current_url.as_ref().map(url::Url::as_str),
            Some(BOOKMARKS_PAGE_URL)
        );
        let text = state
            .page
            .as_ref()
            .map(|page| {
                page.display_list
                    .commands
                    .iter()
                    .filter_map(|command| match command {
                        webby_render::DisplayCommand::DrawText { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(text.contains(&"Bookmarks"));
        assert!(text.contains(&"https://example.test/saved"));
        Ok(())
    }

    #[test]
    fn link_metadata_is_available_through_page_pipeline_for_future_hit_testing() -> WebbyResult<()>
    {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(240, 160)
            .render_html("<body><p><a href=\"/next\">Next page</a></p></body>", url)?;

        assert!(page.links.iter().any(|link| link.href == "/next"));
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(
                command,
                webby_render::DisplayCommand::DrawText {
                    href: Some(href),
                    ..
                } if href == "/next"
            )
        }));
        Ok(())
    }

    #[test]
    fn tab_and_shift_tab_move_page_focus_deterministically() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<p><a href=\"/first\">First</a></p><form><input type=\"text\" name=\"q\"><input type=\"submit\" value=\"Go\"></form>",
        )?;

        assert!(state.focus_next_page_item());
        assert_eq!(
            state.keyboard_focus,
            Some(KeyboardFocusTarget::Link { index: 0 })
        );
        assert!(state.focus_next_page_item());
        assert_eq!(
            state.keyboard_focus,
            Some(KeyboardFocusTarget::FormControl { id: 0 })
        );
        assert!(state.focus_previous_page_item());
        assert_eq!(
            state.keyboard_focus,
            Some(KeyboardFocusTarget::Link { index: 0 })
        );
        Ok(())
    }

    #[test]
    fn enter_on_focused_link_navigates() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/").map_err(url_error)?;
        let mut state = AppState::with_window_size(240, 160);
        state.finish_navigation(
            PagePipeline::new(240, 112)
                .render_html("<body><a href=\"/next\">Next</a></body>", base),
        );
        assert!(state.focus_next_page_item());
        let target = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = StaticHtmlLoader::new("<body>Arrived</body>", target.clone());

        assert!(state.activate_keyboard_focus(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn keyboard_focus_keeps_text_inputs_editable_and_enter_submits() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/find\"><label for=\"q\">Query</label><input id=\"q\" type=\"search\" name=\"q\"><input type=\"submit\" value=\"Go\"></form>",
        )?;
        assert!(state.focus_next_page_item());
        state.type_character('r');
        state.type_character('s');
        state.backspace();
        let target = url::Url::parse("https://example.test/find?q=r").map_err(url_error)?;
        let loader = StaticHtmlLoader::new("<body>Results</body>", target.clone());

        assert!(state.activate_keyboard_focus(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn space_toggles_focused_checkbox_without_navigation() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form><label><input type=\"checkbox\" name=\"ok\" value=\"yes\">Agree</label></form>",
        )?;
        assert!(state.focus_next_page_item());

        assert!(state.press_space_on_keyboard_focus(&FailingLoader));

        assert_eq!(state.form_values.get(&0).map(String::as_str), Some("true"));
        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        Ok(())
    }

    #[test]
    fn space_on_focused_submit_button_submits_form() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/send\"><input type=\"text\" name=\"q\" value=\"webby\"><button type=\"submit\">Send</button></form>",
        )?;
        state.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: 1 });
        state.focused_form_control = Some(1);
        let target = url::Url::parse("https://example.test/send?q=webby").map_err(url_error)?;
        let loader = StaticHtmlLoader::new("<body>Sent</body>", target.clone());

        assert!(state.press_space_on_keyboard_focus(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn accessible_nodes_expose_names_roles_and_state() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<p><a href=\"/about\">About us</a></p><p><a href=\"/logo\"><img src=\"missing.png\" alt=\"Logo mark\"></a></p><form><label for=\"q\">Search query</label><input id=\"q\" type=\"search\" name=\"q\"><button type=\"submit\" aria-label=\"Run search\">Go</button></form>",
        )?;
        state.keyboard_focus = Some(KeyboardFocusTarget::FormControl { id: 0 });
        state.focused_form_control = Some(0);

        let nodes = state.accessible_nodes();

        assert!(nodes.iter().any(|node| {
            node.role == AccessibleRole::Link
                && node.name == "About us"
                && node.href.as_deref() == Some("/about")
        }));
        assert!(nodes.iter().any(|node| {
            node.role == AccessibleRole::Link
                && node.name == "Logo mark"
                && node.href.as_deref() == Some("/logo")
        }));
        assert!(nodes.iter().any(|node| {
            node.role == AccessibleRole::Textbox
                && node.name == "Search query"
                && node.state.focused
        }));
        assert!(
            nodes
                .iter()
                .any(|node| { node.role == AccessibleRole::Button && node.name == "Run search" })
        );
        Ok(())
    }

    #[test]
    fn keyboard_focus_ring_changes_composed_frame_pixels() -> WebbyResult<()> {
        let mut state = loaded_form_state("<p><a href=\"/next\">Next</a></p>")?;
        let before = state.compose_frame()?;

        assert!(state.focus_next_page_item());
        let after = state.compose_frame()?;

        assert_ne!(before.pixels, after.pixels);
        Ok(())
    }

    #[test]
    fn keyboard_scroll_clamps_like_mouse_scroll() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<div style=\"height: 1200px\">Tall</div><p><a href=\"/bottom\">Bottom</a></p>",
        )?;

        state.keyboard_scroll_by(10_000.0);
        let scrolled = state.scroll_y;
        state.keyboard_scroll_by(-10_000.0);

        assert!(scrolled > 0.0);
        assert_eq!(state.scroll_y, 0.0);
        Ok(())
    }

    #[test]
    fn app_pipeline_renders_local_form_fixture() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/form").map_err(url_error)?;
        let page = PagePipeline::new(260, 160).render_html(
            "<body><form action=\"/find\"><label for=\"q\">Query</label><input type=\"search\" name=\"q\" placeholder=\"Search\"><input type=\"submit\" value=\"Go\"></form></body>",
            url,
        )?;

        assert_eq!(page.form_controls.len(), 2);
        assert!(page.display_list.commands.iter().any(|command| {
            matches!(command, webby_render::DisplayCommand::DrawText { text, .. } if text == "Query")
        }));
        assert!(
            page.surface
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel != [255, 255, 255, 255])
        );
        Ok(())
    }

    #[test]
    fn clicking_text_input_focuses_form_control() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"text\" name=\"q\" value=\"ru\"></form>",
        )?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));

        assert!(!state.chrome.address_focused);
        assert!(state.focused_form_control.is_some());
        Ok(())
    }

    #[test]
    fn typing_and_backspace_update_focused_input_value() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"search\" name=\"q\" value=\"ru\"></form>",
        )?;
        let (x, y) = form_control_point(&state, FormControlType::Search)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader);
        state.type_character('s');
        state.type_character('t');
        state.backspace();

        let control_id = state
            .focused_form_control
            .ok_or_else(|| WebbyError::invalid_input("test input was not focused"))?;
        assert_eq!(
            state.form_values.get(&control_id).map(String::as_str),
            Some("rus")
        );
        Ok(())
    }

    #[test]
    fn edited_input_value_is_visible_in_composed_frame() -> WebbyResult<()> {
        let mut state =
            loaded_form_state("<form><input type=\"text\" name=\"q\" value=\"\"></form>")?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader);
        let before = state.compose_frame()?;
        state.type_character('x');
        let after = state.compose_frame()?;

        assert_ne!(before.pixels, after.pixels);
        Ok(())
    }

    #[test]
    fn enter_in_text_input_submits_get_form() -> WebbyResult<()> {
        let target = url::Url::parse("https://example.test/find?q=webby").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Results</body>")]);
        let mut state =
            loaded_form_state("<form action=\"/find\"><input type=\"text\" name=\"q\"></form>")?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &loader);
        for character in "webby".chars() {
            state.type_character(character);
        }
        assert!(state.submit_focused_form(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        assert!(state.navigation.history.contains(&target));
        Ok(())
    }

    #[test]
    fn clicking_submit_input_submits_form_with_submitter() -> WebbyResult<()> {
        let target =
            url::Url::parse("https://example.test/find?q=rust&go=Search").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Results</body>")]);
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"text\" name=\"q\" value=\"rust\"><input type=\"submit\" name=\"go\" value=\"Search\"></form>",
        )?;
        let (x, y) = form_control_point(&state, FormControlType::Submit)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn clicking_submit_button_submits_form_with_submitter() -> WebbyResult<()> {
        let target = url::Url::parse("https://example.test/find?q=rust&go=1").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Button results</body>")]);
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"search\" name=\"q\" value=\"rust\"><button type=\"submit\" name=\"go\" value=\"1\">Go</button></form>",
        )?;
        let (x, y) = form_control_point(&state, FormControlType::Submit)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn button_type_button_click_is_noop_and_does_not_navigate() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"search\" name=\"q\" value=\"rust\"><button type=\"button\" name=\"noop\" value=\"1\">Noop</button></form>",
        )?;
        let current = state.navigation.current_url.clone();
        let original_history = state.navigation.history.clone();
        let (x, y) = form_control_point(&state, FormControlType::Button)?;

        assert!(state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader));

        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        assert_eq!(state.navigation.current_url, current);
        assert_eq!(state.navigation.history, original_history);
        Ok(())
    }

    #[test]
    fn chrome_hit_testing_exposes_mouse_actions() {
        let mut state = AppState::with_window_size(320, 180);
        state.new_tab();

        assert_eq!(
            state.chrome_action_at(10.0, 4.0),
            Some(ChromeAction::SwitchTab(0))
        );
        assert_eq!(
            state.chrome_action_at(150.0, 4.0),
            Some(ChromeAction::CloseTab(0))
        );
        assert_eq!(
            state.chrome_action_at(16.0, CHROME_BUTTON_Y as f32 + 2.0),
            Some(ChromeAction::Back)
        );
        assert_eq!(
            state.chrome_action_at(40.0, CHROME_BUTTON_Y as f32 + 2.0),
            Some(ChromeAction::Forward)
        );
        assert_eq!(
            state.chrome_action_at(64.0, CHROME_BUTTON_Y as f32 + 2.0),
            Some(ChromeAction::Reload)
        );
        assert_eq!(
            state.chrome_action_at(88.0, CHROME_BUTTON_Y as f32 + 2.0),
            Some(ChromeAction::BookmarkCurrentPage)
        );
        assert_eq!(
            state.chrome_action_at(ADDRESS_X as f32 + 4.0, ADDRESS_Y as f32 + 4.0),
            Some(ChromeAction::FocusAddress)
        );
    }

    #[test]
    fn clicking_address_bar_selects_text_and_next_typing_replaces_it() {
        let mut state = AppState::with_window_size(320, 180);
        state.chrome.set_address_input("https://example.test/");

        assert!(state.apply_chrome_action(ChromeAction::FocusAddress, &FailingLoader));
        state.type_character('w');

        assert_eq!(state.chrome.address_input, "w");
        assert!(state.chrome.address_focused);
        assert!(!state.chrome.address_selected);
    }

    #[test]
    fn chrome_tab_actions_switch_and_close_tabs() {
        let mut state = AppState::with_window_size(320, 180);
        state.new_tab();

        assert!(state.apply_chrome_action(ChromeAction::SwitchTab(0), &FailingLoader));
        assert_eq!(state.active_tab_index, 0);
        assert!(state.apply_chrome_action(ChromeAction::CloseTab(1), &FailingLoader));
        assert_eq!(state.tab_count(), 1);
        assert_eq!(state.active_tab_index, 0);
    }

    #[test]
    fn missing_action_submits_against_current_url() -> WebbyResult<()> {
        let target = url::Url::parse("https://example.test/form?q=rust").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Same page results</body>")]);
        let mut state =
            loaded_form_state("<form><input type=\"text\" name=\"q\" value=\"rust\"></form>")?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &loader);
        assert!(state.submit_focused_form(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn empty_name_controls_are_skipped_during_submission() -> WebbyResult<()> {
        let target = url::Url::parse("https://example.test/find?q=kept").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Results</body>")]);
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"text\" value=\"ignored\"><input type=\"text\" name=\"q\" value=\"kept\"></form>",
        )?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &loader);
        assert!(state.submit_focused_form(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn relative_action_resolves_against_current_url() -> WebbyResult<()> {
        let target =
            url::Url::parse("https://example.test/forms/results?q=rust").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Relative results</body>")]);
        let mut state = loaded_form_state_at(
            "https://example.test/forms/search.html",
            "<form action=\"results\"><input type=\"search\" name=\"q\" value=\"rust\"></form>",
        )?;
        let (x, y) = form_control_point(&state, FormControlType::Search)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &loader);
        assert!(state.submit_focused_form(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn invalid_form_action_becomes_error_without_history_commit() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"http://[::1\"><input type=\"text\" name=\"q\" value=\"rust\"></form>",
        )?;
        let original_history = state.navigation.history.clone();
        let current = state.navigation.current_url.clone();
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader);
        assert!(state.submit_focused_form(&FailingLoader));

        assert!(
            matches!(state.status, PageStatus::Error { ref message } if message.contains("invalid form action"))
        );
        assert_eq!(state.navigation.history, original_history);
        assert_eq!(state.navigation.current_url, current);
        Ok(())
    }

    #[test]
    fn unsupported_post_method_sets_error_without_history_commit() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/submit\" method=\"post\"><input type=\"text\" name=\"q\" value=\"rust\"></form>",
        )?;
        let original_history = state.navigation.history.clone();
        let current = state.navigation.current_url.clone();
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader);
        assert!(state.submit_focused_form(&FailingLoader));

        assert!(
            matches!(state.status, PageStatus::Error { ref message } if message.contains("unsupported"))
        );
        assert_eq!(state.navigation.history, original_history);
        assert_eq!(state.navigation.current_url, current);
        Ok(())
    }

    #[test]
    fn empty_method_is_explicit_unsupported_error() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/submit\" method=\"\"><input type=\"text\" name=\"q\" value=\"rust\"></form>",
        )?;
        let original_history = state.navigation.history.clone();
        let current = state.navigation.current_url.clone();
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader);
        assert!(state.submit_focused_form(&FailingLoader));

        assert!(
            matches!(state.status, PageStatus::Error { ref message } if message.contains("form method"))
        );
        assert_eq!(state.navigation.history, original_history);
        assert_eq!(state.navigation.current_url, current);
        Ok(())
    }

    #[test]
    fn input_outside_form_errors_instead_of_implicit_submission() -> WebbyResult<()> {
        let mut state = loaded_form_state("<input type=\"text\" name=\"q\" value=\"rust\">")?;
        let original_history = state.navigation.history.clone();
        let current = state.navigation.current_url.clone();
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader);
        assert!(state.submit_focused_form(&FailingLoader));

        assert!(
            matches!(state.status, PageStatus::Error { ref message } if message.contains("not inside a form"))
        );
        assert_eq!(state.navigation.history, original_history);
        assert_eq!(state.navigation.current_url, current);
        Ok(())
    }

    #[test]
    fn failed_form_navigation_does_not_commit_history() -> WebbyResult<()> {
        let mut state = loaded_form_state(
            "<form action=\"/missing\"><input type=\"text\" name=\"q\" value=\"rust\"></form>",
        )?;
        let original_history = state.navigation.history.clone();
        let current = state.navigation.current_url.clone();
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &FailingLoader);
        assert!(state.submit_focused_form(&FailingLoader));

        assert!(matches!(state.status, PageStatus::Error { .. }));
        assert_eq!(state.navigation.history, original_history);
        assert_eq!(state.navigation.current_url, current);
        Ok(())
    }

    #[test]
    fn successful_form_navigation_commits_history() -> WebbyResult<()> {
        let target = url::Url::parse("https://example.test/find?q=rust").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Results</body>")]);
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"text\" name=\"q\" value=\"rust\"></form>",
        )?;
        let (x, y) = form_control_point(&state, FormControlType::Text)?;

        state.click_at(x, y + CHROME_HEIGHT as f32, &loader);
        assert!(state.submit_focused_form(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        assert_eq!(state.navigation.history.last(), Some(&target));
        Ok(())
    }

    #[test]
    fn checkbox_radio_select_textarea_and_disabled_controls_serialize() -> WebbyResult<()> {
        let target =
            url::Url::parse("https://example.test/find?ok=yes&mode=b&choice=two&note=Hello")
                .map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Results</body>")]);
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"checkbox\" name=\"ok\" value=\"yes\"><input type=\"radio\" name=\"mode\" value=\"a\"><input type=\"radio\" name=\"mode\" value=\"b\"><select name=\"choice\"><option value=\"one\">One</option><option value=\"two\">Two</option></select><textarea name=\"note\">Hello</textarea><input name=\"skip\" value=\"no\" disabled><button type=\"submit\">Go</button></form>",
        )?;
        let (checkbox_x, checkbox_y) = form_control_point(&state, FormControlType::Checkbox)?;
        state.click_at(checkbox_x, checkbox_y + CHROME_HEIGHT as f32, &loader);
        let radios = state
            .page
            .as_ref()
            .map(|page| {
                page.form_controls
                    .iter()
                    .filter(|control| control.control_type == FormControlType::Radio)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let second_radio = radios
            .get(1)
            .ok_or_else(|| WebbyError::invalid_input("missing second radio"))?;
        state.click_at(
            second_radio.rect.x + 1.0,
            second_radio.rect.y + CHROME_HEIGHT as f32 + 1.0,
            &loader,
        );
        let (select_x, select_y) = form_control_point(&state, FormControlType::Select)?;
        state.click_at(select_x, select_y + CHROME_HEIGHT as f32, &loader);
        let (submit_x, submit_y) = form_control_point(&state, FormControlType::Submit)?;

        assert!(state.click_at(submit_x, submit_y + CHROME_HEIGHT as f32, &loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn password_and_email_inputs_edit_and_submit() -> WebbyResult<()> {
        let target =
            url::Url::parse("https://example.test/find?pw=secret%21&mail=a%40example.test")
                .map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Results</body>")]);
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"password\" name=\"pw\" value=\"secret\"><input type=\"email\" name=\"mail\" value=\"a@example.test\"></form>",
        )?;
        let (password_x, password_y) = form_control_point(&state, FormControlType::Password)?;

        state.click_at(password_x, password_y + CHROME_HEIGHT as f32, &loader);
        state.type_character('!');
        assert!(state.submit_focused_form(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn reset_button_restores_initial_form_values() -> WebbyResult<()> {
        let target = url::Url::parse("https://example.test/find?q=one").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Results</body>")]);
        let mut state = loaded_form_state(
            "<form action=\"/find\"><input type=\"text\" name=\"q\" value=\"one\"><button type=\"reset\">Reset</button></form>",
        )?;
        let (text_x, text_y) = form_control_point(&state, FormControlType::Text)?;
        state.click_at(text_x, text_y + CHROME_HEIGHT as f32, &loader);
        state.type_character('x');
        let (reset_x, reset_y) = form_control_point(&state, FormControlType::Reset)?;
        assert!(state.click_at(reset_x, reset_y + CHROME_HEIGHT as f32, &loader));

        assert!(state.submit_form_for_control(&loader, 0, None));
        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn post_form_submission_uses_urlencoded_body_and_navigates_response() -> WebbyResult<()> {
        let target = url::Url::parse("https://example.test/submit").map_err(url_error)?;
        let loader = RecordingPostLoader::new(target.as_str(), "<body>Posted</body>");
        let mut state = loaded_form_state(
            "<form action=\"/submit\" method=\"post\"><input type=\"text\" name=\"q\" value=\"rust browser\"><button type=\"submit\" name=\"go\" value=\"1\">Go</button></form>",
        )?;
        let (submit_x, submit_y) = form_control_point(&state, FormControlType::Submit)?;

        assert!(state.click_at(submit_x, submit_y + CHROME_HEIGHT as f32, &loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        assert_eq!(loader.last_body(), Some("q=rust+browser&go=1".to_string()));
        assert_eq!(state.navigation.history.last(), Some(&target));
        Ok(())
    }

    #[test]
    fn click_hit_testing_accounts_for_chrome_height() -> WebbyResult<()> {
        let state = loaded_link_state("/next")?;
        let (x, page_y) = first_link_point(&state)?;

        assert!(state.link_href_at_window_position(x, page_y).is_none());
        assert_eq!(
            state
                .link_href_at_window_position(x, page_y + CHROME_HEIGHT as f32)
                .as_deref(),
            Some("/next")
        );
        Ok(())
    }

    #[test]
    fn click_hit_testing_accounts_for_scroll_offset() -> WebbyResult<()> {
        let mut state = loaded_link_state("/next")?;
        state.scroll_y = 12.0;
        let (x, page_y) = first_link_point(&state)?;

        assert_eq!(
            state
                .link_href_at_window_position(x, page_y + CHROME_HEIGHT as f32 - 12.0)
                .as_deref(),
            Some("/next")
        );
        Ok(())
    }

    #[test]
    fn chrome_clicks_do_not_trigger_page_links() -> WebbyResult<()> {
        let mut state = loaded_link_state("/next")?;
        let loader = TestSiteLoader::new([]);

        assert!(!state.click_at(20.0, 20.0, &loader));
        assert!(state.navigation.pending_url.is_none());
        Ok(())
    }

    #[test]
    fn hover_state_reports_chrome_links_and_controls() -> WebbyResult<()> {
        let mut state =
            loaded_form_state("<form><input type=\"text\" name=\"q\" value=\"one\"></form>")?;
        assert_eq!(
            state.update_hover_at(16.0, CHROME_BUTTON_Y as f32 + 2.0),
            Some(HoverTarget::Chrome(ChromeAction::Back))
        );

        let mut linked = loaded_link_state("/next")?;
        let (link_x, link_y) = first_link_point(&linked)?;
        assert_eq!(
            linked.update_hover_at(link_x, link_y + CHROME_HEIGHT as f32),
            Some(HoverTarget::Link("/next".to_string()))
        );

        let (control_x, control_y) = form_control_point(&state, FormControlType::Text)?;
        assert!(matches!(
            state.update_hover_at(control_x, control_y + CHROME_HEIGHT as f32),
            Some(HoverTarget::FormControl(_))
        ));
        Ok(())
    }

    #[test]
    fn hit_testing_returns_no_link_while_error_is_visible() -> WebbyResult<()> {
        let mut state = loaded_link_state("/next")?;
        let (x, page_y) = first_link_point(&state)?;
        state.finish_navigation(Err(WebbyError::Network {
            message: "failed".to_string(),
        }));

        assert!(matches!(state.status, PageStatus::Error { .. }));
        assert!(
            state
                .link_href_at_window_position(x, page_y + CHROME_HEIGHT as f32)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn hit_testing_returns_no_link_while_loading_is_visible() -> WebbyResult<()> {
        let mut state = loaded_link_state("/next")?;
        let (x, page_y) = first_link_point(&state)?;
        let target = url::Url::parse("https://example.test/loading").map_err(url_error)?;
        state.begin_navigation_to_url(target, super::PendingHistoryAction::Push);

        assert!(matches!(state.status, PageStatus::Loading { .. }));
        assert!(
            state
                .link_href_at_window_position(x, page_y + CHROME_HEIGHT as f32)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn display_none_link_and_form_controls_are_not_interactive() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>.hidden { display: none; }</style><body><div class=\"hidden\"><a href=\"/gone\">Gone</a><form action=\"/find\"><input name=\"q\"></form></div><p>Visible</p></body>";
        let page = PagePipeline::new(240, 160).render_html(html, base)?;
        let mut state = AppState::with_window_size(240, 220);
        state.finish_navigation(Ok(page));

        assert!(
            state
                .page
                .as_ref()
                .is_some_and(|page| page.links.is_empty() && page.form_controls.is_empty())
        );
        assert!(
            state
                .link_href_at_window_position(20.0, CHROME_HEIGHT as f32 + 20.0)
                .is_none()
        );
        assert!(
            state
                .form_control_at_window_position(20.0, CHROME_HEIGHT as f32 + 20.0)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn visibility_hidden_link_and_form_controls_are_not_interactive() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = "<style>.hidden { visibility: hidden; }</style><body><div class=\"hidden\"><a href=\"/gone\">Gone</a><form action=\"/find\"><input name=\"q\"></form></div><p>Visible</p></body>";
        let page = PagePipeline::new(240, 160).render_html(html, base)?;
        let mut state = AppState::with_window_size(240, 220);
        state.finish_navigation(Ok(page));

        assert!(
            state
                .page
                .as_ref()
                .is_some_and(|page| page.links.is_empty() && page.form_controls.is_empty())
        );
        Ok(())
    }

    #[test]
    fn clicking_link_starts_navigation_with_correct_href() -> WebbyResult<()> {
        let mut state = loaded_link_state("/next")?;
        let next_url = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([(next_url.as_str(), "<body>Next</body>")]);
        let (x, page_y) = first_link_point(&state)?;

        assert!(state.click_at(x, page_y + CHROME_HEIGHT as f32, &loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&next_url));
        Ok(())
    }

    #[test]
    fn relative_href_resolves_against_current_url() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/docs/index.html").map_err(url_error)?;
        let target = url::Url::parse("https://example.test/docs/next.html").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Next</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.finish_navigation(Ok(PagePipeline::new(240, 112)
            .render_html("<body><a href=\"next.html\">Next</a></body>", base)?));
        let (x, page_y) = first_link_point(&state)?;

        state.click_at(x, page_y + CHROME_HEIGHT as f32, &loader);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn file_relative_href_resolves_against_current_file_url() -> WebbyResult<()> {
        let base = url::Url::parse("file:///tmp/webby/docs/index.html").map_err(url_error)?;
        let target = url::Url::parse("file:///tmp/webby/docs/next.html").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Next</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.finish_navigation(Ok(PagePipeline::new(240, 112)
            .render_html("<body><a href=\"next.html\">Next</a></body>", base)?));
        let (x, page_y) = first_link_point(&state)?;

        state.click_at(x, page_y + CHROME_HEIGHT as f32, &loader);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn absolute_href_resolves_to_absolute_target() -> WebbyResult<()> {
        let mut state = loaded_link_state("https://other.example/path")?;
        let target = url::Url::parse("https://other.example/path").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Other</body>")]);
        let (x, page_y) = first_link_point(&state)?;

        state.click_at(x, page_y + CHROME_HEIGHT as f32, &loader);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn root_relative_href_resolves_against_origin() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/docs/index.html").map_err(url_error)?;
        let target = url::Url::parse("https://example.test/docs").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Docs</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.finish_navigation(Ok(PagePipeline::new(240, 112)
            .render_html("<body><a href=\"/docs\">Docs</a></body>", base)?));
        let (x, page_y) = first_link_point(&state)?;

        state.click_at(x, page_y + CHROME_HEIGHT as f32, &loader);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn protocol_relative_href_is_supported_for_http_pages() -> WebbyResult<()> {
        let mut state = loaded_link_state("//other.example/path")?;
        let target = url::Url::parse("https://other.example/path").map_err(url_error)?;
        let loader = TestSiteLoader::new([(target.as_str(), "<body>Other</body>")]);
        let (x, page_y) = first_link_point(&state)?;

        state.click_at(x, page_y + CHROME_HEIGHT as f32, &loader);

        assert_eq!(state.navigation.current_url.as_ref(), Some(&target));
        Ok(())
    }

    #[test]
    fn invalid_href_becomes_error_state() -> WebbyResult<()> {
        let mut state = loaded_link_state("http://[::1")?;
        let loader = TestSiteLoader::new([]);
        let (x, page_y) = first_link_point(&state)?;

        assert!(state.click_at(x, page_y + CHROME_HEIGHT as f32, &loader));

        assert!(matches!(state.status, PageStatus::Error { .. }));
        assert_eq!(state.navigation.failed_url.as_deref(), Some("http://[::1"));
        Ok(())
    }

    #[test]
    fn successful_navigation_commits_history() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (home.as_str(), "<body>Home</body>"),
            (next.as_str(), "<body>Next</body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home.clone());
        state.navigate_to_url(&loader, next.clone());

        assert_eq!(state.navigation.history, vec![home, next]);
        assert_eq!(state.navigation.history_index, Some(1));
        Ok(())
    }

    #[test]
    fn failed_navigation_does_not_commit_history() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let missing = url::Url::parse("https://example.test/missing").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home.clone());

        state.navigate_to_url(&loader, missing);

        assert_eq!(state.navigation.history, vec![home]);
        assert_eq!(state.navigation.history_index, Some(0));
        assert!(matches!(state.status, PageStatus::Error { .. }));
        Ok(())
    }

    #[test]
    fn back_navigates_to_previous_successful_entry() -> WebbyResult<()> {
        let (mut state, loader, home, _) = two_page_history_state()?;

        assert!(state.go_back(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert_eq!(state.navigation.history_index, Some(0));
        Ok(())
    }

    #[test]
    fn failed_back_traversal_preserves_current_history_index_and_url() -> WebbyResult<()> {
        let (mut state, _, _, next) = two_page_history_state()?;

        assert!(state.go_back(&FailingLoader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        assert_eq!(state.navigation.history_index, Some(1));
        assert!(matches!(state.status, PageStatus::Error { .. }));
        Ok(())
    }

    #[test]
    fn forward_navigates_to_next_successful_entry() -> WebbyResult<()> {
        let (mut state, loader, _, next) = two_page_history_state()?;
        state.go_back(&loader);

        assert!(state.go_forward(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&next));
        assert_eq!(state.navigation.history_index, Some(1));
        Ok(())
    }

    #[test]
    fn failed_forward_traversal_preserves_current_history_index_and_url() -> WebbyResult<()> {
        let (mut state, loader, home, _) = two_page_history_state()?;
        state.go_back(&loader);

        assert!(state.go_forward(&FailingLoader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert_eq!(state.navigation.history_index, Some(0));
        assert!(matches!(state.status, PageStatus::Error { .. }));
        Ok(())
    }

    #[test]
    fn new_navigation_after_back_truncates_forward_history() -> WebbyResult<()> {
        let (mut state, loader, home, _) = two_page_history_state()?;
        let other = url::Url::parse("https://example.test/other").map_err(url_error)?;
        state.go_back(&loader);
        let loader = TestSiteLoader::new([
            (home.as_str(), "<body>Home</body>"),
            (other.as_str(), "<body>Other</body>"),
        ]);

        state.navigate_to_url(&loader, other.clone());

        assert_eq!(state.navigation.history, vec![home, other]);
        assert_eq!(state.navigation.history_index, Some(1));
        Ok(())
    }

    #[test]
    fn reload_uses_current_url() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home.clone());

        assert!(state.reload(&loader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert_eq!(state.navigation.history, vec![home]);
        Ok(())
    }

    #[test]
    fn reload_failure_preserves_history_and_current_url_semantics() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home.clone());

        assert!(state.reload(&FailingLoader));

        assert_eq!(state.navigation.current_url.as_ref(), Some(&home));
        assert_eq!(state.navigation.history, vec![home.clone()]);
        assert_eq!(state.navigation.failed_url.as_deref(), Some(home.as_str()));
        Ok(())
    }

    #[test]
    fn address_bar_updates_after_successful_navigation() -> WebbyResult<()> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = TestSiteLoader::new([(home.as_str(), "<body>Home</body>")]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, home.clone());

        assert_eq!(state.chrome.address_input, home.as_str());
        Ok(())
    }

    #[test]
    fn failed_navigation_records_failed_url() -> WebbyResult<()> {
        let missing = url::Url::parse("https://example.test/missing").map_err(url_error)?;
        let loader = TestSiteLoader::new([]);
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, missing.clone());

        assert_eq!(
            state.navigation.failed_url.as_deref(),
            Some(missing.as_str())
        );
        Ok(())
    }

    #[test]
    fn address_bar_keeps_last_successful_url_after_failed_link_navigation() -> WebbyResult<()> {
        let mut state = loaded_link_state("/missing")?;
        let current_url = state
            .navigation
            .current_url
            .clone()
            .ok_or_else(|| WebbyError::invalid_input("test state has no current URL"))?;
        let (x, page_y) = first_link_point(&state)?;

        state.click_at(x, page_y + CHROME_HEIGHT as f32, &FailingLoader);

        assert_eq!(state.chrome.address_input, current_url.as_str());
        assert_eq!(
            state.navigation.failed_url.as_deref(),
            Some("https://example.test/missing")
        );
        assert!(matches!(state.status, PageStatus::Error { .. }));
        Ok(())
    }

    #[test]
    fn composed_frame_keeps_page_below_chrome() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = StaticHtmlLoader::new("<body>Hello</body>", url);
        let mut state = AppState::with_window_size(160, 140);
        state.chrome.set_address_input("example.test");
        state.submit_address(&loader);

        let frame = state.compose_frame()?;

        assert_eq!(frame.width, 160);
        assert_eq!(frame.height, 140);
        assert!(region_has_pixel_other_than(
            &frame,
            20,
            12,
            120,
            24,
            [255, 255, 255, 255]
        ));
        assert_ne!(pixel(&frame, 20, CHROME_HEIGHT + 4), [232, 235, 239, 255]);
        Ok(())
    }

    #[test]
    fn chrome_text_uses_shared_text_renderer() -> WebbyResult<()> {
        let mut state = AppState::with_window_size(180, 100);
        state.chrome.set_address_input("webby");

        let frame = state.compose_frame()?;

        assert!(region_has_pixel_other_than(
            &frame,
            20,
            12,
            80,
            24,
            [255, 255, 255, 255]
        ));
        Ok(())
    }

    fn pixel(surface: &webby_render::Surface, x: usize, y: usize) -> [u8; 4] {
        let index = (y * surface.width + x) * 4;
        [
            surface.pixels[index],
            surface.pixels[index + 1],
            surface.pixels[index + 2],
            surface.pixels[index + 3],
        ]
    }

    fn region_has_pixel_other_than(
        surface: &webby_render::Surface,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        color: [u8; 4],
    ) -> bool {
        let x_end = x.saturating_add(width).min(surface.width);
        let y_end = y.saturating_add(height).min(surface.height);
        for row in y..y_end {
            for column in x..x_end {
                if pixel(surface, column, row) != color {
                    return true;
                }
            }
        }
        false
    }

    fn temp_download_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("webby-download-{name}-{}", std::process::id()))
    }

    #[derive(Debug)]
    struct DownloadResponseLoader {
        url: url::Url,
        content_type: String,
        headers: Vec<(String, String)>,
        bytes: Vec<u8>,
    }

    impl DownloadResponseLoader {
        fn new(
            url: url::Url,
            content_type: &str,
            headers: Vec<(String, String)>,
            bytes: Vec<u8>,
        ) -> Self {
            Self {
                url,
                content_type: content_type.to_string(),
                headers,
                bytes,
            }
        }
    }

    impl ResourceLoader for DownloadResponseLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            if url != &self.url {
                return Err(WebbyError::Network {
                    message: format!("unexpected download URL {url}"),
                });
            }
            Ok(ResourceResponse {
                requested_url: url.clone(),
                final_url: url.clone(),
                status: Some(200),
                content_type: Some(self.content_type.clone()),
                headers: self.headers.clone(),
                bytes: self.bytes.clone(),
            })
        }
    }

    fn url_error(error: url::ParseError) -> WebbyError {
        WebbyError::Url {
            message: error.to_string(),
        }
    }

    fn begin_url(state: &mut AppState, input: &str) -> WebbyResult<url::Url> {
        state.chrome.set_address_input(input);
        state.begin_navigation().ok_or_else(|| WebbyError::Url {
            message: format!("test input did not resolve: {input}"),
        })
    }

    fn loaded_link_state(href: &str) -> WebbyResult<AppState> {
        let base = url::Url::parse("https://example.test/").map_err(url_error)?;
        let html = format!("<body><p><a href=\"{href}\">Next page</a></p></body>");
        let page = PagePipeline::new(240, 160).render_html(&html, base)?;
        let mut state = AppState::with_window_size(240, 220);
        state.finish_navigation(Ok(page));
        Ok(state)
    }

    fn loaded_debug_state() -> WebbyResult<AppState> {
        let base = url::Url::parse("https://example.test/debug").map_err(url_error)?;
        let html = r#"
            <style>
                p { margin: 4px; padding: 3px; border: 2px solid red; background: #00ff00; }
            </style>
            <body><div><p id="intro" class="card highlighted">Debug target</p></div></body>
        "#;
        let page = PagePipeline::new(260, 180).render_html(html, base)?;
        let mut state = AppState::with_window_size(260, 240);
        state.finish_navigation(Ok(page));
        Ok(state)
    }

    fn first_box_by_tag<'a>(state: &'a AppState, tag_name: &str) -> WebbyResult<&'a LayoutBox> {
        let page = state
            .page
            .as_ref()
            .ok_or_else(|| WebbyError::invalid_input("test state has no page"))?;
        find_box_by_tag(&page.layout.root, tag_name)
            .ok_or_else(|| WebbyError::invalid_input(format!("test page has no <{tag_name}> box")))
    }

    fn find_box_by_tag<'a>(layout_box: &'a LayoutBox, tag_name: &str) -> Option<&'a LayoutBox> {
        if tag_name_for_kind(&layout_box.kind) == tag_name {
            return Some(layout_box);
        }
        for item in &layout_box.contents {
            match item {
                webby_layout::LayoutItem::LineBox(line) => {
                    for fragment in &line.fragments {
                        if let webby_layout::InlineFragment::Box(child) = fragment
                            && let Some(found) = find_box_by_tag(child, tag_name)
                        {
                            return Some(found);
                        }
                    }
                }
                webby_layout::LayoutItem::Box(child) => {
                    if let Some(found) = find_box_by_tag(child, tag_name) {
                        return Some(found);
                    }
                }
                webby_layout::LayoutItem::Text(_) => {}
            }
        }
        None
    }

    fn dummy_inspection() -> InspectionInfo {
        InspectionInfo {
            tag_name: "p".to_string(),
            id: None,
            classes: Vec::new(),
            dimensions: Dimensions {
                content: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                padding: webby_layout::EdgeSizes::ZERO,
                border: webby_layout::EdgeSizes::ZERO,
                margin: webby_layout::EdgeSizes::ZERO,
            },
            computed_style: String::new(),
            paint_summary: String::new(),
            link_href: None,
            image_metadata: None,
            form_metadata: None,
        }
    }

    fn first_link_point(state: &AppState) -> WebbyResult<(f32, f32)> {
        let Some(page) = &state.page else {
            return Err(WebbyError::invalid_input("test state has no page"));
        };
        let Some(link) = page.links.first() else {
            return Err(WebbyError::invalid_input("test page has no links"));
        };
        Ok((link.rect.x + 1.0, link.rect.y + 1.0))
    }

    fn loaded_form_state(form_html: &str) -> WebbyResult<AppState> {
        loaded_form_state_at("https://example.test/form", form_html)
    }

    fn loaded_form_state_at(url: &str, form_html: &str) -> WebbyResult<AppState> {
        let base = url::Url::parse(url).map_err(url_error)?;
        let html = format!("<body>{form_html}</body>");
        let page = PagePipeline::new(320, 180).render_html(&html, base)?;
        let mut state = AppState::with_window_size(320, 240);
        state.finish_navigation(Ok(page));
        Ok(state)
    }

    fn temp_profile_store(name: &str) -> webby_state::ProfileStore {
        let root = std::env::temp_dir().join(format!("webby-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        webby_state::ProfileStore::new(root)
    }

    fn form_control_point(
        state: &AppState,
        control_type: FormControlType,
    ) -> WebbyResult<(f32, f32)> {
        let Some(page) = &state.page else {
            return Err(WebbyError::invalid_input("test state has no page"));
        };
        let Some(control) = page
            .form_controls
            .iter()
            .find(|control| control.control_type == control_type)
        else {
            return Err(WebbyError::invalid_input("test page has no form control"));
        };
        Ok((control.rect.x + 1.0, control.rect.y + 1.0))
    }

    fn iframe_link_window_point(state: &AppState) -> WebbyResult<(f32, f32)> {
        let Some(page) = &state.page else {
            return Err(WebbyError::invalid_input("test state has no page"));
        };
        let Some(iframe) = page.iframes.first() else {
            return Err(WebbyError::invalid_input("test page has no iframe"));
        };
        let Some(link) = iframe.page.links.first() else {
            return Err(WebbyError::invalid_input("test iframe has no link"));
        };
        Ok((
            iframe.rect.x + link.rect.x + 1.0,
            CHROME_HEIGHT as f32 + iframe.rect.y + link.rect.y + 1.0 - state.scroll_y,
        ))
    }

    fn two_page_history_state() -> WebbyResult<(AppState, TestSiteLoader, url::Url, url::Url)> {
        let home = url::Url::parse("https://example.test/").map_err(url_error)?;
        let next = url::Url::parse("https://example.test/next").map_err(url_error)?;
        let loader = TestSiteLoader::new([
            (home.as_str(), "<body>Home</body>"),
            (next.as_str(), "<body>Next</body>"),
        ]);
        let mut state = AppState::with_window_size(240, 160);
        state.navigate_to_url(&loader, home.clone());
        state.navigate_to_url(&loader, next.clone());
        Ok((state, loader, home, next))
    }

    fn encoded_test_image(format: ImageFormat) -> WebbyResult<Vec<u8>> {
        let image = ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 255, 0, 255])
            }
        });
        let mut cursor = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut cursor, format)
            .map_err(|error| WebbyError::Render {
                message: format!("failed to encode test image: {error}"),
            })?;
        Ok(cursor.into_inner())
    }

    #[derive(Debug, Clone)]
    struct ByteResourceLoader {
        resources: Vec<(String, String, Vec<u8>)>,
    }

    impl ByteResourceLoader {
        fn new<const N: usize>(resources: [(&str, &str, Vec<u8>); N]) -> Self {
            Self {
                resources: resources
                    .into_iter()
                    .map(|(url, content_type, bytes)| {
                        (url.to_string(), content_type.to_string(), bytes)
                    })
                    .collect(),
            }
        }
    }

    impl ResourceLoader for ByteResourceLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            for (resource_url, content_type, bytes) in &self.resources {
                if resource_url == url.as_str() {
                    return Ok(ResourceResponse {
                        requested_url: url.clone(),
                        final_url: url.clone(),
                        status: Some(200),
                        content_type: Some(content_type.clone()),
                        headers: Vec::new(),
                        bytes: bytes.clone(),
                    });
                }
            }

            Err(WebbyError::Network {
                message: format!("no test resource for {url}"),
            })
        }
    }

    #[derive(Debug)]
    struct CountingByteResourceLoader {
        resources: Vec<(String, String, Vec<u8>)>,
        calls: Cell<usize>,
    }

    impl CountingByteResourceLoader {
        fn new<const N: usize>(resources: [(&str, &str, Vec<u8>); N]) -> Self {
            Self {
                resources: resources
                    .into_iter()
                    .map(|(url, content_type, bytes)| {
                        (url.to_string(), content_type.to_string(), bytes)
                    })
                    .collect(),
                calls: Cell::new(0),
            }
        }
    }

    impl ResourceLoader for CountingByteResourceLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            self.calls.set(self.calls.get().saturating_add(1));
            for (resource_url, content_type, bytes) in &self.resources {
                if resource_url == url.as_str() {
                    return Ok(ResourceResponse {
                        requested_url: url.clone(),
                        final_url: url.clone(),
                        status: Some(200),
                        content_type: Some(content_type.clone()),
                        headers: Vec::new(),
                        bytes: bytes.clone(),
                    });
                }
            }

            Err(WebbyError::Network {
                message: format!("no counted test resource for {url}"),
            })
        }
    }

    #[derive(Debug)]
    struct CookieSessionLoader {
        login: url::Url,
        account: url::Url,
        account_cookie_seen: Cell<bool>,
    }

    impl CookieSessionLoader {
        fn new(login: url::Url, account: url::Url) -> Self {
            Self {
                login,
                account,
                account_cookie_seen: Cell::new(false),
            }
        }
    }

    impl ResourceLoader for CookieSessionLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            self.load_with_headers(url, &[])
        }

        fn load_with_headers(
            &self,
            url: &url::Url,
            headers: &[(String, String)],
        ) -> WebbyResult<ResourceResponse> {
            if url == &self.login {
                return Ok(ResourceResponse {
                    requested_url: url.clone(),
                    final_url: url.clone(),
                    status: Some(200),
                    content_type: Some("text/html; charset=utf-8".to_string()),
                    headers: vec![(
                        "set-cookie".to_string(),
                        "sid=abc; Path=/; Max-Age=60".to_string(),
                    )],
                    bytes: b"<body>Login</body>".to_vec(),
                });
            }
            if url == &self.account {
                self.account_cookie_seen
                    .set(headers.iter().any(|(name, value)| {
                        name.eq_ignore_ascii_case("cookie") && value == "sid=abc"
                    }));
                return Ok(ResourceResponse {
                    requested_url: url.clone(),
                    final_url: url.clone(),
                    status: Some(200),
                    content_type: Some("text/html; charset=utf-8".to_string()),
                    headers: Vec::new(),
                    bytes: b"<body>Account</body>".to_vec(),
                });
            }
            Err(WebbyError::Network {
                message: format!("no cookie test resource for {url}"),
            })
        }
    }

    #[derive(Debug)]
    struct BasicAuthLoader {
        url: url::Url,
        expected_header: String,
        authorized_calls: Cell<usize>,
    }

    impl BasicAuthLoader {
        fn new(url: url::Url, username: &str, password: &str) -> Self {
            let credentials = webby_net::BasicCredentials::new(username, password);
            Self {
                url,
                expected_header: credentials.authorization_header_value(),
                authorized_calls: Cell::new(0),
            }
        }
    }

    impl ResourceLoader for BasicAuthLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            self.load_with_headers(url, &[])
        }

        fn load_with_headers(
            &self,
            url: &url::Url,
            headers: &[(String, String)],
        ) -> WebbyResult<ResourceResponse> {
            if url != &self.url {
                return Err(WebbyError::Network {
                    message: format!("unexpected auth URL {url}"),
                });
            }
            let authorized = headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("authorization") && value == &self.expected_header
            });
            if authorized {
                self.authorized_calls
                    .set(self.authorized_calls.get().saturating_add(1));
                return Ok(ResourceResponse {
                    requested_url: url.clone(),
                    final_url: url.clone(),
                    status: Some(200),
                    content_type: Some("text/html; charset=utf-8".to_string()),
                    headers: Vec::new(),
                    bytes: b"<body>Private</body>".to_vec(),
                });
            }
            Ok(ResourceResponse {
                requested_url: url.clone(),
                final_url: url.clone(),
                status: Some(401),
                content_type: Some("text/html; charset=utf-8".to_string()),
                headers: vec![(
                    "www-authenticate".to_string(),
                    "Basic realm=\"Members\"".to_string(),
                )],
                bytes: b"<body>Auth required</body>".to_vec(),
            })
        }
    }

    #[derive(Debug, Default)]
    struct SequenceHtmlLoader {
        calls: Cell<usize>,
    }

    impl ResourceLoader for SequenceHtmlLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            let next = self.calls.get().saturating_add(1);
            self.calls.set(next);
            Ok(ResourceResponse {
                requested_url: url.clone(),
                final_url: url.clone(),
                status: Some(200),
                content_type: Some("text/html; charset=utf-8".to_string()),
                headers: Vec::new(),
                bytes: format!("<body>v{next}</body>").into_bytes(),
            })
        }
    }

    #[derive(Debug, Clone)]
    struct TestSiteLoader {
        pages: Vec<(String, String)>,
    }

    impl TestSiteLoader {
        fn new<const N: usize>(pages: [(&str, &str); N]) -> Self {
            Self {
                pages: pages
                    .into_iter()
                    .map(|(url, html)| (url.to_string(), html.to_string()))
                    .collect(),
            }
        }
    }

    impl ResourceLoader for TestSiteLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            for (page_url, html) in &self.pages {
                if page_url == url.as_str() {
                    return Ok(ResourceResponse {
                        requested_url: url.clone(),
                        final_url: url.clone(),
                        status: Some(200),
                        content_type: Some("text/html; charset=utf-8".to_string()),
                        headers: Vec::new(),
                        bytes: html.as_bytes().to_vec(),
                    });
                }
            }

            Err(WebbyError::Network {
                message: format!("no test page for {url}"),
            })
        }
    }

    #[derive(Debug, Clone)]
    struct HeaderSiteLoader {
        pages: Vec<HeaderSitePage>,
    }

    #[derive(Debug, Clone)]
    struct HeaderSitePage {
        url: String,
        html: String,
        headers: Vec<(String, String)>,
    }

    type HeaderSiteInput<'a> = (&'a str, &'a str, Vec<(String, String)>);

    impl HeaderSiteLoader {
        fn new<const N: usize>(pages: [HeaderSiteInput<'_>; N]) -> Self {
            Self {
                pages: pages
                    .into_iter()
                    .map(|(url, html, headers)| HeaderSitePage {
                        url: url.to_string(),
                        html: html.to_string(),
                        headers,
                    })
                    .collect(),
            }
        }
    }

    impl ResourceLoader for HeaderSiteLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            for page in &self.pages {
                if page.url == url.as_str() {
                    return Ok(ResourceResponse {
                        requested_url: url.clone(),
                        final_url: url.clone(),
                        status: Some(200),
                        content_type: Some("text/html; charset=utf-8".to_string()),
                        headers: page.headers.clone(),
                        bytes: page.html.as_bytes().to_vec(),
                    });
                }
            }

            Err(WebbyError::Network {
                message: format!("no test page for {url}"),
            })
        }
    }

    #[derive(Debug)]
    struct RecordingPostLoader {
        target: String,
        html: String,
        last_body: RefCell<Option<String>>,
    }

    impl RecordingPostLoader {
        fn new(target: &str, html: &str) -> Self {
            Self {
                target: target.to_string(),
                html: html.to_string(),
                last_body: RefCell::new(None),
            }
        }

        fn last_body(&self) -> Option<String> {
            self.last_body.borrow().clone()
        }
    }

    impl ResourceLoader for RecordingPostLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            Err(WebbyError::Network {
                message: format!("unexpected GET for {url}"),
            })
        }

        fn submit_form_urlencoded(
            &self,
            url: &url::Url,
            _headers: &[(String, String)],
            body: &[u8],
        ) -> WebbyResult<ResourceResponse> {
            if url.as_str() != self.target {
                return Err(WebbyError::Network {
                    message: format!("unexpected POST for {url}"),
                });
            }
            *self.last_body.borrow_mut() = Some(String::from_utf8_lossy(body).into_owned());
            Ok(ResourceResponse {
                requested_url: url.clone(),
                final_url: url.clone(),
                status: Some(200),
                content_type: Some("text/html; charset=utf-8".to_string()),
                headers: Vec::new(),
                bytes: self.html.as_bytes().to_vec(),
            })
        }
    }

    fn display_texts(page: &RenderedPage) -> Vec<&str> {
        page.display_list
            .commands
            .iter()
            .filter_map(|command| match command {
                webby_render::DisplayCommand::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn animated_text_color(transition: &ActiveTransition, text: &str) -> WebbyResult<[u8; 4]> {
        let list = super::interpolated_display_list(transition);
        list.commands
            .iter()
            .find_map(|command| match command {
                webby_render::DisplayCommand::DrawText {
                    text: command_text,
                    color,
                    ..
                } if command_text == text => Some([color.r, color.g, color.b, color.a]),
                _ => None,
            })
            .ok_or_else(|| WebbyError::invalid_input("missing animated text command"))
    }

    fn animated_text_rect(transition: &ActiveTransition, text: &str) -> WebbyResult<Rect> {
        let list = super::interpolated_display_list(transition);
        list.commands
            .iter()
            .find_map(|command| match command {
                webby_render::DisplayCommand::DrawText {
                    text: command_text,
                    rect,
                    ..
                } if command_text == text => Some(*rect),
                _ => None,
            })
            .ok_or_else(|| WebbyError::invalid_input("missing animated text command"))
    }

    fn surface_has_color(surface: &Surface, rgba: [u8; 4]) -> bool {
        surface
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == rgba.as_slice())
    }

    #[test]
    fn app_applies_local_storage_actions_from_page_load() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = StaticHtmlLoader::new(
            "<body><script>localStorage.setItem('theme', 'dark')</script><p>Done</p></body>",
            url.clone(),
        );
        let mut state = AppState::with_window_size(240, 160);

        state.navigate_to_url(&loader, url.clone());

        let origin = webby_state::storage_origin_key(&url)?;
        assert_eq!(state.local_storage.get_item(&origin, "theme"), Some("dark"));
        assert!(matches!(state.status, PageStatus::Loaded { .. }));
        Ok(())
    }

    #[test]
    fn app_records_local_storage_with_persistent_profile_history() -> WebbyResult<()> {
        let store = temp_profile_store("app-local-storage-persist");
        let mut profile = webby_state::BrowserProfile::default();
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = StaticHtmlLoader::new(
            "<body><script>localStorage.setItem('mode', 'reader')</script><p>Done</p></body>",
            url.clone(),
        );
        let mut state = AppState::with_profile_and_window_size(&profile, 240, 160)?;

        state.navigate_to_url(&loader, url.clone());
        assert!(state.record_successful_navigation(&store, &mut profile)?);
        let loaded = store.load()?;
        let origin = webby_state::storage_origin_key(&url)?;

        assert_eq!(
            loaded.local_storage.get_item(&origin, "mode"),
            Some("reader")
        );
        Ok(())
    }

    #[test]
    fn pipeline_storage_snapshot_can_drive_dom_updates() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let page = PagePipeline::new(240, 160)
            .with_storage(
                true,
                vec![webby_js::StorageEntry {
                    key: "message".to_string(),
                    value: "Stored text".to_string(),
                }],
                Vec::new(),
            )
            .render_html(
                "<body><p id=\"out\">Old</p><script>document.getElementById('out').textContent = localStorage.getItem('message')</script></body>",
                url,
            )?;

        assert_eq!(
            webby_html::extract_visible_text(&page.document),
            "Stored text"
        );
        Ok(())
    }

    #[test]
    fn session_storage_is_isolated_per_tab() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let origin = webby_state::storage_origin_key(&url)?;
        let mut state = AppState::with_window_size(240, 160);

        state.session_storage.set_item(&origin, "tab", "one")?;
        state.save_active_tab();
        let first = state.active_tab_index;
        let second = state.new_tab();

        assert_eq!(second, 1);
        assert_eq!(state.session_storage.get_item(&origin, "tab"), None);
        assert!(state.switch_tab(first));
        assert_eq!(state.session_storage.get_item(&origin, "tab"), Some("one"));
        Ok(())
    }

    #[test]
    fn clear_session_storage_preserves_tabs_and_active_context() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/").map_err(url_error)?;
        let origin = webby_state::storage_origin_key(&url)?;
        let mut state = AppState::with_window_size(240, 160);

        state.session_storage.set_item(&origin, "tab", "one")?;
        state.save_active_tab();
        let second = state.new_tab();
        state.session_storage.set_item(&origin, "tab", "two")?;
        state.save_active_tab();

        state.clear_session_storage();

        assert_eq!(state.tab_count(), 2);
        assert_eq!(state.active_tab_index, second);
        assert_eq!(state.session_storage.get_item(&origin, "tab"), None);
        assert!(state.switch_tab(0));
        assert_eq!(state.session_storage.get_item(&origin, "tab"), None);
        state.validate_active_tab_sync()?;
        Ok(())
    }

    #[test]
    fn app_pipeline_renders_inline_svg_pixels() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/svg").map_err(url_error)?;
        let page = PagePipeline::new(120, 80).render_html(
            "<body><svg width=\"20\" height=\"12\"><rect x=\"0\" y=\"0\" width=\"10\" height=\"8\" fill=\"red\"/></svg></body>",
            url,
        )?;

        assert!(page.display_list.commands.iter().any(|command| {
            matches!(command, webby_render::DisplayCommand::FillRect { color, .. } if color.r == 255 && color.g == 0 && color.b == 0)
        }));
        assert!(surface_has_color(&page.surface, [255, 0, 0, 255]));
        Ok(())
    }

    #[test]
    fn app_pipeline_renders_canvas_fill_and_clear_pixels() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/canvas").map_err(url_error)?;
        let page = PagePipeline::new(120, 80).render_html(
            "<body><canvas id=\"c\" width=\"20\" height=\"12\"></canvas><script>
             var ctx = document.getElementById('c').getContext('2d');
             ctx.fillStyle = 'blue';
             ctx.fillRect(0, 0, 10, 8);
             ctx.clearRect(0, 0, 2, 2);
             </script></body>",
            url,
        )?;

        assert!(surface_has_color(&page.surface, [0, 0, 255, 255]));
        assert!(surface_has_color(&page.surface, [255, 255, 255, 255]));
        Ok(())
    }

    #[test]
    fn invalid_svg_and_canvas_operations_do_not_panic() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/bad-graphics").map_err(url_error)?;
        let page = PagePipeline::new(120, 80).render_html(
            "<body><svg width=\"bad\" height=\"10\"><rect width=\"nope\" fill=\"notacolor\"/></svg><canvas width=\"10\" height=\"10\" data-webby-canvas=\"fillRect,not,a,number\"></canvas></body>",
            url,
        )?;

        assert!(page.content_height > 0.0);
        Ok(())
    }

    #[test]
    fn html_parser_recovery_diagnostics_flow_into_page_state() -> WebbyResult<()> {
        let url = url::Url::parse("https://example.test/recovery").map_err(url_error)?;
        let page = PagePipeline::new(120, 80).render_html("<body>Visible<!-- missing", url)?;

        assert_eq!(webby_html::extract_visible_text(&page.document), "Visible");
        assert!(
            page.diagnostics.contains(
                &"HTML diagnostic at byte 13: unclosed comment ignored through end of input"
                    .to_string()
            )
        );
        Ok(())
    }

    struct FailingLoader;

    impl ResourceLoader for FailingLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            Err(WebbyError::Network {
                message: format!("failed test load for {url}"),
            })
        }
    }
}
/// Cache tiers supplied to one loader-backed page pipeline.
#[derive(Debug, Clone, Copy)]
pub struct CacheTiers<'a> {
    /// Shared in-memory fast path.
    pub memory: &'a ResourceCache,
    /// Optional persistent profile cache.
    pub disk: Option<&'a DiskResourceCache>,
}
