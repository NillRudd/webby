//! Resource loading for HTTP, HTTPS, and local files.

use std::fmt;
use std::fs;
use std::io::Read;
use std::time::Duration;

use base64::Engine;
use webby_core::{WebbyError, WebbyResult};

/// Maximum HTTP redirects followed by the default loader.
pub const MAX_REDIRECTS: usize = 10;
/// Default network timeout for connecting and reading responses.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
/// Maximum response body bytes retained by the default loader.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Metadata and bytes returned by a resource load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceResponse {
    /// URL requested by the caller.
    pub requested_url: url::Url,
    /// Final URL after redirects.
    pub final_url: url::Url,
    /// HTTP status when the resource came from HTTP(S).
    pub status: Option<u16>,
    /// Response content type when known.
    pub content_type: Option<String>,
    /// Response headers in deterministic source order where available.
    pub headers: Vec<(String, String)>,
    /// Response body bytes.
    pub bytes: Vec<u8>,
}

/// Browser-shell action suggested by top-level response metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationResponseDisposition {
    /// Render the response as an HTML document.
    RenderHtml,
    /// Save the response bytes instead of treating them as HTML.
    Download(DownloadMetadata),
}

/// Metadata used by a shell-managed download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadMetadata {
    /// Unsanitized filename suggested by headers or the final URL.
    pub suggested_filename: String,
    /// Deterministic explanation for why the response is downloaded.
    pub reason: String,
}

/// In-memory HTTP Basic credentials supplied by app/CLI coordination code.
#[derive(Clone, PartialEq, Eq)]
pub struct BasicCredentials {
    /// Username for the protected origin.
    pub username: String,
    /// Password for the protected origin.
    pub password: String,
}

impl fmt::Debug for BasicCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BasicCredentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl BasicCredentials {
    /// Creates credentials while keeping ownership explicit at the boundary.
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }

    /// Builds an `Authorization` header value. The returned string contains
    /// credentials and must not be logged directly.
    pub fn authorization_header_value(&self) -> String {
        let raw = format!("{}:{}", self.username, self.password);
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
        )
    }
}

impl ResourceResponse {
    /// Number of response body bytes.
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    /// Deterministic diagnostic summary for debugging network loads.
    pub fn network_diagnostic(&self) -> String {
        format!(
            "network requested={} final={} status={:?} content_type={:?} bytes={}",
            self.requested_url,
            self.final_url,
            self.status,
            self.content_type,
            self.bytes.len()
        )
    }

    /// Classifies whether a top-level response should render or download.
    pub fn navigation_disposition(&self) -> NavigationResponseDisposition {
        if let Some(value) = header_value(&self.headers, "content-disposition")
            && content_disposition_is_attachment(value)
        {
            return NavigationResponseDisposition::Download(DownloadMetadata {
                suggested_filename: suggested_download_filename(self, Some(value)),
                reason: "content-disposition attachment".to_string(),
            });
        }

        let content_type = self
            .content_type
            .as_deref()
            .or_else(|| sniff_content_type(&self.bytes))
            .map(media_type);
        if matches!(
            content_type.as_deref(),
            Some("text/html" | "application/xhtml+xml")
        ) {
            return NavigationResponseDisposition::RenderHtml;
        }

        NavigationResponseDisposition::Download(DownloadMetadata {
            suggested_filename: suggested_download_filename(self, None),
            reason: match content_type {
                Some(content_type) => format!("unsupported media type {content_type}"),
                None => "unknown media type".to_string(),
            },
        })
    }

    /// Returns whether the response asks for HTTP Basic authentication.
    pub fn basic_auth_challenge(&self) -> Option<BasicAuthChallenge> {
        (self.status == Some(401))
            .then(|| header_value(&self.headers, "www-authenticate"))
            .flatten()
            .and_then(parse_basic_auth_challenge)
    }
}

/// Redacted HTTP Basic challenge metadata suitable for app diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicAuthChallenge {
    /// Optional server-provided realm.
    pub realm: Option<String>,
}

impl BasicAuthChallenge {
    /// Deterministic redacted diagnostic text.
    pub fn diagnostic(&self, url: &url::Url) -> String {
        match &self.realm {
            Some(realm) => format!("HTTP Basic authentication required for {url} realm={realm:?}"),
            None => format!("HTTP Basic authentication required for {url}"),
        }
    }
}

/// Returns a complete `Authorization` request header for HTTP Basic auth.
pub fn basic_auth_header(credentials: &BasicCredentials) -> (String, String) {
    (
        "Authorization".to_string(),
        credentials.authorization_header_value(),
    )
}

/// Redacts sensitive authorization header values for deterministic diagnostics.
pub fn redact_request_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            if name.eq_ignore_ascii_case("authorization") {
                (name.clone(), "<redacted>".to_string())
            } else {
                (name.clone(), value.clone())
            }
        })
        .collect()
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find_map(|(candidate, value)| {
        candidate
            .eq_ignore_ascii_case(name)
            .then_some(value.as_str())
    })
}

fn media_type(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase()
}

fn content_disposition_is_attachment(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("attachment"))
}

fn suggested_download_filename(
    response: &ResourceResponse,
    content_disposition: Option<&str>,
) -> String {
    content_disposition
        .and_then(content_disposition_filename)
        .or_else(|| {
            response
                .final_url
                .path_segments()
                .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
                .map(ToOwned::to_owned)
        })
        .filter(|filename| !filename.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

fn content_disposition_filename(value: &str) -> Option<String> {
    value.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.trim().split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("filename")
            .then(|| {
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string()
            })
            .filter(|filename| !filename.is_empty())
    })
}

fn parse_basic_auth_challenge(value: &str) -> Option<BasicAuthChallenge> {
    let mut parts = value.split_whitespace();
    let scheme = parts.next()?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let remainder = parts.collect::<Vec<_>>().join(" ");
    let realm = auth_parameter(&remainder, "realm");
    Some(BasicAuthChallenge { realm })
}

fn auth_parameter(value: &str, parameter_name: &str) -> Option<String> {
    value.split(',').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        name.trim().eq_ignore_ascii_case(parameter_name).then(|| {
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
    })
}

/// Interface used by the app and CLI to load resources.
pub trait ResourceLoader {
    /// Loads a URL and returns response metadata plus bytes.
    fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse>;

    /// Loads a URL with request headers. Loaders that cannot send headers may
    /// use the default implementation.
    fn load_with_headers(
        &self,
        url: &url::Url,
        _headers: &[(String, String)],
    ) -> WebbyResult<ResourceResponse> {
        self.load(url)
    }

    /// Submits an `application/x-www-form-urlencoded` POST request. Loaders
    /// without write-capable transports may return a structured unsupported
    /// error.
    fn submit_form_urlencoded(
        &self,
        url: &url::Url,
        _headers: &[(String, String)],
        _body: &[u8],
    ) -> WebbyResult<ResourceResponse> {
        Err(WebbyError::unsupported(format!(
            "form POST is unsupported by this resource loader for {url}"
        )))
    }
}

/// Resource loader backed by `reqwest` for HTTP(S) and `std::fs` for files.
#[derive(Debug, Clone)]
pub struct DefaultResourceLoader {
    client: reqwest::blocking::Client,
}

impl DefaultResourceLoader {
    /// Creates a loader with conservative browser-prototype defaults.
    pub fn new() -> WebbyResult<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().iter().any(|url| url == attempt.url()) {
                    attempt.error("redirect loop detected")
                } else if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error("maximum redirect count exceeded")
                } else {
                    attempt.follow()
                }
            }))
            .user_agent("Webby/0.1")
            .gzip(true)
            .build()
            .map_err(|error| WebbyError::Network {
                message: format!("failed to create HTTP client: {error}"),
            })?;

        Ok(Self { client })
    }
}

impl ResourceLoader for DefaultResourceLoader {
    fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
        self.load_with_headers(url, &[])
    }

    fn load_with_headers(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
    ) -> WebbyResult<ResourceResponse> {
        match url.scheme() {
            "http" | "https" => self.load_http(url, headers),
            "file" => load_file(url),
            "data" => load_data_resource(url),
            scheme => Err(WebbyError::Url {
                message: format!("unsupported resource URL scheme: {scheme}"),
            }),
        }
    }

    fn submit_form_urlencoded(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
        body: &[u8],
    ) -> WebbyResult<ResourceResponse> {
        match url.scheme() {
            "http" | "https" => self.post_http(url, headers, body),
            scheme => Err(WebbyError::Url {
                message: format!("form POST is unsupported for URL scheme: {scheme}"),
            }),
        }
    }
}

impl DefaultResourceLoader {
    fn load_http(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
    ) -> WebbyResult<ResourceResponse> {
        let mut request = self.client.get(url.clone());
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.send().map_err(|error| WebbyError::Network {
            message: format!("failed to load {url}: {error:?}"),
        })?;

        let final_url = response.url().clone();
        let status = Some(response.status().as_u16());
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let headers = response_headers(response.headers());
        let bytes = read_bounded_response_body(response, &final_url)?;

        Ok(ResourceResponse {
            requested_url: url.clone(),
            final_url,
            status,
            content_type,
            headers,
            bytes,
        })
    }
}

impl DefaultResourceLoader {
    fn post_http(
        &self,
        url: &url::Url,
        headers: &[(String, String)],
        body: &[u8],
    ) -> WebbyResult<ResourceResponse> {
        let mut request = self
            .client
            .post(url.clone())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body.to_vec());
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.send().map_err(|error| WebbyError::Network {
            message: format!("failed to submit form to {url}: {error:?}"),
        })?;

        let final_url = response.url().clone();
        let status = Some(response.status().as_u16());
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let headers = response_headers(response.headers());
        let bytes = read_bounded_response_body(response, &final_url)?;

        Ok(ResourceResponse {
            requested_url: url.clone(),
            final_url,
            status,
            content_type,
            headers,
            bytes,
        })
    }
}

/// Loads a resource using the default loader.
pub fn load_resource(url: &url::Url) -> WebbyResult<ResourceResponse> {
    DefaultResourceLoader::new()?.load(url)
}

/// Decodes bytes as UTF-8, replacing invalid sequences instead of failing.
pub fn decode_text_utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Decodes text using HTTP content type first, then HTML `<meta charset>`.
///
/// Webby currently supports UTF-8 plus a deterministic Latin-1 compatible path
/// for `iso-8859-1`, `latin1`, and `windows-1252`. Other charsets fall back to
/// UTF-8 replacement.
pub fn decode_text(bytes: &[u8], content_type: Option<&str>) -> String {
    let charset = content_type
        .and_then(charset_from_content_type)
        .or_else(|| charset_from_html_meta(bytes));
    match charset.as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("iso-8859-1" | "latin1" | "latin-1" | "windows-1252") => {
            bytes.iter().map(|byte| char::from(*byte)).collect()
        }
        _ => decode_text_utf8(bytes),
    }
}

/// Sniffs common static resource content types from bytes.
pub fn sniff_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some("image/webp");
    }

    let leading = bytes
        .iter()
        .copied()
        .skip_while(|byte| byte.is_ascii_whitespace())
        .take(32)
        .collect::<Vec<_>>();
    let leading = String::from_utf8_lossy(&leading).to_ascii_lowercase();
    if leading.starts_with("<!doctype html") || leading.starts_with("<html") {
        return Some("text/html; charset=utf-8");
    }
    None
}

fn load_file(url: &url::Url) -> WebbyResult<ResourceResponse> {
    let path = url.to_file_path().map_err(|()| WebbyError::Url {
        message: format!("file URL cannot be converted to a local path: {url}"),
    })?;

    let bytes = fs::read(&path).map_err(|source| WebbyError::Io {
        path: Some(path.clone()),
        source,
    })?;

    Ok(ResourceResponse {
        requested_url: url.clone(),
        final_url: url.clone(),
        status: None,
        content_type: content_type_for_path(&path)
            .or_else(|| sniff_content_type(&bytes).map(str::to_string)),
        headers: Vec::new(),
        bytes,
    })
}

/// Loads bytes embedded in a `data:` URL.
pub fn load_data_resource(url: &url::Url) -> WebbyResult<ResourceResponse> {
    let raw = url
        .as_str()
        .strip_prefix("data:")
        .ok_or_else(|| WebbyError::Url {
            message: format!("invalid data URL: {url}"),
        })?;
    let Some((metadata, data)) = raw.split_once(',') else {
        return Err(WebbyError::Url {
            message: "invalid data URL: missing comma separator".to_string(),
        });
    };
    let mut content_type = None;
    let mut is_base64 = false;
    for (index, part) in metadata.split(';').enumerate() {
        if part.eq_ignore_ascii_case("base64") {
            is_base64 = true;
        } else if index == 0 && !part.is_empty() {
            content_type = Some(part.to_ascii_lowercase());
        }
    }

    let bytes = if is_base64 {
        base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|error| WebbyError::Parse {
                message: format!("invalid base64 data URL payload: {error}"),
            })?
    } else {
        urlencoding::decode_binary(data.as_bytes()).into_owned()
    };
    let content_type = content_type.or_else(|| sniff_content_type(&bytes).map(str::to_string));

    Ok(ResourceResponse {
        requested_url: url.clone(),
        final_url: url.clone(),
        status: None,
        content_type,
        headers: Vec::new(),
        bytes,
    })
}

fn response_headers(headers: &reqwest::header::HeaderMap) -> Vec<(String, String)> {
    let mut output = Vec::new();
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            output.push((name.as_str().to_ascii_lowercase(), value.to_string()));
        }
    }
    output
}

fn read_bounded_response_body(
    mut response: reqwest::blocking::Response,
    final_url: &url::Url,
) -> WebbyResult<Vec<u8>> {
    if let Some(length) = response.content_length()
        && length > MAX_RESPONSE_BYTES as u64
    {
        return Err(WebbyError::Network {
            message: format!(
                "response body from {final_url} is too large: {length} bytes exceeds {MAX_RESPONSE_BYTES}"
            ),
        });
    }
    let mut limited = response
        .by_ref()
        .take((MAX_RESPONSE_BYTES as u64).saturating_add(1));
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| WebbyError::Network {
            message: format!("failed to read response body from {final_url}: {error:?}"),
        })?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(WebbyError::Network {
            message: format!(
                "response body from {final_url} is too large: exceeded {MAX_RESPONSE_BYTES} bytes"
            ),
        });
    }
    Ok(bytes)
}

fn charset_from_content_type(content_type: &str) -> Option<String> {
    content_type.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        name.eq_ignore_ascii_case("charset").then(|| {
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_ascii_lowercase()
        })
    })
}

fn charset_from_html_meta(bytes: &[u8]) -> Option<String> {
    let prefix_len = bytes.len().min(2048);
    let prefix = String::from_utf8_lossy(&bytes[..prefix_len]).to_ascii_lowercase();
    let meta_index = prefix.find("<meta")?;
    let rest = &prefix[meta_index..];
    if let Some(charset_index) = rest.find("charset") {
        let after = &rest[charset_index + "charset".len()..];
        let after = after.trim_start();
        let after = after.strip_prefix('=')?.trim_start();
        let value = after
            .trim_start_matches(['"', '\''])
            .split(|character: char| {
                character.is_ascii_whitespace()
                    || character == '"'
                    || character == '\''
                    || character == '>'
                    || character == ';'
            })
            .next()?;
        if !value.is_empty() {
            return Some(value.to_ascii_lowercase());
        }
    }
    None
}

fn content_type_for_path(path: &std::path::Path) -> Option<String> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html" | "htm") => Some("text/html; charset=utf-8".to_string()),
        Some("txt") => Some("text/plain; charset=utf-8".to_string()),
        Some("css") => Some("text/css; charset=utf-8".to_string()),
        Some("js") => Some("text/javascript; charset=utf-8".to_string()),
        Some("png") => Some("image/png".to_string()),
        Some("jpg" | "jpeg") => Some("image/jpeg".to_string()),
        Some("gif") => Some("image/gif".to_string()),
        Some("webp") => Some("image/webp".to_string()),
        Some("ico") => Some("image/x-icon".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BasicAuthChallenge, BasicCredentials, DEFAULT_TIMEOUT, DefaultResourceLoader,
        DownloadMetadata, MAX_REDIRECTS, MAX_RESPONSE_BYTES, NavigationResponseDisposition,
        ResourceLoader, ResourceResponse, basic_auth_header, decode_text, decode_text_utf8,
        load_resource, redact_request_headers, sniff_content_type,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;
    use webby_core::WebbyError;

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    #[test]
    fn file_loader_reads_fixture_and_metadata() -> webby_core::WebbyResult<()> {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/simple.html");
        let url = url::Url::from_file_path(&fixture).map_err(|()| WebbyError::Url {
            message: "fixture path could not become file URL".to_string(),
        })?;

        let response = load_resource(&url)?;

        assert_eq!(response.requested_url, url);
        assert_eq!(response.final_url, url);
        assert_eq!(response.status, None);
        assert_eq!(
            response.content_type.as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert!(decode_text_utf8(&response.bytes).contains("Hello from Webby"));
        assert_eq!(response.byte_len(), response.bytes.len());
        Ok(())
    }

    #[test]
    fn loader_rejects_invalid_scheme() -> webby_core::WebbyResult<()> {
        let loader = DefaultResourceLoader::new()?;
        let url = url::Url::parse("ftp://example.com/").map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;

        let result = loader.load(&url);

        assert!(matches!(result, Err(WebbyError::Url { .. })));
        Ok(())
    }

    #[test]
    fn file_loader_reports_missing_file_as_io_error() -> webby_core::WebbyResult<()> {
        let missing_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/webby_missing_dir/nope.html");
        let url = url::Url::from_file_path(&missing_path).map_err(|()| WebbyError::Url {
            message: "missing fixture path could not become file URL".to_string(),
        })?;

        let result = load_resource(&url);

        assert!(matches!(result, Err(WebbyError::Io { .. })));
        Ok(())
    }

    #[test]
    fn file_loader_rejects_file_url_with_host() -> webby_core::WebbyResult<()> {
        let url = url::Url::parse("file://example.com/tmp/webby.html").map_err(|error| {
            WebbyError::Url {
                message: error.to_string(),
            }
        })?;

        let result = load_resource(&url);

        assert!(matches!(result, Err(WebbyError::Url { .. })));
        Ok(())
    }

    #[test]
    fn text_decoder_replaces_invalid_utf8() {
        let decoded = decode_text_utf8(&[b'W', b'e', 0xFF, b'b', b'y']);

        assert_eq!(decoded, "We\u{fffd}by");
    }

    #[test]
    fn data_url_loader_decodes_base64_payload_and_metadata() -> webby_core::WebbyResult<()> {
        let url = url::Url::parse("data:text/plain;base64,V2ViYnk=").map_err(|error| {
            WebbyError::Url {
                message: error.to_string(),
            }
        })?;

        let response = load_resource(&url)?;

        assert_eq!(response.requested_url, url);
        assert_eq!(response.final_url, url);
        assert_eq!(response.content_type.as_deref(), Some("text/plain"));
        assert_eq!(decode_text_utf8(&response.bytes), "Webby");
        Ok(())
    }

    #[test]
    fn invalid_data_url_is_structured_error() -> webby_core::WebbyResult<()> {
        let url = url::Url::parse("data:text/plain;base64,not-valid!").map_err(|error| {
            WebbyError::Url {
                message: error.to_string(),
            }
        })?;

        let result = load_resource(&url);

        assert!(matches!(result, Err(WebbyError::Parse { .. })));
        Ok(())
    }

    #[test]
    fn content_type_sniffing_recognizes_common_image_bytes_without_panicking() {
        assert_eq!(
            sniff_content_type(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(
            sniff_content_type(&[0xFF, 0xD8, 0xFF, 0x00]),
            Some("image/jpeg")
        );
        assert_eq!(sniff_content_type(b"GIF89a..."), Some("image/gif"));
        assert_eq!(sniff_content_type(b"RIFFxxxxWEBPrest"), Some("image/webp"));
        assert_eq!(sniff_content_type(&[0, 1, 2, 3, 4]), None);
    }

    #[test]
    fn http_loader_reads_response_metadata() -> TestResult {
        let (url, server) = spawn_http_server(vec![HttpResponse {
            status_line: "HTTP/1.1 200 OK",
            headers: vec![
                ("Content-Type", "text/plain; charset=utf-8"),
                ("Set-Cookie", "sid=abc; Path=/"),
            ],
            body: b"Hello over HTTP",
        }])?;

        let response = load_resource(&url)?;

        assert_eq!(response.requested_url, url);
        assert_eq!(response.final_url, url);
        assert_eq!(response.status, Some(200));
        assert_eq!(
            response.content_type.as_deref(),
            Some("text/plain; charset=utf-8")
        );
        assert!(
            response
                .headers
                .iter()
                .any(|(name, value)| name == "set-cookie" && value == "sid=abc; Path=/")
        );
        assert_eq!(decode_text_utf8(&response.bytes), "Hello over HTTP");
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn http_loader_sends_explicit_cookie_header() -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let url = url::Url::parse(&format!("http://{address}/"))?;
        let server = thread::spawn(move || -> TestResult {
            let (mut stream, _) = listener.accept()?;
            let mut request_buffer = [0; 2048];
            let bytes_read = stream.read(&mut request_buffer)?;
            let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
            if !request.to_ascii_lowercase().contains("cookie: sid=abc") {
                return Err("missing Cookie request header".into());
            }
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
            stream.write_all(response.as_bytes())?;
            Ok(())
        });
        let loader = DefaultResourceLoader::new()?;

        let response =
            loader.load_with_headers(&url, &[("Cookie".to_string(), "sid=abc".to_string())])?;

        assert_eq!(decode_text_utf8(&response.bytes), "ok");
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn http_loader_returns_404_metadata() -> TestResult {
        let (url, server) = spawn_http_server(vec![HttpResponse {
            status_line: "HTTP/1.1 404 Not Found",
            headers: vec![("Content-Type", "text/plain")],
            body: b"Missing",
        }])?;

        let response = load_resource(&url)?;

        assert_eq!(response.status, Some(404));
        assert_eq!(response.content_type.as_deref(), Some("text/plain"));
        assert_eq!(decode_text_utf8(&response.bytes), "Missing");
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn http_loader_follows_redirects() -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let final_url = url::Url::parse(&format!("http://{address}/final"))?;
        let requested_url = url::Url::parse(&format!("http://{address}/start"))?;
        let server = thread::spawn(move || -> TestResult {
            respond_once(
                &listener,
                HttpResponse {
                    status_line: "HTTP/1.1 302 Found",
                    headers: vec![("Location", "/final")],
                    body: b"",
                },
            )?;
            respond_once(
                &listener,
                HttpResponse {
                    status_line: "HTTP/1.1 200 OK",
                    headers: vec![("Content-Type", "text/plain")],
                    body: b"Redirected",
                },
            )?;
            Ok(())
        });

        let response = load_resource(&requested_url)?;

        assert_eq!(response.requested_url, requested_url);
        assert_eq!(response.final_url, final_url);
        assert_eq!(response.status, Some(200));
        assert_eq!(decode_text_utf8(&response.bytes), "Redirected");
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn post_redirect_307_preserves_method_and_body() -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let requested_url = url::Url::parse(&format!("http://{address}/submit"))?;
        let final_url = url::Url::parse(&format!("http://{address}/posted"))?;
        let server = thread::spawn(move || -> TestResult {
            respond_once(
                &listener,
                HttpResponse {
                    status_line: "HTTP/1.1 307 Temporary Redirect",
                    headers: vec![("Location", "/posted")],
                    body: b"",
                },
            )?;
            let (mut stream, _) = listener.accept()?;
            let mut request_buffer = [0; 2048];
            let bytes_read = stream.read(&mut request_buffer)?;
            let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
            if !request.starts_with("POST /posted ") || !request.contains("q=webby") {
                return Err(
                    format!("redirected POST did not preserve method/body: {request}").into(),
                );
            }
            let response =
                "HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nposted";
            stream.write_all(response.as_bytes())?;
            Ok(())
        });
        let loader = DefaultResourceLoader::new()?;

        let response = loader.submit_form_urlencoded(&requested_url, &[], b"q=webby")?;

        assert_eq!(response.final_url, final_url);
        assert_eq!(decode_text_utf8(&response.bytes), "posted");
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn redirect_loop_fails_gracefully() -> TestResult {
        let (url, server) = spawn_http_server(vec![HttpResponse {
            status_line: "HTTP/1.1 302 Found",
            headers: vec![("Location", "/")],
            body: b"",
        }])?;

        let result = load_resource(&url);

        assert!(matches!(result, Err(WebbyError::Network { .. })));
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn gzip_response_loads_decompressed_bytes() -> TestResult {
        let gzip_body = &[
            31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 115, 206, 207, 45, 40, 74, 45, 46, 78, 77, 81, 8,
            79, 77, 74, 170, 4, 0, 73, 224, 30, 18, 16, 0, 0, 0,
        ];
        let (url, server) = spawn_http_server(vec![HttpResponse {
            status_line: "HTTP/1.1 200 OK",
            headers: vec![("Content-Encoding", "gzip"), ("Content-Type", "text/plain")],
            body: gzip_body,
        }])?;

        let response = load_resource(&url)?;

        assert_eq!(decode_text_utf8(&response.bytes), "Compressed Webby");
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn oversized_http_response_is_structured_error() -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let url = url::Url::parse(&format!("http://{address}/huge"))?;
        let server = thread::spawn(move || -> TestResult {
            let (mut stream, _) = listener.accept()?;
            let mut request_buffer = [0; 1024];
            let _bytes_read = stream.read(&mut request_buffer)?;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_RESPONSE_BYTES + 1
            );
            stream.write_all(response.as_bytes())?;
            Ok(())
        });

        let result = load_resource(&url);

        assert!(matches!(result, Err(WebbyError::Network { .. })));
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn charset_decoding_uses_header_then_html_meta() {
        assert_eq!(
            decode_text(b"Caf\xe9", Some("text/html; charset=iso-8859-1")),
            "Café"
        );
        assert_eq!(
            decode_text(
                b"<meta charset=\"latin1\"><p>Caf\xe9</p>",
                Some("text/html")
            ),
            "<meta charset=\"latin1\"><p>Café</p>"
        );
    }

    #[test]
    fn network_diagnostic_is_deterministic_and_useful() -> webby_core::WebbyResult<()> {
        let url = url::Url::parse("data:text/plain,hello").map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;
        let response = load_resource(&url)?;

        assert_eq!(
            response.network_diagnostic(),
            format!(
                "network requested={url} final={url} status=None content_type=Some(\"text/plain\") bytes=5"
            )
        );
        Ok(())
    }

    #[test]
    fn navigation_disposition_renders_html_and_downloads_unsupported_media() -> TestResult {
        let html_url = url::Url::parse("https://example.test/index.html")?;
        let html = ResourceResponse {
            requested_url: html_url.clone(),
            final_url: html_url,
            status: Some(200),
            content_type: Some("text/html; charset=utf-8".to_string()),
            headers: Vec::new(),
            bytes: b"<html></html>".to_vec(),
        };
        assert_eq!(
            html.navigation_disposition(),
            NavigationResponseDisposition::RenderHtml
        );

        let binary_url = url::Url::parse("https://example.test/files/report.pdf")?;
        let binary = ResourceResponse {
            requested_url: binary_url.clone(),
            final_url: binary_url,
            status: Some(200),
            content_type: Some("application/pdf".to_string()),
            headers: Vec::new(),
            bytes: b"%PDF".to_vec(),
        };
        assert_eq!(
            binary.navigation_disposition(),
            NavigationResponseDisposition::Download(DownloadMetadata {
                suggested_filename: "report.pdf".to_string(),
                reason: "unsupported media type application/pdf".to_string(),
            })
        );
        Ok(())
    }

    #[test]
    fn content_disposition_attachment_downloads_with_header_filename() -> TestResult {
        let url = url::Url::parse("https://example.test/export")?;
        let response = ResourceResponse {
            requested_url: url.clone(),
            final_url: url,
            status: Some(200),
            content_type: Some("text/html".to_string()),
            headers: vec![(
                "Content-Disposition".to_string(),
                "attachment; filename=\"quarterly report.csv\"".to_string(),
            )],
            bytes: b"<p>not a page</p>".to_vec(),
        };
        assert_eq!(
            response.navigation_disposition(),
            NavigationResponseDisposition::Download(DownloadMetadata {
                suggested_filename: "quarterly report.csv".to_string(),
                reason: "content-disposition attachment".to_string(),
            })
        );
        Ok(())
    }

    #[test]
    fn basic_auth_challenge_and_header_are_deterministic_and_redactable() -> TestResult {
        let url = url::Url::parse("https://example.test/private")?;
        let response = ResourceResponse {
            requested_url: url.clone(),
            final_url: url.clone(),
            status: Some(401),
            content_type: Some("text/html".to_string()),
            headers: vec![(
                "WWW-Authenticate".to_string(),
                "Basic realm=\"Members\"".to_string(),
            )],
            bytes: Vec::new(),
        };
        assert_eq!(
            response.basic_auth_challenge(),
            Some(BasicAuthChallenge {
                realm: Some("Members".to_string()),
            })
        );
        assert_eq!(
            response
                .basic_auth_challenge()
                .map(|challenge| challenge.diagnostic(&url)),
            Some("HTTP Basic authentication required for https://example.test/private realm=\"Members\"".to_string())
        );

        let header = basic_auth_header(&BasicCredentials::new("webby", "secret"));
        assert_eq!(header.0, "Authorization");
        assert_eq!(
            redact_request_headers(std::slice::from_ref(&header)),
            vec![("Authorization".to_string(), "<redacted>".to_string())]
        );
        let credentials_debug = format!("{:?}", BasicCredentials::new("webby", "secret"));
        assert!(credentials_debug.contains("webby"));
        assert!(credentials_debug.contains("<redacted>"));
        assert!(!credentials_debug.contains("secret"));
        assert!(!format!("{:?}", redact_request_headers(&[header])).contains("secret"));
        Ok(())
    }

    #[test]
    fn http_basic_auth_test_server_accepts_user_credentials() -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let url = url::Url::parse(&format!("http://{address}/private"))?;
        let expected = BasicCredentials::new("webby", "secret").authorization_header_value();
        let server = thread::spawn(move || -> TestResult {
            let (mut stream, _) = listener.accept()?;
            let mut request_buffer = [0; 2048];
            let bytes_read = stream.read(&mut request_buffer)?;
            let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
            let expected_line = format!("authorization: {expected}");
            let authorized = request
                .lines()
                .map(str::to_ascii_lowercase)
                .any(|line| line == expected_line.to_ascii_lowercase());
            if !authorized {
                let response = "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"Members\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(response.as_bytes())?;
                return Ok(());
            }
            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 20\r\nConnection: close\r\n\r\n<body>Private</body>";
            stream.write_all(response.as_bytes())?;
            Ok(())
        });
        let loader = DefaultResourceLoader::new()?;

        let response = loader.load_with_headers(
            &url,
            &[basic_auth_header(&BasicCredentials::new("webby", "secret"))],
        )?;

        assert_eq!(response.status, Some(200));
        assert_eq!(decode_text_utf8(&response.bytes), "<body>Private</body>");
        server.join().map_err(|_| "HTTP server thread panicked")??;
        Ok(())
    }

    #[test]
    fn timeout_and_size_limits_are_documented_constants() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(15));
        assert_eq!(MAX_RESPONSE_BYTES, 8 * 1024 * 1024);
        assert_eq!(MAX_REDIRECTS, 10);
    }

    struct HttpResponse {
        status_line: &'static str,
        headers: Vec<(&'static str, &'static str)>,
        body: &'static [u8],
    }

    fn spawn_http_server(
        responses: Vec<HttpResponse>,
    ) -> Result<(url::Url, thread::JoinHandle<TestResult>), Box<dyn std::error::Error + Send + Sync>>
    {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let url = url::Url::parse(&format!("http://{address}/"))?;
        let server = thread::spawn(move || -> TestResult {
            for response in responses {
                respond_once(&listener, response)?;
            }
            Ok(())
        });

        Ok((url, server))
    }

    fn respond_once(listener: &TcpListener, response: HttpResponse) -> TestResult {
        let (mut stream, _) = listener.accept()?;
        let mut request_buffer = [0; 1024];
        let _bytes_read = stream.read(&mut request_buffer)?;
        let body = response.body;
        let mut raw_response = format!(
            "{}\r\nContent-Length: {}\r\nConnection: close\r\n",
            response.status_line,
            body.len()
        );

        for (name, value) in response.headers {
            raw_response.push_str(&format!("{name}: {value}\r\n"));
        }

        raw_response.push_str("\r\n");
        stream.write_all(raw_response.as_bytes())?;
        stream.write_all(body)?;
        Ok(())
    }
}
