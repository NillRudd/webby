//! Address/search input resolution for Webby's browser chrome.

use std::net::IpAddr;
use std::path::Path;

use webby_core::{WebbyError, WebbyResult};

/// Search engine configuration for turning free-form input into a search URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchEngine {
    /// Human-readable search engine name.
    pub name: String,
    /// URL prefix used before the percent-encoded query.
    pub query_url: String,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self {
            name: "Google".to_string(),
            query_url: "https://www.google.com/search?q=".to_string(),
        }
    }
}

/// Classification of address bar input after resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedInputKind {
    /// A network URL was entered or inferred.
    Url,
    /// Free-form text became a search URL.
    Search,
    /// A local file path became a file URL.
    File,
}

/// Result of resolving address/search bar input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInput {
    /// Original user-provided input, trimmed but otherwise preserved.
    pub original: String,
    /// Normalized URL target.
    pub url: url::Url,
    /// How Webby classified the input.
    pub kind: ResolvedInputKind,
}

/// One successful form control included in a GET submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    /// Field name.
    pub name: String,
    /// Field value.
    pub value: String,
}

/// Resolves address/search input into a URL.
pub fn resolve_input(input: &str, search_engine: &SearchEngine) -> WebbyResult<ResolvedInput> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err(WebbyError::invalid_input("address/search input is empty"));
    }

    if let Some(resolved) = resolve_file_path(trimmed)? {
        return Ok(resolved);
    }

    if has_explicit_url_marker(trimmed) {
        return resolve_explicit_url(trimmed);
    }

    if is_localhost_target(trimmed) {
        return resolve_inferred_url(trimmed, "http");
    }

    if is_bare_network_target(trimmed) {
        return resolve_inferred_url(trimmed, "https");
    }

    resolve_search(trimmed, search_engine)
}

/// Resolves and serializes a `method=get` form target.
pub fn resolve_get_form_url(
    current_url: &url::Url,
    action: Option<&str>,
    fields: &[FormField],
) -> WebbyResult<url::Url> {
    let target = match action.map(str::trim).filter(|value| !value.is_empty()) {
        Some(action) => current_url.join(action).map_err(|error| WebbyError::Url {
            message: format!("invalid form action {action:?}: {error}"),
        })?,
        None => current_url.clone(),
    };
    let mut target = target;
    {
        let mut query = target.query_pairs_mut();
        query.clear();
        for field in fields {
            if !field.name.is_empty() {
                query.append_pair(&field.name, &field.value);
            }
        }
    }
    Ok(target)
}

/// Resolves a form action URL without adding query fields.
pub fn resolve_form_action_url(
    current_url: &url::Url,
    action: Option<&str>,
) -> WebbyResult<url::Url> {
    match action.map(str::trim).filter(|value| !value.is_empty()) {
        Some(action) => current_url.join(action).map_err(|error| WebbyError::Url {
            message: format!("invalid form action {action:?}: {error}"),
        }),
        None => Ok(current_url.clone()),
    }
}

/// Serializes successful controls as `application/x-www-form-urlencoded`.
pub fn form_urlencoded_body(fields: &[FormField]) -> String {
    fields
        .iter()
        .filter(|field| !field.name.is_empty())
        .map(|field| {
            format!(
                "{}={}",
                urlencoding::encode(&field.name).replace("%20", "+"),
                urlencoding::encode(&field.value).replace("%20", "+")
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn resolve_file_path(input: &str) -> WebbyResult<Option<ResolvedInput>> {
    let path = Path::new(input);

    if !path.exists() {
        return Ok(None);
    }

    let absolute = path.canonicalize().map_err(|source| WebbyError::Io {
        path: Some(path.to_path_buf()),
        source,
    })?;
    let url = url::Url::from_file_path(&absolute).map_err(|()| WebbyError::Url {
        message: format!("could not convert file path to URL: {}", absolute.display()),
    })?;

    Ok(Some(ResolvedInput {
        original: input.to_string(),
        url,
        kind: ResolvedInputKind::File,
    }))
}

fn resolve_explicit_url(input: &str) -> WebbyResult<ResolvedInput> {
    let url = url::Url::parse(input).map_err(|error| WebbyError::Url {
        message: error.to_string(),
    })?;

    let kind = match url.scheme() {
        "http" | "https" => ResolvedInputKind::Url,
        "file" => ResolvedInputKind::File,
        scheme => {
            return Err(WebbyError::Url {
                message: format!("unsupported URL scheme: {scheme}"),
            });
        }
    };

    Ok(ResolvedInput {
        original: input.to_string(),
        url,
        kind,
    })
}

fn resolve_inferred_url(input: &str, scheme: &str) -> WebbyResult<ResolvedInput> {
    let raw_url = format!("{scheme}://{input}");
    let url = url::Url::parse(&raw_url).map_err(|error| WebbyError::Url {
        message: error.to_string(),
    })?;

    Ok(ResolvedInput {
        original: input.to_string(),
        url,
        kind: ResolvedInputKind::Url,
    })
}

fn resolve_search(input: &str, search_engine: &SearchEngine) -> WebbyResult<ResolvedInput> {
    if search_engine.query_url.trim().is_empty() {
        return Err(WebbyError::invalid_input(
            "search engine query URL is empty",
        ));
    }

    let encoded_query = urlencoding::encode(input).replace("%20", "+");
    let raw_url = format!("{}{}", search_engine.query_url, encoded_query);
    let url = url::Url::parse(&raw_url).map_err(|error| WebbyError::Url {
        message: format!("invalid search engine URL: {error}"),
    })?;

    if !matches!(url.scheme(), "http" | "https") {
        return Err(WebbyError::Url {
            message: format!("unsupported search engine URL scheme: {}", url.scheme()),
        });
    }

    Ok(ResolvedInput {
        original: input.to_string(),
        url,
        kind: ResolvedInputKind::Search,
    })
}

fn has_explicit_url_marker(input: &str) -> bool {
    input.contains("://") || input.to_ascii_lowercase().starts_with("file:")
}

fn is_localhost_target(input: &str) -> bool {
    if input == "localhost" || input.starts_with("localhost:") || input.starts_with("localhost/") {
        return true;
    }

    host_part(input)
        .and_then(|host| host.parse::<IpAddr>().ok())
        .is_some_and(|address| address.is_loopback())
}

fn is_bare_network_target(input: &str) -> bool {
    if input.chars().any(char::is_whitespace) {
        return false;
    }

    let Some(host) = host_part(input) else {
        return false;
    };

    if host.parse::<IpAddr>().is_ok() {
        return true;
    }

    host.contains('.') && host.chars().any(char::is_alphabetic)
}

fn host_part(input: &str) -> Option<&str> {
    let without_path = input.split('/').next().unwrap_or(input);
    let without_port = if let Some(rest) = without_path.strip_prefix('[') {
        let end = rest.find(']')?;
        &rest[..end]
    } else {
        without_path.split(':').next().unwrap_or(without_path)
    };

    if without_port.is_empty() {
        None
    } else {
        Some(without_port)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FormField, ResolvedInputKind, SearchEngine, form_urlencoded_body, resolve_form_action_url,
        resolve_get_form_url, resolve_input,
    };
    use webby_core::{WebbyError, WebbyResult};

    fn resolve(input: &str) -> WebbyResult<String> {
        Ok(resolve_input(input, &SearchEngine::default())?
            .url
            .to_string())
    }

    #[test]
    fn get_form_url_serializes_query_fields() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/search?old=1").map_err(url_error)?;
        let fields = vec![
            FormField {
                name: "q".to_string(),
                value: "rust browser".to_string(),
            },
            FormField {
                name: "page".to_string(),
                value: "1".to_string(),
            },
        ];

        let url = resolve_get_form_url(&base, Some("/find"), &fields)?;

        assert_eq!(
            url.as_str(),
            "https://example.test/find?q=rust+browser&page=1"
        );
        Ok(())
    }

    #[test]
    fn get_form_url_missing_action_uses_current_url() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/docs/index.html").map_err(url_error)?;
        let fields = vec![FormField {
            name: "q".to_string(),
            value: "webby".to_string(),
        }];

        let url = resolve_get_form_url(&base, None, &fields)?;

        assert_eq!(url.as_str(), "https://example.test/docs/index.html?q=webby");
        Ok(())
    }

    #[test]
    fn get_form_url_resolves_relative_action() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/forms/search.html").map_err(url_error)?;
        let fields = vec![FormField {
            name: "q".to_string(),
            value: "rust".to_string(),
        }];

        let url = resolve_get_form_url(&base, Some("results"), &fields)?;

        assert_eq!(url.as_str(), "https://example.test/forms/results?q=rust");
        Ok(())
    }

    #[test]
    fn post_form_helpers_resolve_action_and_encode_body() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/forms/search.html").map_err(url_error)?;
        let target = resolve_form_action_url(&base, Some("submit"))?;
        let body = form_urlencoded_body(&[
            FormField {
                name: "q".to_string(),
                value: "rust browser".to_string(),
            },
            FormField {
                name: "emoji".to_string(),
                value: "★&=+".to_string(),
            },
        ]);

        assert_eq!(target.as_str(), "https://example.test/forms/submit");
        assert_eq!(body, "q=rust+browser&emoji=%E2%98%85%26%3D%2B");
        Ok(())
    }

    #[test]
    fn get_form_url_serializes_spaces_unicode_and_symbols() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/find").map_err(url_error)?;
        let fields = vec![
            FormField {
                name: "space".to_string(),
                value: "rust browser".to_string(),
            },
            FormField {
                name: "unicode".to_string(),
                value: "räksmörgås".to_string(),
            },
            FormField {
                name: "symbols".to_string(),
                value: "a&b=c?".to_string(),
            },
        ];

        let url = resolve_get_form_url(&base, None, &fields)?;

        assert_eq!(
            url.as_str(),
            "https://example.test/find?space=rust+browser&unicode=r%C3%A4ksm%C3%B6rg%C3%A5s&symbols=a%26b%3Dc%3F"
        );
        Ok(())
    }

    #[test]
    fn get_form_url_skips_empty_name_fields() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/find").map_err(url_error)?;
        let fields = vec![
            FormField {
                name: String::new(),
                value: "ignored".to_string(),
            },
            FormField {
                name: "q".to_string(),
                value: "kept".to_string(),
            },
        ];

        let url = resolve_get_form_url(&base, None, &fields)?;

        assert_eq!(url.as_str(), "https://example.test/find?q=kept");
        Ok(())
    }

    fn resolve_kind(input: &str) -> WebbyResult<ResolvedInputKind> {
        Ok(resolve_input(input, &SearchEngine::default())?.kind)
    }

    fn url_error(error: url::ParseError) -> WebbyError {
        WebbyError::Url {
            message: error.to_string(),
        }
    }

    #[test]
    fn default_search_engine_matches_v0_1_target() {
        let engine = SearchEngine::default();

        assert_eq!(engine.name, "Google");
        assert_eq!(engine.query_url, "https://www.google.com/search?q=");
    }

    #[test]
    fn resolver_rejects_empty_input_without_panicking() {
        let result = resolve_input("   ", &SearchEngine::default());

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn resolver_preserves_http_and_https_urls() -> WebbyResult<()> {
        assert_eq!(resolve("https://example.com")?, "https://example.com/");
        assert_eq!(resolve("http://example.com")?, "http://example.com/");
        assert_eq!(
            resolve("https://example.com/docs?q=webby")?,
            "https://example.com/docs?q=webby"
        );
        Ok(())
    }

    #[test]
    fn resolver_converts_bare_domains_to_https() -> WebbyResult<()> {
        assert_eq!(resolve("example.com")?, "https://example.com/");
        assert_eq!(
            resolve("example.com/docs/index.html")?,
            "https://example.com/docs/index.html"
        );
        Ok(())
    }

    #[test]
    fn resolver_converts_localhost_and_loopback_to_http() -> WebbyResult<()> {
        assert_eq!(resolve("localhost:8080")?, "http://localhost:8080/");
        assert_eq!(resolve("127.0.0.1:3000")?, "http://127.0.0.1:3000/");
        assert_eq!(resolve("127.0.0.1/docs")?, "http://127.0.0.1/docs");
        assert_eq!(resolve("[::1]:8080")?, "http://[::1]:8080/");
        Ok(())
    }

    #[test]
    fn resolver_converts_public_ip_addresses_to_https() -> WebbyResult<()> {
        assert_eq!(resolve("93.184.216.34")?, "https://93.184.216.34/");
        Ok(())
    }

    #[test]
    fn resolver_converts_search_queries_with_plus_separated_terms() -> WebbyResult<()> {
        assert_eq!(
            resolve("rust browser engine")?,
            "https://www.google.com/search?q=rust+browser+engine"
        );
        assert_eq!(
            resolve("rust+browser")?,
            "https://www.google.com/search?q=rust%2Bbrowser"
        );
        Ok(())
    }

    #[test]
    fn resolver_percent_encodes_unicode_search_queries() -> WebbyResult<()> {
        assert_eq!(
            resolve("räksmörgås browser")?,
            "https://www.google.com/search?q=r%C3%A4ksm%C3%B6rg%C3%A5s+browser"
        );
        Ok(())
    }

    #[test]
    fn resolver_converts_existing_relative_file_paths() -> WebbyResult<()> {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/simple.html");
        let result = resolve_input(&fixture.to_string_lossy(), &SearchEngine::default())?;

        assert_eq!(result.kind, ResolvedInputKind::File);
        assert_eq!(result.url.scheme(), "file");
        assert!(result.url.as_str().ends_with("/examples/simple.html"));
        Ok(())
    }

    #[test]
    fn resolver_preserves_existing_file_urls() {
        let file_url = resolve_input("file:///tmp/webby.html", &SearchEngine::default());

        assert!(matches!(
            file_url,
            Ok(resolved) if resolved.kind == ResolvedInputKind::File
        ));
    }

    #[test]
    fn resolver_trims_input_but_preserves_trimmed_original() -> WebbyResult<()> {
        let resolved = resolve_input("  example.com  ", &SearchEngine::default())?;

        assert_eq!(resolved.original, "example.com");
        assert_eq!(resolved.url.as_str(), "https://example.com/");
        Ok(())
    }

    #[test]
    fn resolver_rejects_unsupported_url_schemes() {
        let result = resolve_input("ftp://example.com", &SearchEngine::default());

        assert!(matches!(result, Err(WebbyError::Url { .. })));
    }

    #[test]
    fn resolver_rejects_invalid_search_engine_configuration() {
        let search_engine = SearchEngine {
            name: "Broken".to_string(),
            query_url: "webby://search?q=".to_string(),
        };
        let result = resolve_input("rust browser engine", &search_engine);

        assert!(matches!(result, Err(WebbyError::Url { .. })));
    }

    #[test]
    fn resolver_supports_search_engine_urls_with_existing_query_parameters() {
        let search_engine = SearchEngine {
            name: "Example".to_string(),
            query_url: "https://search.example/search?source=webby&q=".to_string(),
        };
        let result = resolve_input("rust browser engine", &search_engine);

        assert!(matches!(
            result,
            Ok(resolved)
                if resolved.url.as_str()
                    == "https://search.example/search?source=webby&q=rust+browser+engine"
        ));
    }

    #[test]
    fn resolver_classifies_search_and_url_kinds() -> WebbyResult<()> {
        assert_eq!(resolve_kind("example.com")?, ResolvedInputKind::Url);
        assert_eq!(
            resolve_kind("rust browser engine")?,
            ResolvedInputKind::Search
        );
        Ok(())
    }

    #[test]
    fn generated_boundary_inputs_resolve_deterministically_without_panicking() {
        let fragments = [
            "",
            " ",
            ":",
            "://",
            "http://",
            "https://[",
            "file:",
            "file://host/path",
            "[::1",
            "localhost:",
            "example..test",
            "word word",
            "räksmörgås",
            "\0",
        ];

        for repeat in 1..=fragments.len() {
            let input = fragments[..repeat].join("/");
            let first = resolve_input(&input, &SearchEngine::default());
            let second = resolve_input(&input, &SearchEngine::default());

            assert_eq!(format!("{first:?}"), format!("{second:?}"));
        }
    }
}
