# Module Spec: webby_url

## Purpose

Resolve address bar input into a URL.

## Required behavior

- Existing `http://` and `https://` URLs are preserved and normalized.
- Bare domains become `https://domain/`.
- `localhost` and loopback addresses default to `http://`.
- Existing file paths become `file://` URLs.
- Everything else becomes a search query.

## Public API sketch

```rust
pub struct SearchEngine {
    pub name: String,
    pub query_url: String,
}

pub enum ResolvedInputKind {
    Url,
    Search,
    File,
}

pub struct ResolvedInput {
    pub original: String,
    pub url: url::Url,
    pub kind: ResolvedInputKind,
}

pub fn resolve_input(input: &str, search_engine: &SearchEngine) -> WebbyResult<ResolvedInput>;
```

## Tests

Cover:

- absolute URLs
- bare domains
- localhost
- IP addresses
- queries with spaces
- unicode queries
- empty input
- file paths
