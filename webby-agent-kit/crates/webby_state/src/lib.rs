//! Persistent browser profile state for Webby.
//!
//! `webby_state` owns the on-disk user state format and file I/O for browser
//! history, bookmarks, recent pages, and startup configuration. Engine crates
//! do not depend on this crate.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use webby_core::{WebbyError, WebbyResult};

/// Default homepage used when no profile config exists.
pub const DEFAULT_HOMEPAGE: &str = "examples/simple.html";
const CONFIG_FILE: &str = "config.json";
const HISTORY_FILE: &str = "history.json";
const BOOKMARKS_FILE: &str = "bookmarks.json";
const RECENT_FILE: &str = "recent.json";
const COOKIES_FILE: &str = "cookies.json";
const LOCAL_STORAGE_FILE: &str = "local_storage.json";
const CACHE_DIR: &str = "cache";
const MAX_RECENT_PAGES: usize = 25;
/// Deterministic per-origin Web Storage quota in UTF-8 bytes.
pub const STORAGE_QUOTA_BYTES: usize = 4096;

/// Browser configuration persisted separately from navigation data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// Address/search input used at startup.
    pub homepage: String,
    /// Whether cookies are accepted and sent.
    pub cookies_enabled: bool,
    /// Whether persistent cookies are saved to disk.
    pub persist_cookies: bool,
    /// Whether cookies should be cleared when the app exits.
    pub clear_cookies_on_exit: bool,
    /// Whether persisted browsing data should be cleared when the app exits.
    #[serde(default)]
    pub clear_data_on_exit: bool,
    /// Whether inline JavaScript is executed by the page pipeline.
    pub javascript_enabled: bool,
    /// Whether Web Storage APIs are exposed to JavaScript.
    pub storage_enabled: bool,
    /// Whether deterministic visual transitions are enabled.
    #[serde(default = "default_true")]
    pub animations_enabled: bool,
    /// Whether the optional persistent resource byte cache is enabled.
    #[serde(default = "default_true")]
    pub disk_cache_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            homepage: DEFAULT_HOMEPAGE.to_string(),
            cookies_enabled: true,
            persist_cookies: false,
            clear_cookies_on_exit: false,
            clear_data_on_exit: false,
            javascript_enabled: true,
            storage_enabled: true,
            animations_enabled: true,
            disk_cache_enabled: true,
        }
    }
}

/// One persisted bookmark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    /// Bookmark URL.
    pub url: String,
}

/// Persisted successful navigation history.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HistoryState {
    /// Successful navigation URLs in commit order.
    pub entries: Vec<String>,
}

/// Persisted bookmark list.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BookmarkState {
    /// Bookmarks sorted by URL for deterministic output.
    pub bookmarks: Vec<Bookmark>,
}

/// Persisted recent pages, distinct from append-only history.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RecentState {
    /// Most recent successful page URLs first.
    pub pages: Vec<String>,
}

/// One cookie in Webby's deterministic cookie jar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Canonical domain without leading dot.
    pub domain: String,
    /// Whether the cookie is restricted to exactly `domain`.
    pub host_only: bool,
    /// Path prefix.
    pub path: String,
    /// Whether the cookie is only sent over HTTPS.
    pub secure: bool,
    /// Session cookies are kept in memory and not persisted.
    pub session: bool,
}

/// Browser cookie jar. Entries are sorted deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CookieJar {
    /// Stored cookies.
    pub cookies: Vec<Cookie>,
}

/// Persisted localStorage state, isolated by deterministic origin key.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StorageState {
    /// Origin-keyed storage entries. Origin and item keys are sorted by
    /// `BTreeMap`, keeping on-disk JSON and public snapshots deterministic.
    pub origins: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

/// Complete in-memory profile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrowserProfile {
    /// Startup/config state.
    pub config: BrowserConfig,
    /// Successful navigation history.
    pub history: HistoryState,
    /// Bookmarks.
    pub bookmarks: BookmarkState,
    /// Recently visited pages.
    pub recent: RecentState,
    /// Cookie jar, populated from persistent cookies plus session cookies.
    pub cookies: CookieJar,
    /// Persistent localStorage values.
    pub local_storage: StorageState,
}

impl BrowserProfile {
    /// Returns the configured homepage/start page.
    pub fn homepage(&self) -> &str {
        &self.config.homepage
    }

    /// Validates and returns the configured homepage as address bar input.
    pub fn validated_homepage(&self) -> WebbyResult<&str> {
        if self.config.homepage.trim().is_empty() {
            return Err(WebbyError::invalid_input("configured homepage is empty"));
        }
        Ok(self.homepage())
    }

    /// Records one successful navigation in history and recent pages.
    pub fn record_successful_navigation(&mut self, url: &url::Url) {
        let value = url.to_string();
        self.history.entries.push(value.clone());
        self.recent.pages.retain(|entry| entry != &value);
        self.recent.pages.insert(0, value);
        self.recent.pages.truncate(MAX_RECENT_PAGES);
    }

    /// Adds a bookmark. Duplicate URLs are ignored deterministically.
    pub fn add_bookmark(&mut self, url: &url::Url) {
        let value = url.to_string();
        if self
            .bookmarks
            .bookmarks
            .iter()
            .any(|bookmark| bookmark.url == value)
        {
            return;
        }
        self.bookmarks.bookmarks.push(Bookmark { url: value });
        self.bookmarks
            .bookmarks
            .sort_by(|left, right| left.url.cmp(&right.url));
    }

    /// Removes a bookmark and reports whether one existed.
    pub fn remove_bookmark(&mut self, url: &url::Url) -> bool {
        let value = url.to_string();
        let before = self.bookmarks.bookmarks.len();
        self.bookmarks
            .bookmarks
            .retain(|bookmark| bookmark.url != value);
        self.bookmarks.bookmarks.len() != before
    }

    /// Lists bookmarks in deterministic order.
    pub fn list_bookmarks(&self) -> &[Bookmark] {
        &self.bookmarks.bookmarks
    }

    /// Clears all cookies from the in-memory profile.
    pub fn clear_cookies(&mut self) {
        self.cookies.clear();
    }

    /// Clears persistent navigation history and recent pages.
    pub fn clear_history(&mut self) {
        self.history.entries.clear();
        self.recent.pages.clear();
    }

    /// Clears all persisted bookmarks.
    pub fn clear_bookmarks(&mut self) {
        self.bookmarks.bookmarks.clear();
    }

    /// Clears all persisted localStorage values.
    pub fn clear_local_storage(&mut self) {
        self.local_storage.clear();
    }
}

impl StorageState {
    /// Returns a value for an origin/key pair.
    pub fn get_item(&self, origin: &str, key: &str) -> Option<&str> {
        self.origins
            .get(origin)
            .and_then(|items| items.get(key))
            .map(String::as_str)
    }

    /// Stores a string value while enforcing the deterministic per-origin quota.
    pub fn set_item(&mut self, origin: &str, key: &str, value: &str) -> WebbyResult<()> {
        let mut next = self.origins.get(origin).cloned().unwrap_or_default();
        next.insert(key.to_string(), value.to_string());
        let byte_len = storage_byte_len(&next);
        if byte_len > STORAGE_QUOTA_BYTES {
            return Err(WebbyError::unsupported(format!(
                "localStorage quota exceeded for {origin}: {byte_len}/{STORAGE_QUOTA_BYTES} bytes"
            )));
        }
        self.origins.insert(origin.to_string(), next);
        Ok(())
    }

    /// Removes a key from an origin.
    pub fn remove_item(&mut self, origin: &str, key: &str) {
        if let Some(items) = self.origins.get_mut(origin) {
            items.remove(key);
            if items.is_empty() {
                self.origins.remove(origin);
            }
        }
    }

    /// Clears all keys for an origin.
    pub fn clear_origin(&mut self, origin: &str) {
        self.origins.remove(origin);
    }

    /// Clears all persisted origins and keys.
    pub fn clear(&mut self) {
        self.origins.clear();
    }

    /// Returns all key/value pairs for an origin in deterministic key order.
    pub fn entries_for_origin(&self, origin: &str) -> Vec<StorageEntry> {
        self.origins
            .get(origin)
            .map(|items| {
                items
                    .iter()
                    .map(|(key, value)| StorageEntry {
                        key: key.clone(),
                        value: value.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Replaces one origin with already validated entries.
    pub fn replace_origin_entries(
        &mut self,
        origin: &str,
        entries: &[StorageEntry],
    ) -> WebbyResult<()> {
        let mut items = std::collections::BTreeMap::new();
        for entry in entries {
            items.insert(entry.key.clone(), entry.value.clone());
        }
        let byte_len = storage_byte_len(&items);
        if byte_len > STORAGE_QUOTA_BYTES {
            return Err(WebbyError::unsupported(format!(
                "localStorage quota exceeded for {origin}: {byte_len}/{STORAGE_QUOTA_BYTES} bytes"
            )));
        }
        if items.is_empty() {
            self.origins.remove(origin);
        } else {
            self.origins.insert(origin.to_string(), items);
        }
        Ok(())
    }
}

/// One Web Storage key/value pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageEntry {
    /// Storage key.
    pub key: String,
    /// Storage value.
    pub value: String,
}

/// Returns Webby's deterministic Web Storage origin key.
///
/// HTTP(S) keys are `scheme://host:port`, using known default ports when the
/// URL omits one. `file://` pages share `file://local` because Webby has no
/// per-directory or opaque file-origin model yet.
pub fn storage_origin_key(url: &url::Url) -> WebbyResult<String> {
    webby_security::origin_key(url)
}

fn storage_byte_len(items: &std::collections::BTreeMap<String, String>) -> usize {
    items
        .iter()
        .map(|(key, value)| key.len() + value.len())
        .sum()
}

impl CookieJar {
    /// Clears all cookies.
    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    /// Stores cookies from response headers for the final response URL.
    pub fn store_from_headers(
        &mut self,
        final_url: &url::Url,
        headers: &[(String, String)],
    ) -> Vec<String> {
        let mut diagnostics = Vec::new();
        for (_, value) in headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        {
            match parse_set_cookie(value, final_url) {
                Some(CookieAction::Delete(cookie)) => {
                    self.remove_cookie(&cookie);
                    diagnostics.push(format!("cookie delete {}", cookie_key(&cookie)));
                }
                Some(CookieAction::Store(cookie)) => {
                    self.upsert(cookie.clone());
                    diagnostics.push(format!("cookie store {}", cookie_key(&cookie)));
                }
                None => diagnostics.push("cookie ignored invalid Set-Cookie".to_string()),
            }
        }
        diagnostics
    }

    /// Returns request headers for cookies matching the URL.
    pub fn request_headers(&self, url: &url::Url) -> Vec<(String, String)> {
        let value = self.cookie_header_value(url);
        if value.is_empty() {
            Vec::new()
        } else {
            vec![("Cookie".to_string(), value)]
        }
    }

    /// Returns the deterministic Cookie header value for a URL.
    pub fn cookie_header_value(&self, url: &url::Url) -> String {
        let mut cookies = self
            .cookies
            .iter()
            .filter(|cookie| cookie_matches(cookie, url))
            .collect::<Vec<_>>();
        cookies.sort_by(|left, right| {
            right
                .path
                .len()
                .cmp(&left.path.len())
                .then_with(|| left.name.cmp(&right.name))
        });
        cookies
            .iter()
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Returns only cookies that should be persisted.
    pub fn persistent_only(&self) -> Self {
        Self {
            cookies: self
                .cookies
                .iter()
                .filter(|cookie| !cookie.session)
                .cloned()
                .collect(),
        }
    }

    fn upsert(&mut self, cookie: Cookie) {
        self.remove_cookie(&cookie);
        self.cookies.push(cookie);
        self.cookies.sort_by(|left, right| {
            left.domain
                .cmp(&right.domain)
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.name.cmp(&right.name))
        });
    }

    fn remove_cookie(&mut self, cookie: &Cookie) {
        self.cookies.retain(|existing| {
            existing.name != cookie.name
                || existing.domain != cookie.domain
                || existing.path != cookie.path
                || existing.host_only != cookie.host_only
        });
    }
}

/// Filesystem-backed profile store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStore {
    root: PathBuf,
}

impl ProfileStore {
    /// Creates a store rooted at an explicit directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the documented default user data directory.
    ///
    /// `WEBBY_PROFILE_DIR` overrides all defaults. Otherwise Webby uses
    /// `$XDG_DATA_HOME/webby`, then `$HOME/.local/share/webby`.
    pub fn default_user_data_dir() -> WebbyResult<PathBuf> {
        if let Some(path) = std::env::var_os("WEBBY_PROFILE_DIR") {
            return Ok(PathBuf::from(path));
        }
        if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(path).join("webby"));
        }
        if let Some(path) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(path)
                .join(".local")
                .join("share")
                .join("webby"));
        }
        Err(WebbyError::invalid_input(
            "could not determine user data directory; set WEBBY_PROFILE_DIR",
        ))
    }

    /// Creates a store using the documented default user data directory.
    pub fn default_user() -> WebbyResult<Self> {
        Ok(Self::new(Self::default_user_data_dir()?))
    }

    /// Returns the profile root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the persistent resource cache directory for this profile.
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join(CACHE_DIR)
    }

    /// Loads a complete profile, using defaults for missing files.
    pub fn load(&self) -> WebbyResult<BrowserProfile> {
        Ok(BrowserProfile {
            config: read_json_or_default(&self.path(CONFIG_FILE))?,
            history: read_json_or_default(&self.path(HISTORY_FILE))?,
            bookmarks: read_json_or_default(&self.path(BOOKMARKS_FILE))?,
            recent: read_json_or_default(&self.path(RECENT_FILE))?,
            cookies: read_json_or_default(&self.path(COOKIES_FILE))?,
            local_storage: read_json_or_default(&self.path(LOCAL_STORAGE_FILE))?,
        })
    }

    /// Saves a complete profile in deterministic pretty JSON.
    pub fn save(&self, profile: &BrowserProfile) -> WebbyResult<()> {
        std::fs::create_dir_all(&self.root).map_err(|source| WebbyError::Io {
            path: Some(self.root.clone()),
            source,
        })?;
        write_json(&self.path(CONFIG_FILE), &profile.config)?;
        write_json(&self.path(HISTORY_FILE), &profile.history)?;
        write_json(&self.path(BOOKMARKS_FILE), &profile.bookmarks)?;
        write_json(&self.path(RECENT_FILE), &profile.recent)?;
        if profile.config.persist_cookies {
            write_json(&self.path(COOKIES_FILE), &profile.cookies.persistent_only())?;
        } else {
            remove_file_if_exists(self.path(COOKIES_FILE))?;
        }
        write_json(&self.path(LOCAL_STORAGE_FILE), &profile.local_storage)?;
        Ok(())
    }

    /// Records a successful navigation and persists the profile.
    pub fn record_successful_navigation(
        &self,
        profile: &mut BrowserProfile,
        url: &url::Url,
    ) -> WebbyResult<()> {
        profile.record_successful_navigation(url);
        self.save(profile)
    }

    /// Adds a bookmark and persists the profile.
    pub fn add_bookmark(&self, profile: &mut BrowserProfile, url: &url::Url) -> WebbyResult<()> {
        profile.add_bookmark(url);
        self.save(profile)
    }

    /// Removes a bookmark and persists the profile.
    pub fn remove_bookmark(
        &self,
        profile: &mut BrowserProfile,
        url: &url::Url,
    ) -> WebbyResult<bool> {
        let removed = profile.remove_bookmark(url);
        self.save(profile)?;
        Ok(removed)
    }

    /// Clears persisted and in-memory cookies.
    pub fn clear_cookies(&self, profile: &mut BrowserProfile) -> WebbyResult<()> {
        profile.clear_cookies();
        remove_file_if_exists(self.path(COOKIES_FILE))?;
        self.save(profile)
    }

    /// Clears persisted history and recent pages.
    pub fn clear_history(&self, profile: &mut BrowserProfile) -> WebbyResult<()> {
        profile.clear_history();
        remove_file_if_exists(self.path(HISTORY_FILE))?;
        remove_file_if_exists(self.path(RECENT_FILE))?;
        self.save(profile)
    }

    /// Clears persisted bookmarks.
    pub fn clear_bookmarks(&self, profile: &mut BrowserProfile) -> WebbyResult<()> {
        profile.clear_bookmarks();
        remove_file_if_exists(self.path(BOOKMARKS_FILE))?;
        self.save(profile)
    }

    /// Clears persisted localStorage.
    pub fn clear_local_storage(&self, profile: &mut BrowserProfile) -> WebbyResult<()> {
        profile.clear_local_storage();
        remove_file_if_exists(self.path(LOCAL_STORAGE_FILE))?;
        self.save(profile)
    }

    /// Clears all profile data controlled by Milestone 66 privacy settings.
    pub fn clear_browsing_data(&self, profile: &mut BrowserProfile) -> WebbyResult<()> {
        profile.clear_history();
        profile.clear_bookmarks();
        profile.clear_cookies();
        profile.clear_local_storage();
        remove_file_if_exists(self.path(HISTORY_FILE))?;
        remove_file_if_exists(self.path(RECENT_FILE))?;
        remove_file_if_exists(self.path(BOOKMARKS_FILE))?;
        remove_file_if_exists(self.path(COOKIES_FILE))?;
        remove_file_if_exists(self.path(LOCAL_STORAGE_FILE))?;
        if self.cache_dir().exists() {
            std::fs::remove_dir_all(self.cache_dir()).map_err(|source| WebbyError::Io {
                path: Some(self.cache_dir()),
                source,
            })?;
        }
        self.save(profile)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

fn read_json_or_default<T>(path: &Path) -> WebbyResult<T>
where
    T: for<'de> Deserialize<'de> + Default,
{
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| WebbyError::Parse {
            message: format!("corrupt profile file {}: {error}", path.display()),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(source) => Err(WebbyError::Io {
            path: Some(path.to_path_buf()),
            source,
        }),
    }
}

fn write_json<T>(path: &Path, value: &T) -> WebbyResult<()>
where
    T: Serialize,
{
    let mut text = serde_json::to_string_pretty(value).map_err(|error| WebbyError::Parse {
        message: format!(
            "could not serialize profile state {}: {error}",
            path.display()
        ),
    })?;
    text.push('\n');
    std::fs::write(path, text).map_err(|source| WebbyError::Io {
        path: Some(path.to_path_buf()),
        source,
    })
}

fn remove_file_if_exists(path: PathBuf) -> WebbyResult<()> {
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(WebbyError::Io {
            path: Some(path),
            source,
        }),
    }
}

enum CookieAction {
    Store(Cookie),
    Delete(Cookie),
}

fn parse_set_cookie(header: &str, url: &url::Url) -> Option<CookieAction> {
    let mut parts = header.split(';');
    let first = parts.next()?.trim();
    let (name, value) = first.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || name.chars().any(|character| character.is_control()) {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let mut cookie = Cookie {
        name: name.to_string(),
        value: value.trim().to_string(),
        domain: host,
        host_only: true,
        path: default_cookie_path(url),
        secure: false,
        session: true,
    };
    let mut delete = false;

    for part in parts {
        let attribute = part.trim();
        let (key, value) = attribute.split_once('=').unwrap_or((attribute, ""));
        match key.trim().to_ascii_lowercase().as_str() {
            "domain" => {
                let domain = value.trim().trim_start_matches('.').to_ascii_lowercase();
                if domain.is_empty() || !domain_matches(&url_host(url)?, &domain) {
                    return None;
                }
                cookie.domain = domain;
                cookie.host_only = false;
            }
            "path" => {
                let path = value.trim();
                if path.starts_with('/') {
                    cookie.path = path.to_string();
                }
            }
            "secure" => cookie.secure = true,
            "max-age" => {
                let seconds = value.trim().parse::<i64>().ok()?;
                if seconds <= 0 {
                    delete = true;
                } else {
                    cookie.session = false;
                }
            }
            "expires" => cookie.session = false,
            _ => {}
        }
    }

    Some(if delete {
        CookieAction::Delete(cookie)
    } else {
        CookieAction::Store(cookie)
    })
}

fn cookie_matches(cookie: &Cookie, url: &url::Url) -> bool {
    if cookie.secure && url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    if cookie.host_only {
        if host != cookie.domain {
            return false;
        }
    } else if !domain_matches(&host, &cookie.domain) {
        return false;
    }
    cookie_path_matches(&cookie.path, request_path(url))
}

fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn cookie_path_matches(cookie_path: &str, request_path: &str) -> bool {
    if cookie_path == "/" || request_path == cookie_path {
        return true;
    }
    cookie_path.ends_with('/')
        || request_path
            .strip_prefix(cookie_path)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn default_cookie_path(url: &url::Url) -> String {
    let path = request_path(url);
    let Some(index) = path.rfind('/') else {
        return "/".to_string();
    };
    if index == 0 {
        "/".to_string()
    } else {
        path[..index].to_string()
    }
}

fn request_path(url: &url::Url) -> &str {
    let path = url.path();
    if path.is_empty() { "/" } else { path }
}

fn url_host(url: &url::Url) -> Option<String> {
    url.host_str().map(str::to_ascii_lowercase)
}

fn cookie_key(cookie: &Cookie) -> String {
    format!("{}:{}:{}", cookie.domain, cookie.path, cookie.name)
}

#[cfg(test)]
mod tests {
    use super::{
        BookmarkState, BrowserConfig, BrowserProfile, CookieJar, HistoryState, ProfileStore,
        StorageState,
    };
    use webby_core::{WebbyError, WebbyResult};

    #[test]
    fn missing_config_uses_defaults() -> WebbyResult<()> {
        let store = temp_store("missing-config");

        let profile = store.load()?;

        assert_eq!(profile.config.homepage, super::DEFAULT_HOMEPAGE);
        assert!(profile.history.entries.is_empty());
        Ok(())
    }

    #[test]
    fn corrupt_config_returns_structured_error() -> WebbyResult<()> {
        let store = temp_store("corrupt-config");
        write_file(store.root().join("config.json"), "{not json")?;

        let result = store.load();

        assert!(
            matches!(result, Err(WebbyError::Parse { message }) if message.contains("config.json"))
        );
        Ok(())
    }

    #[test]
    fn history_persists_successful_navigations() -> WebbyResult<()> {
        let store = temp_store("history");
        let mut profile = BrowserProfile::default();
        let url = parse_url("https://example.test/")?;

        store.record_successful_navigation(&mut profile, &url)?;
        let loaded = store.load()?;

        assert_eq!(loaded.history.entries, vec!["https://example.test/"]);
        assert_eq!(loaded.recent.pages, vec!["https://example.test/"]);
        Ok(())
    }

    #[test]
    fn bookmarks_can_be_added_listed_and_removed() -> WebbyResult<()> {
        let store = temp_store("bookmarks");
        let mut profile = BrowserProfile::default();
        let first = parse_url("https://b.example/")?;
        let second = parse_url("https://a.example/")?;

        store.add_bookmark(&mut profile, &first)?;
        store.add_bookmark(&mut profile, &second)?;
        store.add_bookmark(&mut profile, &first)?;

        let loaded = store.load()?;
        assert_eq!(
            loaded
                .list_bookmarks()
                .iter()
                .map(|bookmark| bookmark.url.as_str())
                .collect::<Vec<_>>(),
            vec!["https://a.example/", "https://b.example/"]
        );

        let removed = store.remove_bookmark(&mut profile, &second)?;
        assert!(removed);
        assert_eq!(store.load()?.list_bookmarks().len(), 1);
        Ok(())
    }

    #[test]
    fn persistence_format_is_deterministic() -> WebbyResult<()> {
        let store = temp_store("format");
        let profile = BrowserProfile {
            config: BrowserConfig {
                homepage: "https://example.test/".to_string(),
                cookies_enabled: true,
                persist_cookies: false,
                clear_cookies_on_exit: false,
                clear_data_on_exit: false,
                javascript_enabled: true,
                storage_enabled: true,
                animations_enabled: true,
                disk_cache_enabled: true,
            },
            history: HistoryState {
                entries: vec!["https://example.test/".to_string()],
            },
            bookmarks: BookmarkState {
                bookmarks: vec![super::Bookmark {
                    url: "https://example.test/".to_string(),
                }],
            },
            recent: super::RecentState {
                pages: vec!["https://example.test/".to_string()],
            },
            cookies: CookieJar::default(),
            local_storage: StorageState::default(),
        };

        store.save(&profile)?;
        let first = read_file(store.root().join("config.json"))?;
        store.save(&profile)?;
        let second = read_file(store.root().join("config.json"))?;

        assert_eq!(first, second);
        assert_eq!(
            first,
            "{\n  \"homepage\": \"https://example.test/\",\n  \"cookies_enabled\": true,\n  \"persist_cookies\": false,\n  \"clear_cookies_on_exit\": false,\n  \"clear_data_on_exit\": false,\n  \"javascript_enabled\": true,\n  \"storage_enabled\": true,\n  \"animations_enabled\": true,\n  \"disk_cache_enabled\": true\n}\n"
        );
        Ok(())
    }

    #[test]
    fn invalid_homepage_is_reported_safely() {
        let profile = BrowserProfile {
            config: BrowserConfig {
                homepage: "   ".to_string(),
                ..BrowserConfig::default()
            },
            ..BrowserProfile::default()
        };

        assert!(matches!(
            profile.validated_homepage(),
            Err(WebbyError::InvalidInput { .. })
        ));
    }

    #[test]
    fn cookie_jar_parses_set_cookie_and_sends_matching_header() -> WebbyResult<()> {
        let page_url = parse_url("https://example.test/account/index.html")?;
        let mut jar = CookieJar::default();

        let diagnostics = jar.store_from_headers(
            &page_url,
            &cookie_headers("sid=abc; Path=/account; Max-Age=60"),
        );
        let same_path = parse_url("https://example.test/account/settings")?;
        let other_path = parse_url("https://example.test/other")?;

        assert_eq!(diagnostics, vec!["cookie store example.test:/account:sid"]);
        assert_eq!(jar.cookie_header_value(&same_path), "sid=abc");
        assert_eq!(jar.cookie_header_value(&other_path), "");
        Ok(())
    }

    #[test]
    fn cookie_domain_matching_distinguishes_host_only_and_domain_cookies() -> WebbyResult<()> {
        let page_url = parse_url("https://example.test/")?;
        let mut jar = CookieJar::default();
        jar.store_from_headers(&page_url, &cookie_headers("host=1; Path=/"));
        jar.store_from_headers(
            &page_url,
            &cookie_headers("domain=1; Domain=example.test; Path=/"),
        );

        let subdomain = parse_url("https://sub.example.test/")?;

        assert_eq!(jar.cookie_header_value(&subdomain), "domain=1");
        Ok(())
    }

    #[test]
    fn invalid_cookie_domain_is_ignored_without_panicking() -> WebbyResult<()> {
        let page_url = parse_url("https://example.test/")?;
        let mut jar = CookieJar::default();
        let diagnostics =
            jar.store_from_headers(&page_url, &cookie_headers("sid=abc; Domain=other.test"));

        assert_eq!(diagnostics, vec!["cookie ignored invalid Set-Cookie"]);
        assert!(jar.cookies.is_empty());
        Ok(())
    }

    #[test]
    fn invalid_cookie_diagnostics_do_not_leak_cookie_values() -> WebbyResult<()> {
        let page_url = parse_url("https://example.test/")?;
        let mut jar = CookieJar::default();

        let diagnostics = jar.store_from_headers(
            &page_url,
            &cookie_headers("session=top-secret; Domain=other.test"),
        );

        assert_eq!(diagnostics, vec!["cookie ignored invalid Set-Cookie"]);
        assert!(!diagnostics.join("\n").contains("top-secret"));
        assert!(!diagnostics.join("\n").contains("session="));
        Ok(())
    }

    #[test]
    fn cookie_path_matching_respects_segment_boundaries() -> WebbyResult<()> {
        let page_url = parse_url("https://example.test/foo/index.html")?;
        let mut jar = CookieJar::default();

        jar.store_from_headers(&page_url, &cookie_headers("scoped=1; Path=/foo"));

        assert_eq!(
            jar.cookie_header_value(&parse_url("https://example.test/foo")?),
            "scoped=1"
        );
        assert_eq!(
            jar.cookie_header_value(&parse_url("https://example.test/foo/bar")?),
            "scoped=1"
        );
        assert_eq!(
            jar.cookie_header_value(&parse_url("https://example.test/foobar")?),
            ""
        );
        Ok(())
    }

    #[test]
    fn empty_cookie_value_is_stored_unless_max_age_deletes_it() -> WebbyResult<()> {
        let page_url = parse_url("https://example.test/")?;
        let mut jar = CookieJar::default();

        let diagnostics = jar.store_from_headers(&page_url, &cookie_headers("empty=; Path=/"));

        assert_eq!(diagnostics, vec!["cookie store example.test:/:empty"]);
        assert_eq!(jar.cookie_header_value(&page_url), "empty=");

        let diagnostics =
            jar.store_from_headers(&page_url, &cookie_headers("empty=gone; Path=/; Max-Age=0"));

        assert_eq!(diagnostics, vec!["cookie delete example.test:/:empty"]);
        assert_eq!(jar.cookie_header_value(&page_url), "");
        Ok(())
    }

    #[test]
    fn persistent_cookie_storage_respects_config_and_skips_session_cookies() -> WebbyResult<()> {
        let store = temp_store("cookies");
        let page_url = parse_url("https://example.test/")?;
        let mut profile = BrowserProfile {
            config: BrowserConfig {
                persist_cookies: true,
                ..BrowserConfig::default()
            },
            ..BrowserProfile::default()
        };
        profile
            .cookies
            .store_from_headers(&page_url, &cookie_headers("sid=abc; Path=/"));
        profile.cookies.store_from_headers(
            &page_url,
            &cookie_headers("persist=yes; Path=/; Max-Age=60"),
        );

        store.save(&profile)?;
        let loaded = store.load()?;

        assert_eq!(loaded.cookies.cookies.len(), 1);
        assert_eq!(loaded.cookies.cookies[0].name, "persist");
        Ok(())
    }

    #[test]
    fn disabling_persistent_cookies_removes_stale_cookie_file() -> WebbyResult<()> {
        let store = temp_store("disable-cookies");
        let page_url = parse_url("https://example.test/")?;
        let mut profile = BrowserProfile {
            config: BrowserConfig {
                persist_cookies: true,
                ..BrowserConfig::default()
            },
            ..BrowserProfile::default()
        };
        profile.cookies.store_from_headers(
            &page_url,
            &cookie_headers("persist=yes; Path=/; Max-Age=60"),
        );
        store.save(&profile)?;

        profile.config.persist_cookies = false;
        store.save(&profile)?;

        assert!(store.load()?.cookies.cookies.is_empty());
        Ok(())
    }

    #[test]
    fn clear_cookies_removes_persisted_cookie_file() -> WebbyResult<()> {
        let store = temp_store("clear-cookies");
        let page_url = parse_url("https://example.test/")?;
        let mut profile = BrowserProfile {
            config: BrowserConfig {
                persist_cookies: true,
                ..BrowserConfig::default()
            },
            ..BrowserProfile::default()
        };
        profile.cookies.store_from_headers(
            &page_url,
            &cookie_headers("persist=yes; Path=/; Max-Age=60"),
        );
        store.save(&profile)?;

        store.clear_cookies(&mut profile)?;

        assert!(store.load()?.cookies.cookies.is_empty());
        Ok(())
    }

    #[test]
    fn storage_origin_keys_are_deterministic() -> WebbyResult<()> {
        assert_eq!(
            super::storage_origin_key(&parse_url("https://example.test/path")?)?,
            "https://example.test:443"
        );
        assert_eq!(
            super::storage_origin_key(&parse_url("http://example.test:8080/path")?)?,
            "http://example.test:8080"
        );
        assert_eq!(
            super::storage_origin_key(
                &url::Url::from_file_path("/tmp/webby.html")
                    .map_err(|_| WebbyError::invalid_input("could not create file URL"))?
            )?,
            "file://local"
        );
        assert_eq!(
            super::storage_origin_key(&parse_url("https://example.test/path")?)?,
            webby_security::origin_key(&parse_url("https://example.test/other")?)?
        );
        Ok(())
    }

    #[test]
    fn local_storage_persists_across_profile_reload() -> WebbyResult<()> {
        let store = temp_store("local-storage-persist");
        let origin = "https://example.test:443";
        let mut profile = BrowserProfile::default();
        profile.local_storage.set_item(origin, "theme", "dark")?;

        store.save(&profile)?;
        let loaded = store.load()?;

        assert_eq!(loaded.local_storage.get_item(origin, "theme"), Some("dark"));
        Ok(())
    }

    #[test]
    fn local_storage_is_isolated_by_origin() -> WebbyResult<()> {
        let mut storage = StorageState::default();

        storage.set_item("https://a.example:443", "key", "a")?;
        storage.set_item("https://b.example:443", "key", "b")?;

        assert_eq!(storage.get_item("https://a.example:443", "key"), Some("a"));
        assert_eq!(storage.get_item("https://b.example:443", "key"), Some("b"));
        Ok(())
    }

    #[test]
    fn local_storage_quota_is_deterministic() {
        let mut storage = StorageState::default();
        let oversized = "x".repeat(super::STORAGE_QUOTA_BYTES + 1);

        let result = storage.set_item("https://example.test:443", "key", &oversized);

        assert!(
            matches!(result, Err(WebbyError::Unsupported { message }) if message.contains("quota"))
        );
        assert_eq!(storage.get_item("https://example.test:443", "key"), None);
    }

    #[test]
    fn corrupt_local_storage_returns_structured_error() -> WebbyResult<()> {
        let store = temp_store("corrupt-local-storage");
        write_file(store.root().join("local_storage.json"), "{not json")?;

        let result = store.load();

        assert!(
            matches!(result, Err(WebbyError::Parse { message }) if message.contains("local_storage.json"))
        );
        Ok(())
    }

    #[test]
    fn clear_helpers_remove_persisted_history_bookmarks_and_local_storage() -> WebbyResult<()> {
        let store = temp_store("clear-profile-parts");
        let url = parse_url("https://example.test/")?;
        let mut profile = BrowserProfile::default();
        store.record_successful_navigation(&mut profile, &url)?;
        store.add_bookmark(&mut profile, &url)?;
        profile
            .local_storage
            .set_item("https://example.test:443", "theme", "dark")?;
        store.save(&profile)?;

        store.clear_history(&mut profile)?;
        assert!(profile.history.entries.is_empty());
        assert!(profile.recent.pages.is_empty());
        assert!(store.load()?.history.entries.is_empty());

        store.clear_bookmarks(&mut profile)?;
        assert!(profile.bookmarks.bookmarks.is_empty());
        assert!(store.load()?.bookmarks.bookmarks.is_empty());

        store.clear_local_storage(&mut profile)?;
        assert!(profile.local_storage.origins.is_empty());
        assert!(store.load()?.local_storage.origins.is_empty());
        Ok(())
    }

    #[test]
    fn clear_browsing_data_removes_profile_data_and_cache_directory() -> WebbyResult<()> {
        let store = temp_store("clear-all-data");
        let url = parse_url("https://example.test/")?;
        let mut profile = BrowserProfile {
            config: BrowserConfig {
                persist_cookies: true,
                ..BrowserConfig::default()
            },
            ..BrowserProfile::default()
        };
        store.record_successful_navigation(&mut profile, &url)?;
        store.add_bookmark(&mut profile, &url)?;
        profile
            .cookies
            .store_from_headers(&url, &cookie_headers("sid=abc; Path=/; Max-Age=60"));
        profile
            .local_storage
            .set_item("https://example.test:443", "theme", "dark")?;
        write_file(store.cache_dir().join("body-1.bin"), "cached")?;
        store.save(&profile)?;

        store.clear_browsing_data(&mut profile)?;
        let loaded = store.load()?;

        assert!(loaded.history.entries.is_empty());
        assert!(loaded.recent.pages.is_empty());
        assert!(loaded.bookmarks.bookmarks.is_empty());
        assert!(loaded.cookies.cookies.is_empty());
        assert!(loaded.local_storage.origins.is_empty());
        assert!(!store.cache_dir().exists());
        Ok(())
    }

    #[test]
    fn parser_layout_render_crates_do_not_depend_on_persistence() {
        let manifests = [
            include_str!("../../webby_html/Cargo.toml"),
            include_str!("../../webby_layout/Cargo.toml"),
            include_str!("../../webby_render/Cargo.toml"),
            include_str!("../../webby_css/Cargo.toml"),
            include_str!("../../webby_style/Cargo.toml"),
        ];

        for manifest in manifests {
            assert!(!manifest.contains("webby_state"));
        }
    }

    fn temp_store(name: &str) -> ProfileStore {
        let root = std::env::temp_dir().join(format!("webby-state-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        ProfileStore::new(root)
    }

    fn parse_url(value: &str) -> WebbyResult<url::Url> {
        url::Url::parse(value).map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })
    }

    fn cookie_headers(set_cookie: &str) -> Vec<(String, String)> {
        vec![("set-cookie".to_string(), set_cookie.to_string())]
    }

    fn write_file(path: std::path::PathBuf, text: &str) -> WebbyResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| WebbyError::Io {
                path: Some(parent.to_path_buf()),
                source,
            })?;
        }
        std::fs::write(&path, text).map_err(|source| WebbyError::Io {
            path: Some(path),
            source,
        })
    }

    fn read_file(path: std::path::PathBuf) -> WebbyResult<String> {
        std::fs::read_to_string(&path).map_err(|source| WebbyError::Io {
            path: Some(path),
            source,
        })
    }
}
