//! Origin, CORS, and small security-policy helpers for Webby's browser shell.
//!
//! This crate owns reusable security decisions. It does not load resources,
//! parse documents, execute JavaScript, or mutate app state.

use webby_core::{WebbyError, WebbyResult};

/// Webby's deterministic origin model.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// Tuple origin for network schemes.
    Tuple {
        /// URL scheme.
        scheme: String,
        /// Lowercase host.
        host: String,
        /// Explicit or known-default port.
        port: u16,
    },
    /// Webby's intentionally simplified shared local-file origin.
    LocalFile,
}

impl Origin {
    /// Returns the deterministic key used by storage and diagnostics.
    pub fn key(&self) -> String {
        match self {
            Self::Tuple { scheme, host, port } => format!("{scheme}://{host}:{port}"),
            Self::LocalFile => "file://local".to_string(),
        }
    }

    /// Returns the value used for CORS `Access-Control-Allow-Origin` matching.
    pub fn serialized_for_cors(&self) -> String {
        self.key()
    }
}

/// Builds a Webby origin from a URL.
pub fn origin_for_url(url: &url::Url) -> WebbyResult<Origin> {
    match url.scheme() {
        "http" | "https" => {
            let Some(host) = url.host_str() else {
                return Err(WebbyError::invalid_input(format!(
                    "origin requires host for {}",
                    url.scheme()
                )));
            };
            let Some(port) = url.port_or_known_default() else {
                return Err(WebbyError::invalid_input(format!(
                    "origin requires port for {}",
                    url.scheme()
                )));
            };
            Ok(Origin::Tuple {
                scheme: url.scheme().to_ascii_lowercase(),
                host: host.to_ascii_lowercase(),
                port,
            })
        }
        "file" => Ok(Origin::LocalFile),
        other => Err(WebbyError::unsupported(format!(
            "security origin is unsupported for {other}: URLs"
        ))),
    }
}

/// Returns Webby's deterministic origin key for a URL.
pub fn origin_key(url: &url::Url) -> WebbyResult<String> {
    origin_for_url(url).map(|origin| origin.key())
}

/// Returns true when two URLs share Webby's origin.
pub fn same_origin(left: &url::Url, right: &url::Url) -> bool {
    origin_for_url(left).ok() == origin_for_url(right).ok()
}

/// Result of evaluating a fetch/XHR CORS check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorsDecision {
    /// Request is allowed.
    Allowed,
    /// Request is blocked with a deterministic reason.
    Blocked(String),
}

/// Evaluates Webby's v0.1 CORS policy for a loaded fetch/XHR resource.
pub fn check_cors(
    page_url: &url::Url,
    resource_url: &url::Url,
    headers: &[(String, String)],
) -> CorsDecision {
    if same_origin(page_url, resource_url) {
        return CorsDecision::Allowed;
    }

    let Ok(origin) = origin_for_url(page_url) else {
        return CorsDecision::Blocked(format!(
            "CORS blocked request from unsupported origin {} to {}",
            page_url, resource_url
        ));
    };
    let expected = origin.serialized_for_cors();
    let allowed = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("access-control-allow-origin"))
        .any(|(_, value)| {
            let trimmed = value.trim();
            trimmed == "*" || trimmed == expected
        });
    if allowed {
        CorsDecision::Allowed
    } else {
        CorsDecision::Blocked(format!(
            "CORS blocked request from {} to {}",
            expected, resource_url
        ))
    }
}

/// Returns a deterministic mixed-content warning when an HTTPS page references
/// an HTTP subresource.
pub fn mixed_content_diagnostic(page_url: &url::Url, resource_url: &url::Url) -> Option<String> {
    (page_url.scheme() == "https" && resource_url.scheme() == "http").then(|| {
        format!(
            "mixed content diagnostic: HTTPS page {} referenced insecure resource {}",
            page_url, resource_url
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{CorsDecision, check_cors, mixed_content_diagnostic, origin_for_url, origin_key};
    use webby_core::{WebbyError, WebbyResult};

    fn parse_url(raw: &str) -> WebbyResult<url::Url> {
        url::Url::parse(raw).map_err(|error| WebbyError::Url {
            message: format!("test URL parse failed: {error}"),
        })
    }

    #[test]
    fn origin_model_covers_scheme_host_port_and_file() -> WebbyResult<()> {
        let https = parse_url("https://Example.test/path")?;
        let http = parse_url("http://example.test:8080/path")?;
        let file = parse_url("file:///tmp/page.html")?;

        assert_eq!(origin_for_url(&https)?.key(), "https://example.test:443");
        assert_eq!(origin_for_url(&http)?.key(), "http://example.test:8080");
        assert_eq!(origin_for_url(&file)?.key(), "file://local");
        Ok(())
    }

    #[test]
    fn same_origin_compares_normalized_origin_tuple() -> WebbyResult<()> {
        let page = parse_url("https://example.test/a")?;
        let same = parse_url("https://EXAMPLE.test:443/b")?;
        let other_port = parse_url("https://example.test:444/b")?;

        assert!(super::same_origin(&page, &same));
        assert!(!super::same_origin(&page, &other_port));
        Ok(())
    }

    #[test]
    fn cors_allows_same_origin_wildcard_and_exact_origin() -> WebbyResult<()> {
        let page = parse_url("https://example.test/a")?;
        let same = parse_url("https://example.test/b")?;
        let cross = parse_url("https://api.example.test/data")?;

        assert_eq!(check_cors(&page, &same, &[]), CorsDecision::Allowed);
        assert_eq!(
            check_cors(
                &page,
                &cross,
                &[("Access-Control-Allow-Origin".to_string(), "*".to_string())],
            ),
            CorsDecision::Allowed
        );
        assert_eq!(
            check_cors(
                &page,
                &cross,
                &[(
                    "access-control-allow-origin".to_string(),
                    "https://example.test:443".to_string(),
                )],
            ),
            CorsDecision::Allowed
        );
        assert!(matches!(
            check_cors(&page, &cross, &[]),
            CorsDecision::Blocked(message) if message.contains("CORS blocked")
        ));
        Ok(())
    }

    #[test]
    fn mixed_content_diagnostic_is_https_to_http_only() -> WebbyResult<()> {
        let page = parse_url("https://example.test/")?;
        let insecure = parse_url("http://example.test/image.png")?;
        let secure = parse_url("https://example.test/image.png")?;

        assert!(mixed_content_diagnostic(&page, &insecure).is_some());
        assert!(mixed_content_diagnostic(&page, &secure).is_none());
        assert_eq!(origin_key(&page)?, "https://example.test:443");
        Ok(())
    }
}
