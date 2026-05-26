//! Image reference collection, URL resolution, and PNG/JPEG decoding.
//!
//! This crate does not own resource loading transport. Callers provide a
//! `webby_net::ResourceLoader`, and this crate decodes successfully loaded
//! image bytes into deterministic RGBA pixels for layout and rendering.

use std::collections::BTreeMap;

use image::ImageReader;
use webby_core::{WebbyError, WebbyResult};
use webby_dom::{Document, Node, NodeKind};
use webby_net::ResourceLoader;

/// A decoded image in RGBA8 pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// Intrinsic pixel width.
    pub width: u32,
    /// Intrinsic pixel height.
    pub height: u32,
    /// RGBA8 pixels in row-major order.
    pub pixels: Vec<u8>,
}

/// A decoded image resource associated with an HTML `img src`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResource {
    /// Source string as written in the HTML attribute.
    pub src: String,
    /// Final resolved URL after loading.
    pub final_url: String,
    /// Decoded image pixels and intrinsic dimensions.
    pub image: DecodedImage,
}

/// An image reference collected from the DOM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageReference {
    /// Source string as written in the HTML attribute.
    pub src: String,
}

/// Decoded images keyed by the exact `src` attribute value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageMap {
    images: BTreeMap<String, ImageResource>,
}

impl ImageMap {
    /// Creates an empty image map.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Inserts a decoded resource.
    pub fn insert(&mut self, resource: ImageResource) {
        self.images.insert(resource.src.clone(), resource);
    }

    /// Returns a decoded image for an HTML `src` value.
    pub fn get(&self, src: &str) -> Option<&ImageResource> {
        self.images.get(src)
    }

    /// Number of decoded images.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Iterates decoded resources in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = &ImageResource> {
        self.images.values()
    }
}

/// Collects image-bearing references from a parsed document in DOM order.
///
/// This includes `img src` and `video poster`. Unsupported media sources stay
/// out of the image map so layout/render can use deterministic placeholders.
pub fn collect_image_references(document: &Document) -> Vec<ImageReference> {
    let mut references = Vec::new();
    collect_from_node(&document.root, &mut references);
    references
}

/// Resolves an image `src` against a document base URL.
pub fn resolve_image_url(base_url: &url::Url, src: &str) -> WebbyResult<url::Url> {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return Err(WebbyError::Url {
            message: "image src is empty".to_string(),
        });
    }

    base_url.join(trimmed).map_err(|error| WebbyError::Url {
        message: format!("invalid image URL {src:?}: {error}"),
    })
}

/// Loads and decodes all resolvable image references in a document.
///
/// Individual URL/load/decode failures are intentionally skipped so callers can
/// keep deterministic placeholders in layout/rendering.
pub fn load_images<L: ResourceLoader>(
    document: &Document,
    base_url: &url::Url,
    loader: &L,
) -> ImageMap {
    let mut images = ImageMap::empty();
    for reference in collect_image_references(document) {
        if images.get(&reference.src).is_some() {
            continue;
        }
        let Ok(url) = resolve_image_url(base_url, &reference.src) else {
            continue;
        };
        let response = if url.scheme() == "data" {
            webby_net::load_data_resource(&url)
        } else {
            loader.load(&url)
        };
        let Ok(response) = response else {
            continue;
        };
        let Ok(decoded) = decode_image(&response.bytes) else {
            continue;
        };
        images.insert(ImageResource {
            src: reference.src,
            final_url: response.final_url.to_string(),
            image: decoded,
        });
    }
    images
}

/// Converts decoded resources into layout-facing image metadata.
///
/// Layout intentionally does not depend on the decoder/loading crate, so app
/// and CLI composition code use this single adapter at the pipeline boundary.
pub fn to_layout_image_map(decoded: &ImageMap) -> webby_layout::ImageMap {
    let mut images = webby_layout::ImageMap::empty();
    for resource in decoded.iter() {
        images.insert(webby_layout::ImageResource {
            src: resource.src.clone(),
            final_url: resource.final_url.clone(),
            image: webby_layout::DecodedImage {
                width: resource.image.width,
                height: resource.image.height,
                pixels: resource.image.pixels.clone(),
            },
        });
    }
    images
}

/// Decodes PNG or JPEG bytes into RGBA8 pixels.
pub fn decode_image(bytes: &[u8]) -> WebbyResult<DecodedImage> {
    let reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| WebbyError::Parse {
            message: format!("failed to guess image format: {error}"),
        })?;
    let image = reader.decode().map_err(|error| WebbyError::Parse {
        message: format!("failed to decode image: {error}"),
    })?;
    let rgba = image.to_rgba8();

    Ok(DecodedImage {
        width: rgba.width(),
        height: rgba.height(),
        pixels: rgba.into_raw(),
    })
}

fn collect_from_node(node: &Node, references: &mut Vec<ImageReference>) {
    if let NodeKind::Element(element) = &node.kind {
        match element.tag_name.as_str() {
            "img" => {
                if let Some(src) = element.attributes.get("src") {
                    references.push(ImageReference { src: src.clone() });
                }
            }
            "video" => {
                if let Some(src) = element.attributes.get("poster") {
                    references.push(ImageReference { src: src.clone() });
                }
            }
            _ => {}
        }
    }

    for child in &node.children {
        collect_from_node(child, references);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DecodedImage, ImageMap, ImageResource, collect_image_references, decode_image,
        resolve_image_url,
    };
    use image::{ImageBuffer, ImageFormat, Rgba};
    use std::cell::Cell;
    use webby_core::{WebbyError, WebbyResult};

    #[test]
    fn img_src_is_preserved_from_html() -> WebbyResult<()> {
        let document = webby_html::parse_document(
            "<body><img src=\"images/logo.png\" alt=\"Logo\" width=\"10\" height=\"5\"></body>",
        )?;
        let references = collect_image_references(&document);

        assert_eq!(references.len(), 1);
        assert_eq!(references[0].src, "images/logo.png");
        Ok(())
    }

    #[test]
    fn video_poster_is_collected_as_image_reference() -> WebbyResult<()> {
        let document = webby_html::parse_document(
            "<body><video poster=\"poster.png\" src=\"movie.mp4\"></video></body>",
        )?;
        let references = collect_image_references(&document);

        assert_eq!(references.len(), 1);
        assert_eq!(references[0].src, "poster.png");
        Ok(())
    }

    #[test]
    fn relative_image_url_resolves_against_file_url() -> WebbyResult<()> {
        let base = url::Url::parse("file:///tmp/webby/pages/index.html").map_err(url_error)?;
        let resolved = resolve_image_url(&base, "../img/logo.png")?;

        assert_eq!(resolved.as_str(), "file:///tmp/webby/img/logo.png");
        Ok(())
    }

    #[test]
    fn relative_image_url_resolves_against_http_url() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/docs/index.html").map_err(url_error)?;
        let resolved = resolve_image_url(&base, "img/logo.png")?;

        assert_eq!(resolved.as_str(), "https://example.test/docs/img/logo.png");
        Ok(())
    }

    #[test]
    fn invalid_image_url_is_reported_for_empty_src() -> WebbyResult<()> {
        let base = url::Url::parse("https://example.test/").map_err(url_error)?;
        let result = resolve_image_url(&base, " ");

        assert!(matches!(result, Err(WebbyError::Url { .. })));
        Ok(())
    }

    #[test]
    fn invalid_image_url_becomes_empty_page_image_map() -> WebbyResult<()> {
        let document = webby_html::parse_document("<body><img src=\" \"></body>")?;
        let base = url::Url::parse("https://example.test/").map_err(url_error)?;
        let loader = EmptyLoader;

        let images = super::load_images(&document, &base, &loader);

        assert!(images.is_empty());
        Ok(())
    }

    #[test]
    fn resource_loading_failure_falls_back_to_empty_image_map() -> WebbyResult<()> {
        let document = webby_html::parse_document("<body><img src=\"logo.png\"></body>")?;
        let base = url::Url::parse("https://example.test/docs/").map_err(url_error)?;
        let loader = EmptyLoader;

        let images = super::load_images(&document, &base, &loader);

        assert!(images.is_empty());
        Ok(())
    }

    #[test]
    fn decode_failure_falls_back_to_empty_image_map() -> WebbyResult<()> {
        let document = webby_html::parse_document("<body><img src=\"logo.png\"></body>")?;
        let base = url::Url::parse("https://example.test/docs/").map_err(url_error)?;
        let loader = StaticBytesLoader {
            bytes: b"this is not an image".to_vec(),
        };

        let images = super::load_images(&document, &base, &loader);

        assert!(images.is_empty());
        Ok(())
    }

    #[test]
    fn image_loading_can_use_shared_resource_cache() -> WebbyResult<()> {
        let document = webby_html::parse_document("<body><img src=\"pixel.png\"></body>")?;
        let base = url::Url::parse("https://example.test/index.html").map_err(url_error)?;
        let image_url = url::Url::parse("https://example.test/pixel.png").map_err(url_error)?;
        let cache = webby_cache::ResourceCache::new();
        let loader = CountingImageLoader {
            bytes: encoded_test_image(ImageFormat::Png)?,
            calls: Cell::new(0),
        };
        let cached_loader = webby_cache::CachedResourceLoader::new(&loader, &cache);

        let first = super::load_images(&document, &base, &cached_loader);
        let second = super::load_images(&document, &base, &cached_loader);

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_eq!(loader.calls.get(), 1);
        assert!(cache.contains_url(&image_url));
        Ok(())
    }

    #[test]
    fn png_decode_works() -> WebbyResult<()> {
        let bytes = encoded_test_image(ImageFormat::Png)?;
        let decoded = decode_image(&bytes)?;

        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(&decoded.pixels[0..4], &[255, 0, 0, 255]);
        Ok(())
    }

    #[test]
    fn jpeg_decode_works() -> WebbyResult<()> {
        let bytes = encoded_test_image(ImageFormat::Jpeg)?;
        let decoded = decode_image(&bytes)?;

        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(decoded.pixels.len(), 2 * 4);
        Ok(())
    }

    #[test]
    fn image_map_uses_exact_src_key() {
        let mut map = ImageMap::empty();
        map.insert(ImageResource {
            src: "logo.png".to_string(),
            final_url: "https://example.test/logo.png".to_string(),
            image: DecodedImage {
                width: 1,
                height: 1,
                pixels: vec![1, 2, 3, 255],
            },
        });

        assert!(map.get("logo.png").is_some());
        assert!(map.get("./logo.png").is_none());
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

    fn url_error(error: url::ParseError) -> WebbyError {
        WebbyError::Url {
            message: error.to_string(),
        }
    }

    struct EmptyLoader;

    impl webby_net::ResourceLoader for EmptyLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<webby_net::ResourceResponse> {
            Err(WebbyError::Network {
                message: format!("unexpected test load for {url}"),
            })
        }
    }

    struct StaticBytesLoader {
        bytes: Vec<u8>,
    }

    impl webby_net::ResourceLoader for StaticBytesLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<webby_net::ResourceResponse> {
            Ok(webby_net::ResourceResponse {
                requested_url: url.clone(),
                final_url: url.clone(),
                status: Some(200),
                content_type: Some("application/octet-stream".to_string()),
                headers: Vec::new(),
                bytes: self.bytes.clone(),
            })
        }
    }

    struct CountingImageLoader {
        bytes: Vec<u8>,
        calls: Cell<usize>,
    }

    impl webby_net::ResourceLoader for CountingImageLoader {
        fn load(&self, url: &url::Url) -> WebbyResult<webby_net::ResourceResponse> {
            self.calls.set(self.calls.get().saturating_add(1));
            Ok(webby_net::ResourceResponse {
                requested_url: url.clone(),
                final_url: url.clone(),
                status: Some(200),
                content_type: Some("image/png".to_string()),
                headers: Vec::new(),
                bytes: self.bytes.clone(),
            })
        }
    }
}
