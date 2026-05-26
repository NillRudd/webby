//! External stylesheet resource coordination for app and CLI composition.
//!
//! Transport remains in `webby_net`, parsing remains in `webby_css`, and style
//! application remains in `webby_style`. This crate only keeps the fetch/parse
//! orchestration and non-fatal diagnostics consistent across callers.

use webby_core::{WebbyError, WebbyResult};
use webby_css::Stylesheet;
use webby_dom::Document;
use webby_net::{ResourceLoader, decode_text_utf8};

/// Loaded external stylesheets plus deterministic non-fatal diagnostics.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadedStylesheets {
    /// Parsed external stylesheets in document order.
    pub stylesheets: Vec<Stylesheet>,
    /// Non-fatal URL/load/parse diagnostics in document order.
    pub diagnostics: Vec<String>,
}

/// Resolves a linked stylesheet URL against the owning document URL.
pub fn resolve_stylesheet_url(base_url: &url::Url, href: &str) -> WebbyResult<url::Url> {
    let trimmed = href.trim();
    if trimmed.is_empty() {
        return Err(WebbyError::Url {
            message: "stylesheet href is empty".to_string(),
        });
    }

    base_url.join(trimmed).map_err(|error| WebbyError::Url {
        message: format!("could not resolve stylesheet href {href:?} against {base_url}: {error}"),
    })
}

/// Loads and parses linked external stylesheets without failing the page.
pub fn load_external_stylesheets<L: ResourceLoader>(
    document: &Document,
    base_url: &url::Url,
    loader: &L,
) -> LoadedStylesheets {
    let mut loaded = LoadedStylesheets::default();
    for link in webby_html::collect_stylesheet_links(document) {
        let stylesheet_url = match resolve_stylesheet_url(base_url, &link.href) {
            Ok(url) => url,
            Err(error) => {
                loaded.diagnostics.push(format!(
                    "stylesheet href {:?} could not be resolved: {}",
                    link.href, error
                ));
                continue;
            }
        };
        let response = match loader.load(&stylesheet_url) {
            Ok(response) => response,
            Err(error) => {
                loaded.diagnostics.push(format!(
                    "stylesheet {} could not be loaded: {}",
                    stylesheet_url, error
                ));
                continue;
            }
        };
        let css = decode_text_utf8(&response.bytes);
        match webby_css::parse_stylesheet(&css) {
            Ok(mut stylesheet) => {
                loaded.diagnostics.extend(
                    stylesheet.diagnostics.iter().map(|diagnostic| {
                        format_source_diagnostic(&response.final_url, diagnostic)
                    }),
                );
                stylesheet.diagnostics.clear();
                loaded.stylesheets.push(stylesheet);
            }
            Err(error) => loaded.diagnostics.push(format!(
                "stylesheet {} could not be parsed: {}",
                response.final_url, error
            )),
        }
    }
    loaded
}

fn format_source_diagnostic(url: &url::Url, diagnostic: &webby_css::CssDiagnostic) -> String {
    format!(
        "stylesheet {url} CSS diagnostic at byte {}: {}",
        diagnostic.offset, diagnostic.message
    )
}

#[cfg(test)]
mod tests {
    use super::{load_external_stylesheets, resolve_stylesheet_url};
    use std::cell::Cell;
    use webby_core::{WebbyError, WebbyResult};
    use webby_net::{ResourceLoader, ResourceResponse};

    #[test]
    fn relative_stylesheet_urls_resolve_for_file_and_http_pages() -> WebbyResult<()> {
        let file = url::Url::parse("file:///tmp/webby/pages/index.html").map_err(url_error)?;
        let http = url::Url::parse("https://example.test/docs/index.html").map_err(url_error)?;

        assert_eq!(
            resolve_stylesheet_url(&file, "../assets/site.css")?.as_str(),
            "file:///tmp/webby/assets/site.css"
        );
        assert_eq!(
            resolve_stylesheet_url(&http, "../assets/site.css")?.as_str(),
            "https://example.test/assets/site.css"
        );
        Ok(())
    }

    #[test]
    fn stylesheet_load_and_css_diagnostics_keep_document_order() -> WebbyResult<()> {
        let document = webby_html::parse_document(
            "<link rel=\"stylesheet\" href=\"missing.css\"><link rel=\"stylesheet\" href=\"bad.css\">",
        )?;
        let base = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let loader = StylesheetLoader;

        let loaded = load_external_stylesheets(&document, &base, &loader);

        assert_eq!(loaded.stylesheets.len(), 1);
        assert_eq!(loaded.diagnostics.len(), 2);
        assert!(loaded.diagnostics[0].contains("missing.css"));
        assert!(loaded.diagnostics[0].contains("could not be loaded"));
        assert!(loaded.diagnostics[1].contains("bad.css"));
        assert!(loaded.diagnostics[1].contains("CSS diagnostic at byte"));
        assert!(loaded.diagnostics[1].contains("skipped CSS without declaration block"));
        assert!(loaded.stylesheets[0].diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn malformed_external_css_diagnostics_keep_source_and_order() -> WebbyResult<()> {
        let document = webby_html::parse_document(
            "<link rel=\"stylesheet\" href=\"first.css\"><link rel=\"stylesheet\" href=\"second.css\">",
        )?;
        let base = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let loader = MultiBadStylesheetLoader;

        let loaded = load_external_stylesheets(&document, &base, &loader);

        assert_eq!(loaded.stylesheets.len(), 2);
        assert_eq!(loaded.diagnostics.len(), 2);
        assert!(loaded.diagnostics[0].contains("https://example.test/first.css"));
        assert!(loaded.diagnostics[0].contains("CSS diagnostic at byte"));
        assert!(loaded.diagnostics[1].contains("https://example.test/second.css"));
        assert!(loaded.diagnostics[1].contains("CSS diagnostic at byte"));
        assert!(
            loaded
                .stylesheets
                .iter()
                .all(|stylesheet| stylesheet.diagnostics.is_empty())
        );
        Ok(())
    }

    #[test]
    fn external_stylesheet_loading_can_use_shared_resource_cache() -> WebbyResult<()> {
        let document = webby_html::parse_document("<link rel=\"stylesheet\" href=\"site.css\">")?;
        let base = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let cache = webby_cache::ResourceCache::new();
        let loader = CountingStylesheetLoader::default();
        let cached_loader = webby_cache::CachedResourceLoader::new(&loader, &cache);

        load_external_stylesheets(&document, &base, &cached_loader);
        load_external_stylesheets(&document, &base, &cached_loader);

        assert_eq!(loader.calls.get(), 1);
        assert!(
            cache.contains_url(
                &url::Url::parse("https://example.test/site.css").map_err(url_error)?
            )
        );
        Ok(())
    }

    #[derive(Debug, Clone, Copy)]
    struct StylesheetLoader;

    impl ResourceLoader for StylesheetLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            if url.as_str().ends_with("/bad.css") {
                return Ok(ResourceResponse {
                    requested_url: url.clone(),
                    final_url: url.clone(),
                    status: Some(200),
                    content_type: Some("text/css".to_string()),
                    headers: Vec::new(),
                    bytes: b"p { color: red; } broken".to_vec(),
                });
            }

            Err(WebbyError::Network {
                message: format!("no test stylesheet for {url}"),
            })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct MultiBadStylesheetLoader;

    impl ResourceLoader for MultiBadStylesheetLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            Ok(ResourceResponse {
                requested_url: url.clone(),
                final_url: url.clone(),
                status: Some(200),
                content_type: Some("text/css".to_string()),
                headers: Vec::new(),
                bytes: format!("p {{ color: red; }} trailing for {url}").into_bytes(),
            })
        }
    }

    #[derive(Debug, Default)]
    struct CountingStylesheetLoader {
        calls: Cell<usize>,
    }

    impl ResourceLoader for CountingStylesheetLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
            self.calls.set(self.calls.get().saturating_add(1));
            Ok(ResourceResponse {
                requested_url: url.clone(),
                final_url: url.clone(),
                status: Some(200),
                content_type: Some("text/css".to_string()),
                headers: Vec::new(),
                bytes: b"p { color: red; }".to_vec(),
            })
        }
    }

    fn url_error(error: url::ParseError) -> WebbyError {
        WebbyError::Url {
            message: error.to_string(),
        }
    }
}
