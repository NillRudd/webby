//! Resource loading for HTTP, HTTPS, and local files.

use std::fs;
use std::time::Duration;

use base64::Engine;
use webby_core::{WebbyError, WebbyResult};

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

impl ResourceResponse {
    /// Number of response body bytes.
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
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
            .timeout(Duration::from_secs(15))
            .user_agent("Webby/0.1")
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
        let bytes = response
            .bytes()
            .map_err(|error| WebbyError::Network {
                message: format!("failed to read response body from {final_url}: {error:?}"),
            })?
            .to_vec();

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
        let bytes = response
            .bytes()
            .map_err(|error| WebbyError::Network {
                message: format!("failed to read response body from {final_url}: {error:?}"),
            })?
            .to_vec();

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
        DefaultResourceLoader, ResourceLoader, decode_text_utf8, load_resource, sniff_content_type,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
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
            body: "Hello over HTTP",
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
            body: "Missing",
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
                    body: "",
                },
            )?;
            respond_once(
                &listener,
                HttpResponse {
                    status_line: "HTTP/1.1 200 OK",
                    headers: vec![("Content-Type", "text/plain")],
                    body: "Redirected",
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

    struct HttpResponse {
        status_line: &'static str,
        headers: Vec<(&'static str, &'static str)>,
        body: &'static str,
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
        let body = response.body.as_bytes();
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
