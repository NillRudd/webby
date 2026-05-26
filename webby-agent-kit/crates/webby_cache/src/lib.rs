//! Deterministic resource byte cache for page, stylesheet, and image loads.
//!
//! The cache stores successful `webby_net::ResourceResponse` bytes keyed by
//! the requested URL string. Transport remains in `webby_net`; this crate owns
//! only storage, limits, policy, and cache diagnostics.

use std::cell::RefCell;
use std::collections::BTreeMap;

use webby_core::WebbyResult;
use webby_net::{ResourceLoader, ResourceResponse};

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
    diagnostics: RefCell<Vec<CacheDiagnostic>>,
}

impl<'a, L: ResourceLoader> CachedResourceLoader<'a, L> {
    /// Creates a cache-aware adapter around a concrete resource loader.
    pub fn new(loader: &'a L, cache: &'a ResourceCache) -> Self {
        Self {
            loader,
            cache,
            diagnostics: RefCell::new(Vec::new()),
        }
    }

    /// Loads with an explicit mode and records diagnostics on this adapter.
    pub fn load_with_mode(&self, url: &url::Url, mode: CacheMode) -> WebbyResult<ResourceResponse> {
        let (result, diagnostics) = self.cache.load(self.loader, url, mode);
        self.diagnostics.borrow_mut().extend(diagnostics);
        result
    }

    /// Loads with request headers and an explicit mode.
    pub fn load_with_headers_and_mode(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
        mode: CacheMode,
    ) -> WebbyResult<ResourceResponse> {
        let (result, diagnostics) = self
            .cache
            .load_with_headers(self.loader, url, mode, headers);
        self.diagnostics.borrow_mut().extend(diagnostics);
        result
    }

    /// Returns and clears adapter-local cache diagnostics.
    pub fn take_diagnostics(&self) -> Vec<CacheDiagnostic> {
        std::mem::take(&mut *self.diagnostics.borrow_mut())
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

#[cfg(test)]
mod tests {
    use super::{CacheLimits, CacheMode, CachedResourceLoader, ResourceCache, cache_key};
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

    fn test_url(path: &str) -> WebbyResult<url::Url> {
        url::Url::parse(&format!("https://example.test{path}")).map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })
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
