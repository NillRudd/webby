//! Deterministic resource byte cache for page, stylesheet, and image loads.
//!
//! The cache stores successful `webby_net::ResourceResponse` bytes keyed by
//! the requested URL string. Transport remains in `webby_net`; this crate owns
//! only storage, limits, policy, and cache diagnostics.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use webby_core::WebbyError;
use webby_core::WebbyResult;
use webby_net::{ResourceLoader, ResourceResponse};

const INDEX_FILE: &str = "index.json";

/// Cache lookup mode for one resource request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Use a cached response when available, otherwise load and store.
    Use,
    /// Bypass lookup, load from the underlying loader, and refresh on success.
    Refresh,
}

/// Deterministic cache policy limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLimits {
    /// Maximum stored entries.
    pub max_entries: usize,
    /// Maximum total stored response body bytes.
    pub max_bytes: usize,
}

impl CacheLimits {
    /// Creates explicit cache limits.
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            max_entries,
            max_bytes,
        }
    }
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            max_entries: 64,
            max_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Disk-backed cache tier with deterministic JSON metadata and body files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskResourceCache {
    root: PathBuf,
    limits: CacheLimits,
    inner: RefCell<DiskCacheInner>,
    diagnostics: RefCell<Vec<CacheDiagnostic>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
struct DiskCacheInner {
    entries: BTreeMap<String, DiskCacheEntry>,
    access_sequence: u64,
    store_sequence: u64,
    total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DiskCacheEntry {
    requested_url: String,
    final_url: String,
    status: Option<u16>,
    content_type: Option<String>,
    headers: Vec<(String, String)>,
    byte_len: usize,
    body_file: String,
    stored_at: u64,
    last_used: u64,
}

impl DiskResourceCache {
    /// Opens or creates a disk cache rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> WebbyResult<Self> {
        Self::with_limits(root, CacheLimits::default())
    }

    /// Opens a disk cache with explicit deterministic limits.
    pub fn with_limits(root: impl Into<PathBuf>, limits: CacheLimits) -> WebbyResult<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(|source| WebbyError::Io {
            path: Some(root.clone()),
            source,
        })?;
        let mut diagnostics = Vec::new();
        let inner = match read_disk_index(&root) {
            Ok(inner) => inner,
            Err(error) => {
                remove_disk_cache_contents(&root)?;
                diagnostics.push(diagnostic("disk-corrupt", INDEX_FILE, &error.to_string()));
                DiskCacheInner::default()
            }
        };
        Ok(Self {
            root,
            limits,
            inner: RefCell::new(inner),
            diagnostics: RefCell::new(diagnostics),
        })
    }

    /// Clears all persistent entries while keeping the cache directory usable.
    pub fn clear(&self) -> WebbyResult<()> {
        remove_disk_cache_contents(&self.root)?;
        *self.inner.borrow_mut() = DiskCacheInner::default();
        Ok(())
    }

    /// Returns and clears disk-tier diagnostics.
    pub fn take_diagnostics(&self) -> Vec<CacheDiagnostic> {
        std::mem::take(&mut *self.diagnostics.borrow_mut())
    }

    fn load(&self, url: &url::Url) -> WebbyResult<Option<ResourceResponse>> {
        let key = cache_key(url);
        let entry = self.inner.borrow().entries.get(&key).cloned();
        let Some(entry) = entry else {
            self.diagnostics
                .borrow_mut()
                .push(diagnostic("disk-miss", &key, ""));
            return Ok(None);
        };
        let body_path = self.root.join(&entry.body_file);
        let bytes = match std::fs::read(&body_path) {
            Ok(bytes) if bytes.len() == entry.byte_len => bytes,
            Ok(bytes) => {
                self.remove_corrupt_entry(
                    &key,
                    &format!("expected {} bytes, read {}", entry.byte_len, bytes.len()),
                )?;
                return Ok(None);
            }
            Err(error) => {
                self.remove_corrupt_entry(&key, &error.to_string())?;
                return Ok(None);
            }
        };
        let requested_url = match parse_stored_url(&entry.requested_url) {
            Ok(url) => url,
            Err(error) => {
                self.remove_corrupt_entry(&key, &error.to_string())?;
                return Ok(None);
            }
        };
        let final_url = match parse_stored_url(&entry.final_url) {
            Ok(url) => url,
            Err(error) => {
                self.remove_corrupt_entry(&key, &error.to_string())?;
                return Ok(None);
            }
        };
        {
            let mut inner = self.inner.borrow_mut();
            inner.access_sequence = inner.access_sequence.saturating_add(1);
            let used_at = inner.access_sequence;
            if let Some(stored) = inner.entries.get_mut(&key) {
                stored.last_used = used_at;
            }
        }
        self.persist_index()?;
        self.diagnostics
            .borrow_mut()
            .push(diagnostic("disk-hit", &key, ""));
        Ok(Some(ResourceResponse {
            requested_url,
            final_url,
            status: entry.status,
            content_type: entry.content_type,
            headers: entry.headers,
            bytes,
        }))
    }

    fn store(&self, response: &ResourceResponse) -> WebbyResult<()> {
        let key = cache_key(&response.requested_url);
        let byte_len = response.bytes.len();
        if self.limits.max_entries == 0 || byte_len > self.limits.max_bytes {
            self.diagnostics.borrow_mut().push(diagnostic(
                "disk-skip-store",
                &key,
                &format!("bytes={byte_len} exceeds cache limits"),
            ));
            return Ok(());
        }
        let previous = self.inner.borrow().entries.get(&key).cloned();
        let (body_file, stored_at, last_used) = {
            let mut inner = self.inner.borrow_mut();
            inner.store_sequence = inner.store_sequence.saturating_add(1);
            inner.access_sequence = inner.access_sequence.saturating_add(1);
            (
                format!("body-{:020}.bin", inner.store_sequence),
                inner.store_sequence,
                inner.access_sequence,
            )
        };
        let body_path = self.root.join(&body_file);
        std::fs::write(&body_path, &response.bytes).map_err(|source| WebbyError::Io {
            path: Some(body_path.clone()),
            source,
        })?;
        {
            let mut inner = self.inner.borrow_mut();
            if let Some(previous) = previous.as_ref() {
                inner.total_bytes = inner.total_bytes.saturating_sub(previous.byte_len);
            }
            inner.total_bytes = inner.total_bytes.saturating_add(byte_len);
            inner.entries.insert(
                key.clone(),
                DiskCacheEntry {
                    requested_url: response.requested_url.to_string(),
                    final_url: response.final_url.to_string(),
                    status: response.status,
                    content_type: response.content_type.clone(),
                    headers: response.headers.clone(),
                    byte_len,
                    body_file,
                    stored_at,
                    last_used,
                },
            );
        }
        if let Some(previous) = previous {
            remove_file_if_exists(&self.root.join(previous.body_file))?;
        }
        self.diagnostics.borrow_mut().push(diagnostic(
            "disk-store",
            &key,
            &format!("bytes={byte_len}"),
        ));
        self.evict_until_within_limits()?;
        self.persist_index()
    }

    fn remove_corrupt_entry(&self, key: &str, detail: &str) -> WebbyResult<()> {
        let removed = self.inner.borrow_mut().entries.remove(key);
        if let Some(entry) = removed {
            let mut inner = self.inner.borrow_mut();
            inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_len);
            drop(inner);
            remove_file_if_exists(&self.root.join(entry.body_file))?;
        }
        self.diagnostics
            .borrow_mut()
            .push(diagnostic("disk-corrupt", key, detail));
        self.persist_index()
    }

    fn evict_until_within_limits(&self) -> WebbyResult<()> {
        loop {
            let victim = {
                let inner = self.inner.borrow();
                if inner.entries.len() <= self.limits.max_entries
                    && inner.total_bytes <= self.limits.max_bytes
                {
                    None
                } else {
                    inner
                        .entries
                        .iter()
                        .min_by_key(|(key, entry)| (entry.last_used, entry.stored_at, *key))
                        .map(|(key, _)| key.clone())
                }
            };
            let Some(key) = victim else {
                break;
            };
            let entry = {
                let mut inner = self.inner.borrow_mut();
                let entry = inner.entries.remove(&key);
                if let Some(entry) = entry.as_ref() {
                    inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_len);
                }
                entry
            };
            if let Some(entry) = entry {
                remove_file_if_exists(&self.root.join(entry.body_file))?;
                self.diagnostics.borrow_mut().push(diagnostic(
                    "disk-evict",
                    &key,
                    &format!("bytes={}", entry.byte_len),
                ));
            }
        }
        Ok(())
    }

    fn persist_index(&self) -> WebbyResult<()> {
        let path = self.root.join(INDEX_FILE);
        let mut text = serde_json::to_string_pretty(&*self.inner.borrow()).map_err(|error| {
            WebbyError::Parse {
                message: format!("could not serialize disk cache index: {error}"),
            }
        })?;
        text.push('\n');
        std::fs::write(&path, text).map_err(|source| WebbyError::Io {
            path: Some(path),
            source,
        })
    }
}

/// Public metadata for a stored resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntryMetadata {
    /// Deterministic cache key: requested URL string.
    pub key: String,
    /// Requested URL for the original load.
    pub requested_url: String,
    /// Final URL after redirects.
    pub final_url: String,
    /// Content type when known.
    pub content_type: Option<String>,
    /// Stored response byte length.
    pub byte_len: usize,
    /// Monotonic deterministic store sequence.
    pub stored_at: u64,
}

/// Human-readable cache event for debug output and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheDiagnostic {
    /// Event name such as `hit`, `miss`, `store`, `evict`, or `refresh`.
    pub event: &'static str,
    /// Cache key affected by the event.
    pub key: String,
    /// Extra deterministic details.
    pub detail: String,
}

impl CacheDiagnostic {
    /// Formats the diagnostic for CLI/app debug streams.
    pub fn format(&self) -> String {
        if self.detail.is_empty() {
            format!("cache {} {}", self.event, self.key)
        } else {
            format!("cache {} {} {}", self.event, self.key, self.detail)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CacheEntry {
    response: ResourceResponse,
    stored_at: u64,
    last_used: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct CacheInner {
    entries: BTreeMap<String, CacheEntry>,
    access_sequence: u64,
    store_sequence: u64,
    total_bytes: usize,
}

/// In-memory resource byte cache with deterministic LRU eviction.
#[derive(Debug)]
pub struct ResourceCache {
    limits: CacheLimits,
    inner: RefCell<CacheInner>,
}

impl ResourceCache {
    /// Creates an empty cache with default limits.
    pub fn new() -> Self {
        Self::with_limits(CacheLimits::default())
    }

    /// Creates an empty cache with explicit limits.
    pub fn with_limits(limits: CacheLimits) -> Self {
        Self {
            limits,
            inner: RefCell::new(CacheInner::default()),
        }
    }

    /// Loads a resource through the cache using the requested policy.
    pub fn load<L: ResourceLoader>(
        &self,
        loader: &L,
        url: &url::Url,
        mode: CacheMode,
    ) -> (WebbyResult<ResourceResponse>, Vec<CacheDiagnostic>) {
        self.load_with_headers(loader, url, mode, &[])
    }

    /// Loads a resource through the cache with request headers.
    pub fn load_with_headers<L: ResourceLoader>(
        &self,
        loader: &L,
        url: &url::Url,
        mode: CacheMode,
        headers: &[(String, String)],
    ) -> (WebbyResult<ResourceResponse>, Vec<CacheDiagnostic>) {
        let mut diagnostics = Vec::new();
        let key = cache_key(url);

        if mode == CacheMode::Use
            && let Some(response) = self.get_cached_response(&key, &mut diagnostics)
        {
            return (Ok(response), diagnostics);
        }

        diagnostics.push(match mode {
            CacheMode::Use => diagnostic("miss", &key, ""),
            CacheMode::Refresh => diagnostic("refresh", &key, "bypassing cached response"),
        });

        let result = loader.load_with_headers(url, headers);
        match result {
            Ok(response) => {
                self.store_response(&key, response.clone(), &mut diagnostics);
                (Ok(response), diagnostics)
            }
            Err(error) => {
                diagnostics.push(diagnostic("load-failed", &key, &error.to_string()));
                (Err(error), diagnostics)
            }
        }
    }

    /// Returns whether a key is present.
    pub fn contains_url(&self, url: &url::Url) -> bool {
        self.inner.borrow().entries.contains_key(&cache_key(url))
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        self.inner.borrow().entries.len()
    }

    /// Whether the cache has no stored entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Current stored response body bytes.
    pub fn total_bytes(&self) -> usize {
        self.inner.borrow().total_bytes
    }

    /// Deterministic metadata snapshot sorted by cache key.
    pub fn metadata(&self) -> Vec<CacheEntryMetadata> {
        self.inner
            .borrow()
            .entries
            .iter()
            .map(|(key, entry)| CacheEntryMetadata {
                key: key.clone(),
                requested_url: entry.response.requested_url.to_string(),
                final_url: entry.response.final_url.to_string(),
                content_type: entry.response.content_type.clone(),
                byte_len: entry.response.bytes.len(),
                stored_at: entry.stored_at,
            })
            .collect()
    }

    fn get_cached_response(
        &self,
        key: &str,
        diagnostics: &mut Vec<CacheDiagnostic>,
    ) -> Option<ResourceResponse> {
        let mut inner = self.inner.borrow_mut();
        inner.access_sequence = inner.access_sequence.saturating_add(1);
        let used_at = inner.access_sequence;
        let response = inner.entries.get_mut(key).map(|entry| {
            entry.last_used = used_at;
            entry.response.clone()
        });
        if response.is_some() {
            diagnostics.push(diagnostic("hit", key, ""));
        }
        response
    }

    fn store_response(
        &self,
        key: &str,
        response: ResourceResponse,
        diagnostics: &mut Vec<CacheDiagnostic>,
    ) {
        let byte_len = response.bytes.len();
        if self.limits.max_entries == 0 || byte_len > self.limits.max_bytes {
            diagnostics.push(diagnostic(
                "skip-store",
                key,
                &format!("bytes={byte_len} exceeds cache limits"),
            ));
            return;
        }

        {
            let mut inner = self.inner.borrow_mut();
            inner.store_sequence = inner.store_sequence.saturating_add(1);
            inner.access_sequence = inner.access_sequence.saturating_add(1);
            let stored_at = inner.store_sequence;
            let last_used = inner.access_sequence;
            if let Some(previous) = inner.entries.remove(key) {
                inner.total_bytes = inner
                    .total_bytes
                    .saturating_sub(previous.response.bytes.len());
            }
            inner.total_bytes = inner.total_bytes.saturating_add(byte_len);
            inner.entries.insert(
                key.to_string(),
                CacheEntry {
                    response,
                    stored_at,
                    last_used,
                },
            );
            diagnostics.push(diagnostic("store", key, &format!("bytes={byte_len}")));
        }

        self.evict_until_within_limits(diagnostics);
    }

    fn evict_until_within_limits(&self, diagnostics: &mut Vec<CacheDiagnostic>) {
        loop {
            let victim = {
                let inner = self.inner.borrow();
                if inner.entries.len() <= self.limits.max_entries
                    && inner.total_bytes <= self.limits.max_bytes
                {
                    None
                } else {
                    inner
                        .entries
                        .iter()
                        .min_by_key(|(key, entry)| (entry.last_used, entry.stored_at, *key))
                        .map(|(key, _)| key.clone())
                }
            };

            let Some(victim_key) = victim else {
                break;
            };

            let mut inner = self.inner.borrow_mut();
            if let Some(removed) = inner.entries.remove(&victim_key) {
                inner.total_bytes = inner
                    .total_bytes
                    .saturating_sub(removed.response.bytes.len());
                diagnostics.push(diagnostic(
                    "evict",
                    &victim_key,
                    &format!("bytes={}", removed.response.bytes.len()),
                ));
            }
        }
    }
}

impl Clone for ResourceCache {
    fn clone(&self) -> Self {
        Self {
            limits: self.limits,
            inner: RefCell::new(self.inner.borrow().clone()),
        }
    }
}

impl PartialEq for ResourceCache {
    fn eq(&self, other: &Self) -> bool {
        self.limits == other.limits && *self.inner.borrow() == *other.inner.borrow()
    }
}

impl Eq for ResourceCache {}

impl Default for ResourceCache {
    fn default() -> Self {
        Self::new()
    }
}

/// ResourceLoader adapter backed by a shared `ResourceCache`.
#[derive(Debug)]
pub struct CachedResourceLoader<'a, L> {
    loader: &'a L,
    cache: &'a ResourceCache,
    disk_cache: Option<&'a DiskResourceCache>,
    diagnostics: RefCell<Vec<CacheDiagnostic>>,
}

impl<'a, L: ResourceLoader> CachedResourceLoader<'a, L> {
    /// Creates a cache-aware adapter around a concrete resource loader.
    pub fn new(loader: &'a L, cache: &'a ResourceCache) -> Self {
        Self {
            loader,
            cache,
            disk_cache: None,
            diagnostics: RefCell::new(Vec::new()),
        }
    }

    /// Adds an optional persistent disk tier below the in-memory cache.
    pub fn with_disk_cache(mut self, disk_cache: &'a DiskResourceCache) -> Self {
        self.disk_cache = Some(disk_cache);
        self
    }

    /// Loads with an explicit mode and records diagnostics on this adapter.
    pub fn load_with_mode(&self, url: &url::Url, mode: CacheMode) -> WebbyResult<ResourceResponse> {
        self.load_with_headers_and_mode(url, &[], mode)
    }

    /// Loads with request headers and an explicit mode.
    pub fn load_with_headers_and_mode(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
        mode: CacheMode,
    ) -> WebbyResult<ResourceResponse> {
        let disk_loader = DiskBackedLoader {
            loader: self.loader,
            disk_cache: self.disk_cache,
            mode,
        };
        let (result, diagnostics) = self
            .cache
            .load_with_headers(&disk_loader, url, mode, headers);
        self.diagnostics.borrow_mut().extend(diagnostics);
        if let Some(disk_cache) = self.disk_cache {
            self.diagnostics
                .borrow_mut()
                .extend(disk_cache.take_diagnostics());
        }
        result
    }

    /// Returns and clears adapter-local cache diagnostics.
    pub fn take_diagnostics(&self) -> Vec<CacheDiagnostic> {
        std::mem::take(&mut *self.diagnostics.borrow_mut())
    }
}

struct DiskBackedLoader<'a, L> {
    loader: &'a L,
    disk_cache: Option<&'a DiskResourceCache>,
    mode: CacheMode,
}

impl<L: ResourceLoader> ResourceLoader for DiskBackedLoader<'_, L> {
    fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
        self.load_with_headers(url, &[])
    }

    fn load_with_headers(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
    ) -> WebbyResult<ResourceResponse> {
        if self.mode == CacheMode::Use
            && let Some(disk_cache) = self.disk_cache
            && let Some(response) = disk_cache.load(url)?
        {
            return Ok(response);
        }
        let response = self.loader.load_with_headers(url, headers)?;
        if let Some(disk_cache) = self.disk_cache {
            disk_cache.store(&response)?;
        }
        Ok(response)
    }
}

impl<L: ResourceLoader> ResourceLoader for CachedResourceLoader<'_, L> {
    fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
        self.load_with_mode(url, CacheMode::Use)
    }

    fn load_with_headers(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
    ) -> WebbyResult<ResourceResponse> {
        self.load_with_headers_and_mode(url, headers, CacheMode::Use)
    }
}

/// Returns the documented cache key for a requested resource URL.
pub fn cache_key(url: &url::Url) -> String {
    url.to_string()
}

fn diagnostic(event: &'static str, key: &str, detail: &str) -> CacheDiagnostic {
    CacheDiagnostic {
        event,
        key: key.to_string(),
        detail: detail.to_string(),
    }
}

fn parse_stored_url(value: &str) -> WebbyResult<url::Url> {
    url::Url::parse(value).map_err(|error| WebbyError::Parse {
        message: format!("corrupt disk cache URL {value:?}: {error}"),
    })
}

fn read_disk_index(root: &Path) -> WebbyResult<DiskCacheInner> {
    let path = root.join(INDEX_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| WebbyError::Parse {
            message: format!("corrupt disk cache index {}: {error}", path.display()),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DiskCacheInner::default()),
        Err(source) => Err(WebbyError::Io {
            path: Some(path),
            source,
        }),
    }
}

fn remove_disk_cache_contents(root: &Path) -> WebbyResult<()> {
    match std::fs::read_dir(root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|source| WebbyError::Io {
                    path: Some(root.to_path_buf()),
                    source,
                })?;
                let path = entry.path();
                if path.is_file() {
                    remove_file_if_exists(&path)?;
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(WebbyError::Io {
            path: Some(root.to_path_buf()),
            source,
        }),
    }
}

fn remove_file_if_exists(path: &Path) -> WebbyResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(WebbyError::Io {
            path: Some(path.to_path_buf()),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CacheLimits, CacheMode, CachedResourceLoader, DiskResourceCache, ResourceCache, cache_key,
    };
    use std::cell::Cell;
    use webby_core::{WebbyError, WebbyResult};
    use webby_net::{ResourceLoader, ResourceResponse};

    #[test]
    fn cache_miss_loads_through_loader_and_stores_bytes() -> WebbyResult<()> {
        let cache = ResourceCache::new();
        let loader = CountingLoader::new("first");
        let url = test_url("/page")?;

        let (response, diagnostics) = cache.load(&loader, &url, CacheMode::Use);

        assert_eq!(response?.bytes, b"first".to_vec());
        assert_eq!(loader.calls.get(), 1);
        assert!(cache.contains_url(&url));
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.event)
                .collect::<Vec<_>>(),
            vec!["miss", "store"]
        );
        Ok(())
    }

    #[test]
    fn cache_hit_avoids_second_loader_call() -> WebbyResult<()> {
        let cache = ResourceCache::new();
        let loader = CountingLoader::new("cached");
        let cached = CachedResourceLoader::new(&loader, &cache);
        let url = test_url("/page")?;

        assert_eq!(cached.load(&url)?.bytes, b"cached".to_vec());
        assert_eq!(cached.load(&url)?.bytes, b"cached".to_vec());

        assert_eq!(loader.calls.get(), 1);
        assert_eq!(
            cached
                .take_diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.event)
                .collect::<Vec<_>>(),
            vec!["miss", "store", "hit"]
        );
        Ok(())
    }

    #[test]
    fn failed_load_does_not_create_cache_entry() -> WebbyResult<()> {
        let cache = ResourceCache::new();
        let loader = FailingLoader;
        let url = test_url("/missing")?;

        let (result, diagnostics) = cache.load(&loader, &url, CacheMode::Use);

        assert!(matches!(result, Err(WebbyError::Network { .. })));
        assert!(!cache.contains_url(&url));
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.event)
                .collect::<Vec<_>>(),
            vec!["miss", "load-failed"]
        );
        Ok(())
    }

    #[test]
    fn cache_key_uses_requested_url_string() -> WebbyResult<()> {
        let url = test_url("/docs/../index.html?b=2&a=1")?;

        assert_eq!(cache_key(&url), "https://example.test/index.html?b=2&a=1");
        Ok(())
    }

    #[test]
    fn metadata_exposes_stored_response_fields() -> WebbyResult<()> {
        let cache = ResourceCache::new();
        let requested = test_url("/redirect")?;
        let final_url = test_url("/final")?;
        let loader = RedirectingLoader {
            final_url: final_url.clone(),
            content_type: Some("text/css; charset=utf-8".to_string()),
            body: b"body { color: red; }".to_vec(),
        };

        cache.load(&loader, &requested, CacheMode::Use).0?;
        let metadata = cache.metadata();
        let Some(entry) = metadata.first() else {
            return Err(WebbyError::invalid_input("cache metadata was empty"));
        };

        assert_eq!(metadata.len(), 1);
        assert_eq!(entry.key, requested.as_str());
        assert_eq!(entry.requested_url, requested.as_str());
        assert_eq!(entry.final_url, final_url.as_str());
        assert_eq!(
            entry.content_type.as_deref(),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(entry.byte_len, b"body { color: red; }".len());
        assert_eq!(entry.stored_at, 1);
        Ok(())
    }

    #[test]
    fn metadata_store_sequence_is_deterministic() -> WebbyResult<()> {
        let cache = ResourceCache::new();
        let loader = EchoLoader;
        let first = test_url("/a")?;
        let second = test_url("/b")?;

        cache.load(&loader, &first, CacheMode::Use).0?;
        cache.load(&loader, &second, CacheMode::Use).0?;

        let metadata = cache.metadata();
        let stored_sequences = metadata
            .iter()
            .map(|entry| (entry.key.as_str(), entry.stored_at))
            .collect::<Vec<_>>();
        assert_eq!(
            stored_sequences,
            vec![("https://example.test/a", 1), ("https://example.test/b", 2)]
        );
        Ok(())
    }

    #[test]
    fn count_eviction_uses_deterministic_lru_policy() -> WebbyResult<()> {
        let cache = ResourceCache::with_limits(CacheLimits::new(2, 1024));
        let loader = EchoLoader;
        let first = test_url("/first")?;
        let second = test_url("/second")?;
        let third = test_url("/third")?;

        cache.load(&loader, &first, CacheMode::Use).0?;
        cache.load(&loader, &second, CacheMode::Use).0?;
        cache.load(&loader, &first, CacheMode::Use).0?;
        let (_, diagnostics) = cache.load(&loader, &third, CacheMode::Use);

        assert!(cache.contains_url(&first));
        assert!(!cache.contains_url(&second));
        assert!(cache.contains_url(&third));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .format()
                .contains("evict https://example.test/second")
        }));
        Ok(())
    }

    #[test]
    fn formatted_diagnostics_are_exact_and_deterministic() -> WebbyResult<()> {
        let cache = ResourceCache::new();
        let loader = SequenceLoader::default();
        let url = test_url("/page")?;

        let (_, first_diagnostics) = cache.load(&loader, &url, CacheMode::Use);
        let (_, hit_diagnostics) = cache.load(&loader, &url, CacheMode::Use);
        let (_, refresh_diagnostics) = cache.load(&loader, &url, CacheMode::Refresh);

        assert_eq!(
            first_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.format())
                .collect::<Vec<_>>(),
            vec![
                "cache miss https://example.test/page".to_string(),
                "cache store https://example.test/page bytes=2".to_string(),
            ]
        );
        assert_eq!(
            hit_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.format())
                .collect::<Vec<_>>(),
            vec!["cache hit https://example.test/page".to_string()]
        );
        assert_eq!(
            refresh_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.format())
                .collect::<Vec<_>>(),
            vec![
                "cache refresh https://example.test/page bypassing cached response".to_string(),
                "cache store https://example.test/page bytes=2".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn byte_limit_eviction_is_deterministic() -> WebbyResult<()> {
        let cache = ResourceCache::with_limits(CacheLimits::new(4, 6));
        let loader = EchoLoader;
        let first = test_url("/aaa")?;
        let second = test_url("/bbbb")?;

        cache.load(&loader, &first, CacheMode::Use).0?;
        cache.load(&loader, &second, CacheMode::Use).0?;

        assert!(!cache.contains_url(&first));
        assert!(cache.contains_url(&second));
        assert!(cache.total_bytes() <= 6);
        Ok(())
    }

    #[test]
    fn refresh_bypasses_cached_entry_and_replaces_on_success() -> WebbyResult<()> {
        let cache = ResourceCache::new();
        let loader = SequenceLoader::default();
        let url = test_url("/page")?;

        assert_eq!(cache.load(&loader, &url, CacheMode::Use).0?.bytes, b"v1");
        assert_eq!(
            cache.load(&loader, &url, CacheMode::Refresh).0?.bytes,
            b"v2"
        );
        assert_eq!(cache.load(&loader, &url, CacheMode::Use).0?.bytes, b"v2");
        assert_eq!(loader.calls.get(), 2);
        Ok(())
    }

    #[test]
    fn parser_layout_render_crates_do_not_depend_on_cache() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for crate_name in [
            "webby_html",
            "webby_css",
            "webby_style",
            "webby_layout",
            "webby_render",
        ] {
            let manifest = std::fs::read_to_string(
                workspace.join("crates").join(crate_name).join("Cargo.toml"),
            )
            .unwrap_or_default();
            assert!(
                !manifest.contains("webby_cache"),
                "{crate_name} must not depend on webby_cache"
            );
        }
    }

    #[test]
    fn disk_cache_survives_adapter_restart() -> WebbyResult<()> {
        let root = temp_cache_dir("restart");
        let url = test_url("/page")?;
        let loader = CountingLoader::new("persistent");
        {
            let memory = ResourceCache::new();
            let disk = DiskResourceCache::open(&root)?;
            let cached = CachedResourceLoader::new(&loader, &memory).with_disk_cache(&disk);
            assert_eq!(cached.load(&url)?.bytes, b"persistent");
        }
        let memory = ResourceCache::new();
        let disk = DiskResourceCache::open(&root)?;
        let cached = CachedResourceLoader::new(&loader, &memory).with_disk_cache(&disk);

        assert_eq!(cached.load(&url)?.bytes, b"persistent");
        assert_eq!(loader.calls.get(), 1);
        assert!(
            cached
                .take_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.event == "disk-hit")
        );
        Ok(())
    }

    #[test]
    fn corrupt_disk_index_is_removed_safely() -> WebbyResult<()> {
        let root = temp_cache_dir("corrupt-index");
        std::fs::create_dir_all(&root).map_err(|source| WebbyError::Io {
            path: Some(root.clone()),
            source,
        })?;
        std::fs::write(root.join("index.json"), "{not json").map_err(|source| WebbyError::Io {
            path: Some(root.clone()),
            source,
        })?;

        let disk = DiskResourceCache::open(&root)?;

        assert!(
            disk.take_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.event == "disk-corrupt")
        );
        assert!(!root.join("index.json").exists());
        Ok(())
    }

    #[test]
    fn corrupt_disk_body_falls_back_to_loader() -> WebbyResult<()> {
        let root = temp_cache_dir("corrupt-body");
        let url = test_url("/page")?;
        let first_loader = CountingLoader::new("first");
        let first_memory = ResourceCache::new();
        let disk = DiskResourceCache::open(&root)?;
        CachedResourceLoader::new(&first_loader, &first_memory)
            .with_disk_cache(&disk)
            .load(&url)?;
        let Some(body_path) = std::fs::read_dir(&root)
            .map_err(|source| WebbyError::Io {
                path: Some(root.clone()),
                source,
            })?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|value| value.to_str()) == Some("bin"))
        else {
            return Err(WebbyError::invalid_input("disk cache body file was absent"));
        };
        std::fs::write(&body_path, b"bad").map_err(|source| WebbyError::Io {
            path: Some(body_path),
            source,
        })?;
        let second_loader = CountingLoader::new("second");
        let second_memory = ResourceCache::new();
        let cached =
            CachedResourceLoader::new(&second_loader, &second_memory).with_disk_cache(&disk);

        assert_eq!(cached.load(&url)?.bytes, b"second");
        assert_eq!(second_loader.calls.get(), 1);
        assert!(
            cached
                .take_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.event == "disk-corrupt")
        );
        Ok(())
    }

    #[test]
    fn corrupt_stored_url_falls_back_to_loader() -> WebbyResult<()> {
        let root = temp_cache_dir("corrupt-url");
        let url = test_url("/page")?;
        let first_memory = ResourceCache::new();
        let disk = DiskResourceCache::open(&root)?;
        CachedResourceLoader::new(&CountingLoader::new("first"), &first_memory)
            .with_disk_cache(&disk)
            .load(&url)?;
        let index_path = root.join("index.json");
        let index = std::fs::read_to_string(&index_path).map_err(|source| WebbyError::Io {
            path: Some(index_path.clone()),
            source,
        })?;
        std::fs::write(
            &index_path,
            index.replace(
                "\"final_url\": \"https://example.test/page\"",
                "\"final_url\": \"not a url\"",
            ),
        )
        .map_err(|source| WebbyError::Io {
            path: Some(index_path),
            source,
        })?;
        let reopened = DiskResourceCache::open(&root)?;
        let second_loader = CountingLoader::new("second");
        let second_memory = ResourceCache::new();
        let cached =
            CachedResourceLoader::new(&second_loader, &second_memory).with_disk_cache(&reopened);

        assert_eq!(cached.load(&url)?.bytes, b"second");
        assert!(
            cached
                .take_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.event == "disk-corrupt")
        );
        Ok(())
    }

    #[test]
    fn failed_load_does_not_poison_disk_cache() -> WebbyResult<()> {
        let root = temp_cache_dir("failed-load");
        let url = test_url("/missing")?;
        let memory = ResourceCache::new();
        let disk = DiskResourceCache::open(&root)?;
        let cached = CachedResourceLoader::new(&FailingLoader, &memory).with_disk_cache(&disk);

        assert!(cached.load(&url).is_err());
        let reopened = DiskResourceCache::open(&root)?;
        assert!(reopened.load(&url)?.is_none());
        Ok(())
    }

    #[test]
    fn disk_cache_evicts_oldest_entry_deterministically() -> WebbyResult<()> {
        let root = temp_cache_dir("eviction");
        let disk = DiskResourceCache::with_limits(&root, CacheLimits::new(1, 1024))?;
        let memory = ResourceCache::new();
        let cached = CachedResourceLoader::new(&EchoLoader, &memory).with_disk_cache(&disk);
        let first = test_url("/first")?;
        let second = test_url("/second")?;

        cached.load(&first)?;
        cached.load(&second)?;
        let diagnostics = cached.take_diagnostics();
        let reopened = DiskResourceCache::with_limits(&root, CacheLimits::new(1, 1024))?;

        assert!(reopened.load(&first)?.is_none());
        assert!(reopened.load(&second)?.is_some());
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.event == "disk-evict" && diagnostic.key == first.as_str()
        }));
        Ok(())
    }

    #[test]
    fn disk_cache_refresh_replaces_persisted_bytes() -> WebbyResult<()> {
        let root = temp_cache_dir("refresh");
        let disk = DiskResourceCache::open(&root)?;
        let memory = ResourceCache::new();
        let loader = SequenceLoader::default();
        let cached = CachedResourceLoader::new(&loader, &memory).with_disk_cache(&disk);
        let url = test_url("/page")?;

        assert_eq!(cached.load(&url)?.bytes, b"v1");
        assert_eq!(
            cached.load_with_mode(&url, CacheMode::Refresh)?.bytes,
            b"v2"
        );
        let reopened = DiskResourceCache::open(&root)?;
        assert_eq!(
            reopened.load(&url)?.map(|response| response.bytes),
            Some(b"v2".to_vec())
        );
        Ok(())
    }

    #[test]
    fn disk_cache_clear_removes_persisted_entries() -> WebbyResult<()> {
        let root = temp_cache_dir("clear");
        let disk = DiskResourceCache::open(&root)?;
        let memory = ResourceCache::new();
        let url = test_url("/page")?;
        CachedResourceLoader::new(&EchoLoader, &memory)
            .with_disk_cache(&disk)
            .load(&url)?;

        disk.clear()?;

        assert!(DiskResourceCache::open(&root)?.load(&url)?.is_none());
        Ok(())
    }

    fn test_url(path: &str) -> WebbyResult<url::Url> {
        url::Url::parse(&format!("https://example.test{path}")).map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })
    }

    fn temp_cache_dir(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("webby-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[derive(Debug)]
    struct CountingLoader {
        calls: Cell<usize>,
        body: Vec<u8>,
    }

    impl CountingLoader {
        fn new(body: &str) -> Self {
            Self {
                calls: Cell::new(0),
                body: body.as_bytes().to_vec(),
            }
        }
    }

    impl ResourceLoader for CountingLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            self.calls.set(self.calls.get().saturating_add(1));
            Ok(response(url, self.body.clone()))
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct EchoLoader;

    impl ResourceLoader for EchoLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            Ok(response(url, url.path().as_bytes().to_vec()))
        }
    }

    #[derive(Debug, Default)]
    struct SequenceLoader {
        calls: Cell<usize>,
    }

    impl ResourceLoader for SequenceLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            let next = self.calls.get().saturating_add(1);
            self.calls.set(next);
            Ok(response(url, format!("v{next}").into_bytes()))
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FailingLoader;

    impl ResourceLoader for FailingLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            Err(WebbyError::Network {
                message: format!("no test resource for {url}"),
            })
        }
    }

    #[derive(Debug, Clone)]
    struct RedirectingLoader {
        final_url: url::Url,
        content_type: Option<String>,
        body: Vec<u8>,
    }

    impl ResourceLoader for RedirectingLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            Ok(ResourceResponse {
                requested_url: url.clone(),
                final_url: self.final_url.clone(),
                status: Some(200),
                content_type: self.content_type.clone(),
                headers: Vec::new(),
                bytes: self.body.clone(),
            })
        }
    }

    fn response(url: &url::Url, bytes: Vec<u8>) -> ResourceResponse {
        ResourceResponse {
            requested_url: url.clone(),
            final_url: url.clone(),
            status: Some(200),
            content_type: Some("text/plain".to_string()),
            headers: Vec::new(),
            bytes,
        }
    }
}
