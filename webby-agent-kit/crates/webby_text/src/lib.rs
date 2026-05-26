//! Shared deterministic text measurement and glyph rasterization.
//!
//! This crate owns Webby's text metrics so layout and rendering cannot drift.
//! It uses `fontdue` with a small system-font search path. If no usable font is
//! available, Webby falls back to a deterministic built-in bitmap-like glyph
//! path so the engine remains functional and testable.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use fontdue::Font;
use webby_core::{WebbyError, WebbyResult};

const FALLBACK_ADVANCE_FACTOR: f32 = 0.62;
const FALLBACK_HEIGHT_FACTOR: f32 = 1.0;
const LINE_HEIGHT_FACTOR: f32 = 1.2;
const BOLD_WIDTH_FACTOR: f32 = 1.04;

/// Text font weight used by layout and rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// Normal font weight.
    Normal,
    /// Bold text. When no bold font is available, width and rasterization get a
    /// deterministic synthetic bold approximation.
    Bold,
}

/// Text family category used by layout and rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    /// Default proportional/sans text path.
    Sans,
    /// Deterministic monospace fallback path for code/preformatted text.
    Monospace,
}

/// Measured text extents in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    /// Measured advance width.
    pub width: f32,
    /// Recommended line-height box height.
    pub height: f32,
}

/// A positioned glyph bitmap relative to the top-left of the text run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphBitmap {
    /// Left offset in pixels.
    pub x: i32,
    /// Top offset in pixels.
    pub y: i32,
    /// Bitmap width.
    pub width: usize,
    /// Bitmap height.
    pub height: usize,
    /// Alpha coverage values in row-major order.
    pub coverage: Vec<u8>,
}

/// Text rasterization output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterizedText {
    /// Positioned glyph bitmaps.
    pub glyphs: Vec<GlyphBitmap>,
}

/// Source of the active text font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontSource {
    /// A system font loaded from a known path.
    System(PathBuf),
    /// No system font was available; deterministic fallback is active.
    BuiltInFallback,
}

/// Shared text engine.
pub struct TextEngine {
    font: Option<Font>,
    source: FontSource,
}

impl TextEngine {
    /// Loads the default text engine.
    pub fn load_default() -> Self {
        for path in candidate_font_paths() {
            if let Ok(engine) = Self::from_font_path(&path) {
                return engine;
            }
        }

        Self {
            font: None,
            source: FontSource::BuiltInFallback,
        }
    }

    /// Loads a text engine from an explicit font path.
    pub fn from_font_path(path: &Path) -> WebbyResult<Self> {
        let bytes = std::fs::read(path).map_err(|source| WebbyError::Io {
            path: Some(path.to_path_buf()),
            source,
        })?;
        let font = Font::from_bytes(bytes, fontdue::FontSettings::default()).map_err(|error| {
            WebbyError::Render {
                message: format!("failed to load font {}: {error}", path.display()),
            }
        })?;

        Ok(Self {
            font: Some(font),
            source: FontSource::System(path.to_path_buf()),
        })
    }

    /// Creates the deterministic fallback engine.
    pub fn fallback() -> Self {
        Self {
            font: None,
            source: FontSource::BuiltInFallback,
        }
    }

    /// Returns the active font source.
    pub fn source(&self) -> &FontSource {
        &self.source
    }

    /// Measures text with the active engine.
    pub fn measure(&self, text: &str, font_size: f32, font_weight: FontWeight) -> TextMetrics {
        self.measure_with_family(text, font_size, font_weight, FontFamily::Sans)
    }

    /// Measures text with a selected family category.
    pub fn measure_with_family(
        &self,
        text: &str,
        font_size: f32,
        font_weight: FontWeight,
        font_family: FontFamily,
    ) -> TextMetrics {
        let size = sanitize_font_size(font_size);
        let width = if font_family == FontFamily::Monospace {
            measure_monospace_width(text, size, font_weight)
        } else {
            match &self.font {
                Some(font) => measure_fontdue_width(font, text, size, font_weight),
                None => measure_fallback_width(text, size, font_weight),
            }
        };

        TextMetrics {
            width,
            height: line_height(size),
        }
    }

    /// Rasterizes text with the active engine.
    pub fn rasterize(&self, text: &str, font_size: f32, font_weight: FontWeight) -> RasterizedText {
        self.rasterize_with_family(text, font_size, font_weight, FontFamily::Sans)
    }

    /// Rasterizes text with a selected family category.
    pub fn rasterize_with_family(
        &self,
        text: &str,
        font_size: f32,
        font_weight: FontWeight,
        font_family: FontFamily,
    ) -> RasterizedText {
        let size = sanitize_font_size(font_size);
        if font_family == FontFamily::Monospace {
            rasterize_monospace(text, size, font_weight)
        } else {
            match &self.font {
                Some(font) => rasterize_fontdue(font, text, size, font_weight),
                None => rasterize_fallback(text, size, font_weight),
            }
        }
    }
}

/// Returns the process-wide default text engine.
pub fn default_text_engine() -> &'static TextEngine {
    static ENGINE: OnceLock<TextEngine> = OnceLock::new();
    ENGINE.get_or_init(TextEngine::load_default)
}

/// Measures text using the default text engine.
pub fn measure_text(text: &str, font_size: f32, font_weight: FontWeight) -> TextMetrics {
    default_text_engine().measure(text, font_size, font_weight)
}

/// Measures text using the default text engine and a selected family category.
pub fn measure_text_with_family(
    text: &str,
    font_size: f32,
    font_weight: FontWeight,
    font_family: FontFamily,
) -> TextMetrics {
    default_text_engine().measure_with_family(text, font_size, font_weight, font_family)
}

/// Rasterizes text using the default text engine and a selected family category.
pub fn rasterize_text_with_family(
    text: &str,
    font_size: f32,
    font_weight: FontWeight,
    font_family: FontFamily,
) -> RasterizedText {
    default_text_engine().rasterize_with_family(text, font_size, font_weight, font_family)
}

/// Returns Webby's shared line-height for a font size.
pub fn line_height(font_size: f32) -> f32 {
    sanitize_font_size(font_size) * LINE_HEIGHT_FACTOR
}

/// Returns the active default font source.
pub fn default_font_source() -> &'static FontSource {
    default_text_engine().source()
}

/// Returns deterministic fallback metrics without touching system fonts.
pub fn fallback_measure_text(text: &str, font_size: f32, font_weight: FontWeight) -> TextMetrics {
    TextEngine::fallback().measure(text, font_size, font_weight)
}

fn candidate_font_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("WEBBY_FONT_PATH") {
        candidates.push(PathBuf::from(path));
    }

    candidates.extend([
        PathBuf::from("/System/Library/Fonts/SFNS.ttf"),
        PathBuf::from("/System/Library/Fonts/SFNSMono.ttf"),
        PathBuf::from("/System/Library/Fonts/Geneva.ttf"),
        PathBuf::from("/Library/Fonts/Arial.ttf"),
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        PathBuf::from("/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf"),
        PathBuf::from("/usr/share/fonts/TTF/DejaVuSans.ttf"),
    ]);

    candidates
}

fn measure_fontdue_width(font: &Font, text: &str, font_size: f32, font_weight: FontWeight) -> f32 {
    let width = text
        .chars()
        .map(|character| font.metrics(character, font_size).advance_width.max(0.0))
        .sum::<f32>();
    apply_weight_width(width, font_weight)
}

fn measure_fallback_width(text: &str, font_size: f32, font_weight: FontWeight) -> f32 {
    let width = text
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                font_size * FALLBACK_ADVANCE_FACTOR * 0.55
            } else {
                font_size * FALLBACK_ADVANCE_FACTOR
            }
        })
        .sum::<f32>();
    apply_weight_width(width, font_weight)
}

fn measure_monospace_width(text: &str, font_size: f32, font_weight: FontWeight) -> f32 {
    text.chars().count() as f32 * font_size * FALLBACK_ADVANCE_FACTOR * weight_factor(font_weight)
}

fn apply_weight_width(width: f32, font_weight: FontWeight) -> f32 {
    match font_weight {
        FontWeight::Normal => width,
        FontWeight::Bold => width * BOLD_WIDTH_FACTOR,
    }
}

fn rasterize_fontdue(
    font: &Font,
    text: &str,
    font_size: f32,
    font_weight: FontWeight,
) -> RasterizedText {
    let mut glyphs = Vec::new();
    let mut cursor_x = 0.0_f32;
    let baseline = font_size * 0.86;
    let bold_offset = match font_weight {
        FontWeight::Normal => 0,
        FontWeight::Bold => 1,
    };

    for character in text.chars() {
        let metrics = font.metrics(character, font_size);
        let advance = apply_weight_width(metrics.advance_width.max(0.0), font_weight);

        if !character.is_whitespace() {
            let (raster_metrics, bitmap) = font.rasterize(character, font_size);
            if !bitmap.is_empty() && raster_metrics.width > 0 && raster_metrics.height > 0 {
                let x = cursor_x.round() as i32 + raster_metrics.xmin;
                let y = (baseline - raster_metrics.ymin as f32 - raster_metrics.height as f32)
                    .round() as i32;
                glyphs.push(GlyphBitmap {
                    x,
                    y,
                    width: raster_metrics.width + bold_offset,
                    height: raster_metrics.height,
                    coverage: bolden_bitmap(
                        bitmap,
                        raster_metrics.width,
                        raster_metrics.height,
                        bold_offset,
                    ),
                });
            }
        }

        cursor_x += advance;
    }

    RasterizedText { glyphs }
}

fn bolden_bitmap(bitmap: Vec<u8>, width: usize, height: usize, extra: usize) -> Vec<u8> {
    if extra == 0 || width == 0 || height == 0 {
        return bitmap;
    }

    let new_width = width.saturating_add(extra);
    let Some(len) = new_width.checked_mul(height) else {
        return bitmap;
    };
    let mut output = vec![0; len];
    for y in 0..height {
        for x in 0..width {
            let Some(source_index) = y.checked_mul(width).and_then(|row| row.checked_add(x)) else {
                continue;
            };
            let Some(destination_index) =
                y.checked_mul(new_width).and_then(|row| row.checked_add(x))
            else {
                continue;
            };
            let Some(alpha) = bitmap.get(source_index).copied() else {
                continue;
            };
            if let Some(pixel) = output.get_mut(destination_index) {
                *pixel = (*pixel).max(alpha);
            }
            if let Some(pixel) = output.get_mut(destination_index.saturating_add(1)) {
                *pixel = (*pixel).max(alpha);
            }
        }
    }
    output
}

fn rasterize_fallback(text: &str, font_size: f32, font_weight: FontWeight) -> RasterizedText {
    let glyph_width = (font_size * FALLBACK_ADVANCE_FACTOR).round().max(1.0) as usize;
    let glyph_height = (font_size * FALLBACK_HEIGHT_FACTOR).round().max(1.0) as usize;
    let space_width = (glyph_width as f32 * 0.55).round().max(1.0);
    let bold_extra = usize::from(matches!(font_weight, FontWeight::Bold));
    let mut cursor_x = 0.0_f32;
    let mut glyphs = Vec::new();

    for character in text.chars() {
        if character.is_whitespace() {
            cursor_x += space_width;
            continue;
        }

        let width = glyph_width.saturating_add(bold_extra);
        let Some(len) = width.checked_mul(glyph_height) else {
            cursor_x += glyph_width as f32;
            continue;
        };
        let mut coverage = vec![0; len];
        fill_fallback_glyph(&mut coverage, width, glyph_height, character);
        glyphs.push(GlyphBitmap {
            x: cursor_x.round() as i32,
            y: 0,
            width,
            height: glyph_height,
            coverage,
        });
        cursor_x += apply_weight_width(glyph_width as f32, font_weight);
    }

    RasterizedText { glyphs }
}

fn rasterize_monospace(text: &str, font_size: f32, font_weight: FontWeight) -> RasterizedText {
    let advance = font_size * FALLBACK_ADVANCE_FACTOR * weight_factor(font_weight);
    let glyph_width = advance.round().max(1.0) as usize;
    let glyph_height = (font_size * FALLBACK_HEIGHT_FACTOR).round().max(1.0) as usize;
    let mut cursor_x = 0.0_f32;
    let mut glyphs = Vec::new();

    for character in text.chars() {
        if !character.is_whitespace() {
            let Some(len) = glyph_width.checked_mul(glyph_height) else {
                cursor_x += advance;
                continue;
            };
            let mut coverage = vec![0; len];
            fill_fallback_glyph(&mut coverage, glyph_width, glyph_height, character);
            glyphs.push(GlyphBitmap {
                x: cursor_x.round() as i32,
                y: 0,
                width: glyph_width,
                height: glyph_height,
                coverage,
            });
        }
        cursor_x += advance;
    }

    RasterizedText { glyphs }
}

fn weight_factor(font_weight: FontWeight) -> f32 {
    match font_weight {
        FontWeight::Normal => 1.0,
        FontWeight::Bold => BOLD_WIDTH_FACTOR,
    }
}

fn fill_fallback_glyph(coverage: &mut [u8], width: usize, height: usize, character: char) {
    for y in 0..height {
        for x in 0..width {
            let border = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
            let diagonal =
                width > 2 && height > 2 && (x + y + character as usize).is_multiple_of(5);
            if border || diagonal {
                let Some(index) = y.checked_mul(width).and_then(|row| row.checked_add(x)) else {
                    continue;
                };
                if let Some(pixel) = coverage.get_mut(index) {
                    *pixel = 220;
                }
            }
        }
    }
}

fn sanitize_font_size(font_size: f32) -> f32 {
    if font_size.is_finite() && font_size > 0.0 {
        font_size
    } else {
        16.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FontFamily, FontSource, FontWeight, TextEngine, default_font_source, fallback_measure_text,
        measure_text, measure_text_with_family, rasterize_text_with_family,
    };

    #[test]
    fn text_measurement_is_deterministic() {
        let first = measure_text("Webby text", 16.0, FontWeight::Normal);
        let second = measure_text("Webby text", 16.0, FontWeight::Normal);

        assert_eq!(first, second);
    }

    #[test]
    fn larger_font_size_produces_larger_metrics() {
        let small = measure_text("Webby", 12.0, FontWeight::Normal);
        let large = measure_text("Webby", 24.0, FontWeight::Normal);

        assert!(large.width > small.width);
        assert!(large.height > small.height);
    }

    #[test]
    fn bold_measurement_is_not_narrower_than_normal() {
        let normal = measure_text("Webby", 16.0, FontWeight::Normal);
        let bold = measure_text("Webby", 16.0, FontWeight::Bold);

        assert!(bold.width >= normal.width);
    }

    #[test]
    fn fallback_metrics_are_available_without_system_font() {
        let metrics = fallback_measure_text("fallback", 18.0, FontWeight::Normal);

        assert!(metrics.width > 0.0);
        assert!(metrics.height > 0.0);
    }

    #[test]
    fn fallback_engine_rasterizes_text() {
        let rasterized = TextEngine::fallback().rasterize("A", 16.0, FontWeight::Normal);

        assert!(!rasterized.glyphs.is_empty());
        assert!(rasterized.glyphs[0].coverage.iter().any(|alpha| *alpha > 0));
    }

    #[test]
    fn monospace_spaces_use_the_same_cell_advance_as_letters() {
        let letter = measure_text_with_family("A", 16.0, FontWeight::Normal, FontFamily::Monospace);
        let space = measure_text_with_family(" ", 16.0, FontWeight::Normal, FontFamily::Monospace);
        let rasterized =
            rasterize_text_with_family("A A", 16.0, FontWeight::Normal, FontFamily::Monospace);

        assert_eq!(letter.width, space.width);
        assert_eq!(rasterized.glyphs.len(), 2);
        assert_eq!(rasterized.glyphs[1].x, (letter.width * 2.0).round() as i32);
    }

    #[test]
    fn default_font_fallback_status_is_explicit() {
        assert!(matches!(
            default_font_source(),
            FontSource::System(_) | FontSource::BuiltInFallback
        ));
    }
}
