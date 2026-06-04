//! Display-list construction and deterministic software rendering.
//!
//! `webby_render` consumes `webby_layout` output only. It does not parse HTML,
//! fetch resources, or own browser navigation state.

use webby_core::{WebbyError, WebbyResult};
use webby_layout::{
    FontFamily as LayoutFontFamily, FontWeight as LayoutFontWeight, GraphicCommand, InlineFragment,
    LayoutBox, LayoutItem, LayoutKind, LayoutTree, Rect, ScrollOffsets,
    TextDecoration as LayoutTextDecoration, VisualColor,
};
use webby_text::{FontFamily as TextFontFamily, FontWeight as TextFontWeight};

/// RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

impl Color {
    /// Transparent black.
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Opaque white.
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Opaque black.
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    fn is_visible(self) -> bool {
        self.a > 0
    }
}

impl From<VisualColor> for Color {
    fn from(value: VisualColor) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
            a: value.a,
        }
    }
}

/// Font weight snapshot for text display commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// Normal weight.
    Normal,
    /// Bold weight.
    Bold,
}

/// Font family category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    /// Default sans/proportional family.
    Sans,
    /// Deterministic monospace family.
    Monospace,
}

impl From<LayoutFontFamily> for FontFamily {
    fn from(value: LayoutFontFamily) -> Self {
        match value {
            LayoutFontFamily::Sans => Self::Sans,
            LayoutFontFamily::Monospace => Self::Monospace,
        }
    }
}

impl From<FontFamily> for TextFontFamily {
    fn from(value: FontFamily) -> Self {
        match value {
            FontFamily::Sans => Self::Sans,
            FontFamily::Monospace => Self::Monospace,
        }
    }
}

impl From<LayoutFontWeight> for FontWeight {
    fn from(value: LayoutFontWeight) -> Self {
        match value {
            LayoutFontWeight::Normal => Self::Normal,
            LayoutFontWeight::Bold => Self::Bold,
        }
    }
}

impl From<FontWeight> for TextFontWeight {
    fn from(value: FontWeight) -> Self {
        match value {
            FontWeight::Normal => Self::Normal,
            FontWeight::Bold => Self::Bold,
        }
    }
}

/// Text decoration snapshot for text display commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecoration {
    /// No decoration.
    None,
    /// Underline decoration.
    Underline,
}

impl From<LayoutTextDecoration> for TextDecoration {
    fn from(value: LayoutTextDecoration) -> Self {
        match value {
            LayoutTextDecoration::None => Self::None,
            LayoutTextDecoration::Underline => Self::Underline,
        }
    }
}

/// Visual state for drawing a text-like form control overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormControlVisualState {
    /// Normal, unfocused control.
    Normal,
    /// Focused control.
    Focused,
}

/// A point in render coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

/// Renderer command produced from layout.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayCommand {
    /// Begin clipping subsequent commands to a layout viewport.
    PushClip { rect: Rect },
    /// End the most recently pushed clip scope.
    PopClip,
    /// Fill a rectangle.
    FillRect { rect: Rect, color: Color },
    /// Draw a text run.
    ///
    /// `text_decoration` is semantic metadata. Primitive underline pixels are
    /// represented by explicit [`DisplayCommand::Line`] commands.
    DrawText {
        text: String,
        rect: Rect,
        font_size: f32,
        font_weight: FontWeight,
        font_family: FontFamily,
        text_decoration: TextDecoration,
        color: Color,
        href: Option<String>,
    },
    /// Stroke a rectangle outline.
    StrokeRect {
        rect: Rect,
        color: Color,
        width: f32,
    },
    /// Draw a line, including authoritative primitive underline strokes.
    Line {
        from: Point,
        to: Point,
        color: Color,
        width: f32,
    },
    /// Draw a filled/stroked circle.
    Circle {
        center: Point,
        radius: f32,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
    },
    /// Draw an image placeholder.
    ImagePlaceholder { rect: Rect, border_color: Color },
    /// Draw decoded image pixels.
    DrawImage {
        rect: Rect,
        image_width: u32,
        image_height: u32,
        pixels: Vec<u8>,
    },
}

/// Ordered render commands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayList {
    /// Commands in paint order.
    pub commands: Vec<DisplayCommand>,
}

impl DisplayList {
    /// Creates an empty display list.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Builds an ordered display list from layout output.
pub fn build_display_list(layout: &LayoutTree) -> DisplayList {
    build_display_list_with_scroll_offsets(layout, &ScrollOffsets::new())
}

/// Builds an ordered display list with app-owned per-container scroll offsets.
pub fn build_display_list_with_scroll_offsets(
    layout: &LayoutTree,
    offsets: &ScrollOffsets,
) -> DisplayList {
    let mut list = DisplayList::empty();
    append_box_commands(&layout.root, offsets, 0.0, 0.0, &mut list);
    list
}

/// Formats a display list for deterministic CLI/debug output.
pub fn dump_display_list(list: &DisplayList) -> String {
    let mut output = String::new();
    output.push_str("display-list commands=");
    output.push_str(&list.commands.len().to_string());
    output.push('\n');

    for command in &list.commands {
        match command {
            DisplayCommand::PushClip { rect } => {
                output.push_str("  push-clip rect=");
                output.push_str(&format_rect(*rect));
            }
            DisplayCommand::PopClip => output.push_str("  pop-clip"),
            DisplayCommand::FillRect { rect, color } => {
                output.push_str("  fill rect=");
                output.push_str(&format_rect(*rect));
                output.push_str(" color=");
                output.push_str(&format_color(*color));
            }
            DisplayCommand::DrawText {
                text,
                rect,
                font_size,
                font_weight,
                font_family,
                text_decoration,
                color,
                href,
            } => {
                output.push_str("  text \"");
                output.push_str(&escape_dump_string(text));
                output.push_str("\" rect=");
                output.push_str(&format_rect(*rect));
                output.push_str(" font-size=");
                output.push_str(&format_px(*font_size));
                output.push_str(" font-weight=");
                output.push_str(match font_weight {
                    FontWeight::Normal => "normal",
                    FontWeight::Bold => "bold",
                });
                output.push_str(" font-family=");
                output.push_str(match font_family {
                    FontFamily::Sans => "sans",
                    FontFamily::Monospace => "monospace",
                });
                output.push_str(" text-decoration=");
                output.push_str(match text_decoration {
                    TextDecoration::None => "none",
                    TextDecoration::Underline => "underline",
                });
                output.push_str(" color=");
                output.push_str(&format_color(*color));
                if let Some(href) = href {
                    output.push_str(" href=\"");
                    output.push_str(&escape_dump_string(href));
                    output.push('"');
                }
            }
            DisplayCommand::StrokeRect { rect, color, width } => {
                output.push_str("  stroke rect=");
                output.push_str(&format_rect(*rect));
                output.push_str(" width=");
                output.push_str(&format_px(*width));
                output.push_str(" color=");
                output.push_str(&format_color(*color));
            }
            DisplayCommand::Line {
                from,
                to,
                color,
                width,
            } => {
                output.push_str("  line from=");
                output.push_str(&format_point(*from));
                output.push_str(" to=");
                output.push_str(&format_point(*to));
                output.push_str(" width=");
                output.push_str(&format_px(*width));
                output.push_str(" color=");
                output.push_str(&format_color(*color));
            }
            DisplayCommand::Circle {
                center,
                radius,
                fill,
                stroke,
                stroke_width,
            } => {
                output.push_str("  circle center=");
                output.push_str(&format_point(*center));
                output.push_str(" radius=");
                output.push_str(&format_px(*radius));
                if let Some(fill) = fill {
                    output.push_str(" fill=");
                    output.push_str(&format_color(*fill));
                }
                if let Some(stroke) = stroke {
                    output.push_str(" stroke=");
                    output.push_str(&format_color(*stroke));
                    output.push_str(" stroke-width=");
                    output.push_str(&format_px(*stroke_width));
                }
            }
            DisplayCommand::ImagePlaceholder { rect, border_color } => {
                output.push_str("  image-placeholder rect=");
                output.push_str(&format_rect(*rect));
                output.push_str(" border-color=");
                output.push_str(&format_color(*border_color));
            }
            DisplayCommand::DrawImage {
                rect,
                image_width,
                image_height,
                ..
            } => {
                output.push_str("  image rect=");
                output.push_str(&format_rect(*rect));
                output.push_str(" intrinsic=");
                output.push_str(&image_width.to_string());
                output.push('x');
                output.push_str(&image_height.to_string());
            }
        }
        output.push('\n');
    }

    output
}

/// In-memory RGBA rendering surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// RGBA pixels in row-major order.
    pub pixels: Vec<u8>,
    clip_stack: Vec<Rect>,
}

impl Surface {
    /// Creates a surface filled with the given color.
    pub fn new(width: usize, height: usize, color: Color) -> WebbyResult<Self> {
        if width == 0 || height == 0 {
            return Err(WebbyError::invalid_input(
                "surface width and height must be positive",
            ));
        }

        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| WebbyError::Render {
                message: "surface dimensions are too large".to_string(),
            })?;
        let byte_len = pixel_count
            .checked_mul(4)
            .ok_or_else(|| WebbyError::Render {
                message: "surface buffer is too large".to_string(),
            })?;
        let mut pixels = vec![0; byte_len];

        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = color.r;
            chunk[1] = color.g;
            chunk[2] = color.b;
            chunk[3] = color.a;
        }

        Ok(Self {
            width,
            height,
            pixels,
            clip_stack: Vec::new(),
        })
    }

    /// Renders display commands onto this surface with safe clipping.
    pub fn render_display_list(&mut self, list: &DisplayList) {
        for command in &list.commands {
            self.render_command(command);
        }
        self.clip_stack.clear();
    }

    /// Draws text directly onto the surface using Webby's shared text engine.
    ///
    /// This is used for browser chrome/status text. Page content should still
    /// go through display-list [`DisplayCommand::DrawText`] commands.
    pub fn draw_text(
        &mut self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        font_weight: FontWeight,
        color: Color,
    ) {
        let metrics = webby_text::measure_text_with_family(
            text,
            font_size,
            TextFontWeight::from(font_weight),
            TextFontFamily::Sans,
        );
        self.render_text_run(
            text,
            Rect {
                x,
                y,
                width: metrics.width,
                height: metrics.height,
            },
            font_size,
            font_weight,
            FontFamily::Sans,
            color,
        );
    }

    /// Encodes the surface as binary PPM (P6), dropping alpha.
    pub fn to_ppm(&self) -> Vec<u8> {
        let mut output = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        for pixel in self.pixels.chunks_exact(4) {
            output.push(pixel[0]);
            output.push(pixel[1]);
            output.push(pixel[2]);
        }
        output
    }

    fn render_command(&mut self, command: &DisplayCommand) {
        match command {
            DisplayCommand::PushClip { rect } => self.push_clip(*rect),
            DisplayCommand::PopClip => {
                self.clip_stack.pop();
            }
            DisplayCommand::FillRect { rect, color } => self.fill_rect(*rect, *color),
            DisplayCommand::DrawText {
                text,
                rect,
                font_size,
                font_weight,
                font_family,
                color,
                ..
            } => self.render_text_run(text, *rect, *font_size, *font_weight, *font_family, *color),
            DisplayCommand::StrokeRect { rect, color, width } => {
                self.stroke_rect(*rect, *color, *width);
            }
            DisplayCommand::Line {
                from,
                to,
                color,
                width,
            } => self.stroke_line(*from, *to, *color, *width),
            DisplayCommand::Circle {
                center,
                radius,
                fill,
                stroke,
                stroke_width,
            } => self.circle(*center, *radius, *fill, *stroke, *stroke_width),
            DisplayCommand::ImagePlaceholder { rect, border_color } => {
                self.fill_rect(
                    *rect,
                    Color {
                        r: 235,
                        g: 235,
                        b: 235,
                        a: 255,
                    },
                );
                self.stroke_rect(*rect, *border_color, 1.0);
                self.stroke_line(
                    Point {
                        x: rect.x,
                        y: rect.y,
                    },
                    Point {
                        x: rect.x + rect.width,
                        y: rect.y + rect.height,
                    },
                    *border_color,
                    1.0,
                );
                self.stroke_line(
                    Point {
                        x: rect.x + rect.width,
                        y: rect.y,
                    },
                    Point {
                        x: rect.x,
                        y: rect.y + rect.height,
                    },
                    *border_color,
                    1.0,
                );
            }
            DisplayCommand::DrawImage {
                rect,
                image_width,
                image_height,
                pixels,
            } => self.draw_image(*rect, *image_width, *image_height, pixels),
        }
    }

    fn push_clip(&mut self, rect: Rect) {
        let clipped = match self.clip_stack.last().copied() {
            Some(current) => intersect_rect(current, rect).unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            }),
            None => rect,
        };
        self.clip_stack.push(clipped);
    }

    fn circle(
        &mut self,
        center: Point,
        radius: f32,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
    ) {
        if !center.x.is_finite() || !center.y.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return;
        }
        let rect = Rect {
            x: center.x - radius,
            y: center.y - radius,
            width: radius * 2.0,
            height: radius * 2.0,
        };
        let Some(bounds) = self.clipped_rect(rect) else {
            return;
        };
        let radius_sq = radius * radius;
        let inner = (radius - stroke_width.max(0.0)).max(0.0);
        let inner_sq = inner * inner;
        for y in bounds.y_start..bounds.y_end {
            for x in bounds.x_start..bounds.x_end {
                let dx = x as f32 + 0.5 - center.x;
                let dy = y as f32 + 0.5 - center.y;
                let distance_sq = dx * dx + dy * dy;
                if distance_sq <= radius_sq {
                    if let Some(color) = fill
                        && distance_sq <= inner_sq
                    {
                        self.blend_pixel(x, y, color);
                    }
                    if let Some(color) = stroke
                        && distance_sq >= inner_sq
                    {
                        self.blend_pixel(x, y, color);
                    }
                }
            }
        }
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) {
        if !color.is_visible() {
            return;
        }

        let Some(bounds) = self.clipped_rect(rect) else {
            return;
        };

        for y in bounds.y_start..bounds.y_end {
            for x in bounds.x_start..bounds.x_end {
                self.blend_pixel(x, y, color);
            }
        }
    }

    fn stroke_rect(&mut self, rect: Rect, color: Color, width: f32) {
        if width <= 0.0 || !width.is_finite() {
            return;
        }

        let stroke = width.ceil();
        self.fill_rect(
            Rect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: stroke,
            },
            color,
        );
        self.fill_rect(
            Rect {
                x: rect.x,
                y: rect.y + rect.height - stroke,
                width: rect.width,
                height: stroke,
            },
            color,
        );
        self.fill_rect(
            Rect {
                x: rect.x,
                y: rect.y,
                width: stroke,
                height: rect.height,
            },
            color,
        );
        self.fill_rect(
            Rect {
                x: rect.x + rect.width - stroke,
                y: rect.y,
                width: stroke,
                height: rect.height,
            },
            color,
        );
    }

    fn stroke_line(&mut self, from: Point, to: Point, color: Color, width: f32) {
        if !color.is_visible() || width <= 0.0 || !width.is_finite() {
            return;
        }

        let min_x = from.x.min(to.x);
        let min_y = from.y.min(to.y);
        let max_x = from.x.max(to.x);
        let max_y = from.y.max(to.y);
        let stroke = width.ceil().max(1.0);

        if (from.y - to.y).abs() <= (from.x - to.x).abs() {
            self.fill_rect(
                Rect {
                    x: min_x,
                    y: min_y,
                    width: (max_x - min_x).max(stroke),
                    height: stroke,
                },
                color,
            );
        } else {
            self.fill_rect(
                Rect {
                    x: min_x,
                    y: min_y,
                    width: stroke,
                    height: (max_y - min_y).max(stroke),
                },
                color,
            );
        }
    }

    fn render_text_run(
        &mut self,
        text: &str,
        rect: Rect,
        font_size: f32,
        font_weight: FontWeight,
        font_family: FontFamily,
        color: Color,
    ) {
        if !color.is_visible() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        let rasterized = webby_text::rasterize_text_with_family(
            text,
            font_size,
            TextFontWeight::from(font_weight),
            TextFontFamily::from(font_family),
        );
        for glyph in rasterized.glyphs {
            self.blend_glyph(rect, &glyph, color);
        }
    }

    fn blend_glyph(&mut self, rect: Rect, glyph: &webby_text::GlyphBitmap, color: Color) {
        for glyph_y in 0..glyph.height {
            for glyph_x in 0..glyph.width {
                let Some(coverage_index) = glyph_y
                    .checked_mul(glyph.width)
                    .and_then(|row| row.checked_add(glyph_x))
                else {
                    continue;
                };
                let Some(coverage) = glyph.coverage.get(coverage_index).copied() else {
                    continue;
                };
                if coverage == 0 {
                    continue;
                }

                let destination_x = rect.x.floor() as i32 + glyph.x + glyph_x as i32;
                let destination_y = rect.y.floor() as i32 + glyph.y + glyph_y as i32;
                if destination_x < 0 || destination_y < 0 {
                    continue;
                }

                let x = destination_x as usize;
                let y = destination_y as usize;
                if x >= self.width || y >= self.height {
                    continue;
                }

                let alpha = scale_alpha(color.a, coverage);
                self.blend_pixel(x, y, Color { a: alpha, ..color });
            }
        }
    }

    fn draw_image(&mut self, rect: Rect, image_width: u32, image_height: u32, pixels: &[u8]) {
        if rect.width <= 0.0 || rect.height <= 0.0 || image_width == 0 || image_height == 0 {
            return;
        }
        let Some(expected_len) = usize::try_from(image_width)
            .ok()
            .and_then(|width| {
                usize::try_from(image_height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixel_count| pixel_count.checked_mul(4))
        else {
            return;
        };
        if expected_len > pixels.len() {
            return;
        }
        let Some(bounds) = self.clipped_rect(rect) else {
            return;
        };
        let source_width = image_width as f32;
        let source_height = image_height as f32;
        let source_width_usize = image_width as usize;

        for y in bounds.y_start..bounds.y_end {
            let relative_y = ((y as f32 - rect.y) / rect.height).clamp(0.0, 1.0);
            let source_y = (relative_y * source_height)
                .floor()
                .min(source_height - 1.0) as usize;
            for x in bounds.x_start..bounds.x_end {
                let relative_x = ((x as f32 - rect.x) / rect.width).clamp(0.0, 1.0);
                let source_x = (relative_x * source_width).floor().min(source_width - 1.0) as usize;
                let Some(index) = source_y
                    .checked_mul(source_width_usize)
                    .and_then(|row| row.checked_add(source_x))
                    .and_then(|pixel| pixel.checked_mul(4))
                else {
                    continue;
                };
                if index + 3 >= pixels.len() {
                    continue;
                }
                self.blend_pixel(
                    x,
                    y,
                    Color {
                        r: pixels[index],
                        g: pixels[index + 1],
                        b: pixels[index + 2],
                        a: pixels[index + 3],
                    },
                );
            }
        }
    }

    fn blend_pixel(&mut self, x: usize, y: usize, color: Color) {
        if let Some(clip) = self.clip_stack.last()
            && !point_in_rect(x as f32, y as f32, *clip)
        {
            return;
        }
        let Some(index) = y
            .checked_mul(self.width)
            .and_then(|row| row.checked_add(x))
            .and_then(|pixel| pixel.checked_mul(4))
        else {
            return;
        };

        if index + 3 >= self.pixels.len() {
            return;
        }

        if color.a == 255 {
            self.pixels[index] = color.r;
            self.pixels[index + 1] = color.g;
            self.pixels[index + 2] = color.b;
            self.pixels[index + 3] = color.a;
            return;
        }

        let alpha = u16::from(color.a);
        let inverse = 255_u16.saturating_sub(alpha);
        self.pixels[index] = blend_channel(color.r, self.pixels[index], alpha, inverse);
        self.pixels[index + 1] = blend_channel(color.g, self.pixels[index + 1], alpha, inverse);
        self.pixels[index + 2] = blend_channel(color.b, self.pixels[index + 2], alpha, inverse);
        self.pixels[index + 3] = 255;
    }

    fn clipped_rect(&self, rect: Rect) -> Option<ClippedRect> {
        let rect = match self.clip_stack.last().copied() {
            Some(clip) => intersect_rect(rect, clip)?,
            None => rect,
        };
        clip_rect(rect, self.width, self.height)
    }
}

/// Backend abstraction for turning backend-neutral display lists into pixels.
pub trait RenderBackend {
    /// Stable backend identifier for diagnostics and configuration.
    fn name(&self) -> &'static str;

    /// Renders a display list into a surface of the requested size.
    fn render_display_list(
        &self,
        list: &DisplayList,
        width: usize,
        height: usize,
    ) -> WebbyResult<Surface>;
}

/// Deterministic software renderer used by tests, CLI, and the native app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SoftwareRenderBackend;

impl RenderBackend for SoftwareRenderBackend {
    fn name(&self) -> &'static str {
        "software"
    }

    fn render_display_list(
        &self,
        list: &DisplayList,
        width: usize,
        height: usize,
    ) -> WebbyResult<Surface> {
        let mut surface = Surface::new(width, height, Color::WHITE)?;
        surface.render_display_list(list);
        Ok(surface)
    }
}

/// Renders a display list through an explicit backend.
pub fn render_with_backend<B: RenderBackend>(
    backend: &B,
    list: &DisplayList,
    width: usize,
    height: usize,
) -> WebbyResult<Surface> {
    backend.render_display_list(list, width, height)
}

/// Renders a display list to a white RGBA surface.
pub fn render_to_surface(list: &DisplayList, width: usize, height: usize) -> WebbyResult<Surface> {
    render_with_backend(&SoftwareRenderBackend, list, width, height)
}

/// Draws the current value overlay for a text/search form control.
///
/// The app owns focus and editing state; this helper centralizes form-control
/// drawing constants so chrome/app composition does not duplicate renderer
/// behavior.
pub fn draw_text_control_overlay(
    surface: &mut Surface,
    rect: Rect,
    value: &str,
    visual_state: FormControlVisualState,
) {
    let border = match visual_state {
        FormControlVisualState::Normal => Color {
            r: 120,
            g: 126,
            b: 136,
            a: 255,
        },
        FormControlVisualState::Focused => Color {
            r: 20,
            g: 105,
            b: 210,
            a: 255,
        },
    };
    let list = DisplayList {
        commands: vec![
            DisplayCommand::FillRect {
                rect,
                color: Color::WHITE,
            },
            DisplayCommand::StrokeRect {
                rect,
                color: border,
                width: 1.0,
            },
            DisplayCommand::DrawText {
                text: value.to_string(),
                rect: Rect {
                    x: rect.x + 5.0,
                    y: rect.y + 4.0,
                    width: (rect.width - 10.0).max(0.0),
                    height: (rect.height - 8.0).max(0.0),
                },
                font_size: 14.0,
                font_weight: FontWeight::Normal,
                font_family: FontFamily::Sans,
                text_decoration: TextDecoration::None,
                color: Color::BLACK,
                href: None,
            },
        ],
    };
    surface.render_display_list(&list);
}

fn append_box_commands(
    layout_box: &LayoutBox,
    offsets: &ScrollOffsets,
    dx: f32,
    dy: f32,
    list: &mut DisplayList,
) {
    let border_box = translate(layout_box.dimensions.border_box(), dx, dy);
    let background = Color::from(layout_box.visuals.background_color);
    if background.is_visible() && has_area(border_box) {
        list.commands.push(DisplayCommand::FillRect {
            rect: border_box,
            color: background,
        });
    }

    let border_color = Color::from(layout_box.visuals.border.color);
    let border_width = max_edge(layout_box.dimensions.border);
    if border_width > 0.0 && border_color.is_visible() && has_area(border_box) {
        list.commands.push(DisplayCommand::StrokeRect {
            rect: border_box,
            color: border_color,
            width: border_width,
        });
    }

    if let LayoutKind::Image {
        image,
        graphics,
        visible,
        ..
    } = &layout_box.kind
        && has_area(layout_box.dimensions.content)
        && *visible
    {
        if graphics.is_empty() {
            match image {
                Some(resource) => list.commands.push(DisplayCommand::DrawImage {
                    rect: translate(layout_box.dimensions.content, dx, dy),
                    image_width: resource.image.width,
                    image_height: resource.image.height,
                    pixels: resource.image.pixels.clone(),
                }),
                None => list.commands.push(DisplayCommand::ImagePlaceholder {
                    rect: translate(layout_box.dimensions.content, dx, dy),
                    border_color,
                }),
            }
        } else {
            append_graphic_commands(
                translate(layout_box.dimensions.content, dx, dy),
                graphics,
                list,
            );
        }
    }

    let (child_dx, child_dy, clipped) = match &layout_box.scroll_container {
        Some(container) => {
            let offset =
                container.clamp_offset(offsets.get(&container.id).copied().unwrap_or_default());
            list.commands.push(DisplayCommand::PushClip {
                rect: translate(container.viewport, dx, dy),
            });
            (dx - offset.x, dy - offset.y, true)
        }
        None => (dx, dy, false),
    };
    for item in &layout_box.contents {
        match item {
            LayoutItem::LineBox(line) => {
                for fragment in &line.fragments {
                    match fragment {
                        InlineFragment::Text(run) => {
                            append_text_commands(run, child_dx, child_dy, list)
                        }
                        InlineFragment::Box(child) => {
                            append_box_commands(child, offsets, child_dx, child_dy, list)
                        }
                    }
                }
            }
            LayoutItem::Text(run) => append_text_commands(run, child_dx, child_dy, list),
            LayoutItem::Box(child) => append_box_commands(child, offsets, child_dx, child_dy, list),
        }
    }
    if clipped {
        list.commands.push(DisplayCommand::PopClip);
    }
}

fn append_graphic_commands(origin: Rect, graphics: &[GraphicCommand], list: &mut DisplayList) {
    for command in graphics {
        match command {
            GraphicCommand::FillRect { rect, color } => {
                list.commands.push(DisplayCommand::FillRect {
                    rect: translate_rect(origin, *rect),
                    color: Color::from(*color),
                })
            }
            GraphicCommand::StrokeRect { rect, color, width } => {
                list.commands.push(DisplayCommand::StrokeRect {
                    rect: translate_rect(origin, *rect),
                    color: Color::from(*color),
                    width: *width,
                });
            }
            GraphicCommand::Line {
                from_x,
                from_y,
                to_x,
                to_y,
                color,
                width,
            } => list.commands.push(DisplayCommand::Line {
                from: Point {
                    x: origin.x + *from_x,
                    y: origin.y + *from_y,
                },
                to: Point {
                    x: origin.x + *to_x,
                    y: origin.y + *to_y,
                },
                color: Color::from(*color),
                width: *width,
            }),
            GraphicCommand::Circle {
                cx,
                cy,
                radius,
                fill,
                stroke,
                stroke_width,
            } => list.commands.push(DisplayCommand::Circle {
                center: Point {
                    x: origin.x + *cx,
                    y: origin.y + *cy,
                },
                radius: *radius,
                fill: fill.map(Color::from),
                stroke: stroke.map(Color::from),
                stroke_width: *stroke_width,
            }),
        }
    }
}

fn translate_rect(origin: Rect, rect: Rect) -> Rect {
    Rect {
        x: origin.x + rect.x,
        y: origin.y + rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn append_text_commands(run: &webby_layout::TextRun, dx: f32, dy: f32, list: &mut DisplayList) {
    if run.text.is_empty() || !has_area(run.rect) {
        return;
    }

    let color = Color::from(run.visuals.color);
    list.commands.push(DisplayCommand::DrawText {
        text: run.text.clone(),
        rect: translate(run.rect, dx, dy),
        font_size: run.font_size,
        font_weight: FontWeight::from(run.visuals.font_weight),
        font_family: FontFamily::from(run.visuals.font_family),
        text_decoration: TextDecoration::from(run.visuals.text_decoration),
        color,
        href: run.visuals.href.clone(),
    });

    if run.visuals.text_decoration == LayoutTextDecoration::Underline {
        let rect = translate(run.rect, dx, dy);
        let y = rect.y + rect.height - 2.0;
        list.commands.push(DisplayCommand::Line {
            from: Point { x: rect.x, y },
            to: Point {
                x: rect.x + rect.width,
                y,
            },
            color,
            width: 1.0,
        });
    }
}

fn translate(rect: Rect, dx: f32, dy: f32) -> Rect {
    Rect {
        x: rect.x + dx,
        y: rect.y + dy,
        ..rect
    }
}

fn intersect_rect(left: Rect, right: Rect) -> Option<Rect> {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = (left.x + left.width).min(right.x + right.width);
    let bottom = (left.y + left.height).min(right.y + right.height);
    (right_edge > x && bottom > y).then_some(Rect {
        x,
        y,
        width: right_edge - x,
        height: bottom - y,
    })
}

fn point_in_rect(x: f32, y: f32, rect: Rect) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

fn max_edge(edges: webby_layout::EdgeSizes) -> f32 {
    edges.top.max(edges.right).max(edges.bottom).max(edges.left)
}

fn has_area(rect: Rect) -> bool {
    rect.width > 0.0 && rect.height > 0.0 && rect.width.is_finite() && rect.height.is_finite()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClippedRect {
    x_start: usize,
    x_end: usize,
    y_start: usize,
    y_end: usize,
}

fn clip_rect(rect: Rect, width: usize, height: usize) -> Option<ClippedRect> {
    if !rect.x.is_finite()
        || !rect.y.is_finite()
        || !rect.width.is_finite()
        || !rect.height.is_finite()
        || rect.width <= 0.0
        || rect.height <= 0.0
    {
        return None;
    }

    let x_start = rect.x.floor().max(0.0).min(width as f32) as usize;
    let y_start = rect.y.floor().max(0.0).min(height as f32) as usize;
    let x_end = (rect.x + rect.width).ceil().max(0.0).min(width as f32) as usize;
    let y_end = (rect.y + rect.height).ceil().max(0.0).min(height as f32) as usize;

    if x_start >= x_end || y_start >= y_end {
        return None;
    }

    Some(ClippedRect {
        x_start,
        x_end,
        y_start,
        y_end,
    })
}

fn blend_channel(source: u8, destination: u8, alpha: u16, inverse: u16) -> u8 {
    let blended = u16::from(source) * alpha + u16::from(destination) * inverse;
    (blended / 255) as u8
}

fn scale_alpha(alpha: u8, coverage: u8) -> u8 {
    ((u16::from(alpha) * u16::from(coverage)) / 255) as u8
}

fn format_px(value: f32) -> String {
    format!("{value:.1}")
}

fn format_rect(rect: Rect) -> String {
    format!(
        "{:.1},{:.1},{:.1},{:.1}",
        rect.x, rect.y, rect.width, rect.height
    )
}

fn format_point(point: Point) -> String {
    format!("{:.1},{:.1}", point.x, point.y)
}

fn format_color(color: Color) -> String {
    format!("rgba({},{},{},{})", color.r, color.g, color.b, color.a)
}

fn escape_dump_string(input: &str) -> String {
    let mut escaped = String::new();

    for character in input.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str("\\u{");
                escaped.push_str(&format!("{:x}", character as u32));
                escaped.push('}');
            }
            character => escaped.push(character),
        }
    }

    escaped
}

#[cfg(test)]
mod tests {
    use super::{
        Color, DisplayCommand, FontFamily, FontWeight, FormControlVisualState, RenderBackend,
        SoftwareRenderBackend, Surface, TextDecoration, build_display_list,
        draw_text_control_overlay, dump_display_list, render_to_surface, render_with_backend,
    };
    use webby_core::WebbyResult;
    use webby_layout::{
        BorderVisuals, BoxVisuals, DecodedImage, Dimensions, EdgeSizes, ElementMetadata,
        ImageResource, InlineFragment, LayoutBox, LayoutFlow, LayoutItem, LayoutKind, LayoutTree,
        LineBox, Positioning, Rect, TextRun, TextVisuals, Viewport, VisualColor,
    };

    #[test]
    fn empty_display_list_has_no_commands() {
        assert!(super::DisplayList::empty().commands.is_empty());
    }

    #[test]
    fn software_backend_has_stable_identity_and_renders_display_list() -> WebbyResult<()> {
        let backend = SoftwareRenderBackend;
        let list = super::DisplayList {
            commands: vec![DisplayCommand::FillRect {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 2.0,
                    height: 2.0,
                },
                color: Color::BLACK,
            }],
        };

        let surface = render_with_backend(&backend, &list, 2, 2)?;

        assert_eq!(backend.name(), "software");
        assert_eq!(surface.pixels[0..4], [0, 0, 0, 255]);
        Ok(())
    }

    #[test]
    fn compatibility_render_helper_uses_deterministic_software_backend() -> WebbyResult<()> {
        let list = super::DisplayList {
            commands: vec![DisplayCommand::FillRect {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: Color {
                    r: 8,
                    g: 16,
                    b: 32,
                    a: 255,
                },
            }],
        };

        let backend_surface = render_with_backend(&SoftwareRenderBackend, &list, 2, 2)?;
        let helper_surface = render_to_surface(&list, 2, 2)?;

        assert_eq!(backend_surface, helper_surface);
        Ok(())
    }

    #[test]
    fn display_list_contains_background_rectangles_for_styled_boxes() {
        let list = build_display_list(&sample_layout_tree());

        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                DisplayCommand::FillRect {
                    color: Color {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255
                    },
                    ..
                }
            )
        }));
    }

    #[test]
    fn display_list_contains_border_commands_for_bordered_boxes() {
        let list = build_display_list(&sample_layout_tree());

        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                DisplayCommand::StrokeRect {
                    width,
                    color: Color {
                        r: 180,
                        g: 180,
                        b: 180,
                        a: 255
                    },
                    ..
                } if *width == 1.0
            )
        }));
    }

    #[test]
    fn display_list_contains_text_commands_for_visible_text() {
        let list = build_display_list(&sample_layout_tree());

        assert!(list.commands.iter().any(|command| {
            matches!(command, DisplayCommand::DrawText { text, .. } if text == "Hello")
        }));
    }

    #[test]
    fn text_control_overlay_helper_draws_focused_control() -> WebbyResult<()> {
        let mut surface = Surface::new(32, 20, Color::WHITE)?;

        draw_text_control_overlay(
            &mut surface,
            Rect {
                x: 1.0,
                y: 1.0,
                width: 26.0,
                height: 16.0,
            },
            "x",
            FormControlVisualState::Focused,
        );

        assert_eq!(pixel(&surface, 1, 1), [20, 105, 210, 255]);
        assert!(
            surface
                .pixels
                .chunks_exact(4)
                .any(|pixel| { pixel != [255, 255, 255, 255] && pixel != [20, 105, 210, 255] })
        );
        Ok(())
    }

    #[test]
    fn renderer_draws_input_and_button_controls_from_layout() -> WebbyResult<()> {
        let tree = layout_from_html(
            "<body><form><input type=\"text\" name=\"q\" value=\"webby\"><input type=\"checkbox\" checked><select><option>One</option><option selected>Two</option></select><textarea>Hello</textarea><button type=\"submit\">Go</button></form></body>",
        )?;
        let list = build_display_list(&tree);
        let surface = render_to_surface(&list, 420, 120)?;

        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                DisplayCommand::StrokeRect {
                    color: Color {
                        r: 120,
                        g: 126,
                        b: 136,
                        a: 255
                    },
                    ..
                }
            )
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(command, DisplayCommand::DrawText { text, .. } if text == "webby")
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(command, DisplayCommand::DrawText { text, .. } if text == "[x]")
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(command, DisplayCommand::DrawText { text, .. } if text == "Two")
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(command, DisplayCommand::DrawText { text, .. } if text == "Hello")
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(command, DisplayCommand::DrawText { text, .. } if text == "Go")
        }));
        assert!(!surface.pixels.iter().all(|value| *value == 255));
        Ok(())
    }

    #[test]
    fn hidden_elements_do_not_produce_display_commands() {
        let list = build_display_list(&sample_layout_tree());

        assert!(!dump_display_list(&list).contains("Hidden"));
    }

    #[test]
    fn full_pipeline_hidden_content_produces_no_display_commands() -> WebbyResult<()> {
        let tree = layout_from_html(
            "<!doctype html><html><head><title>Hidden title</title><style>.x{}</style></head>\
             <body><script>Hidden script</script><p>Visible text</p></body></html>",
        )?;
        let list = build_display_list(&tree);
        let dump = dump_display_list(&list);

        assert!(dump.contains("Visible"));
        assert!(!dump.contains("Hidden title"));
        assert!(!dump.contains("Hidden script"));
        assert!(!dump.contains(".x"));
        Ok(())
    }

    #[test]
    fn visibility_hidden_image_preserves_layout_without_painting_placeholder() -> WebbyResult<()> {
        let tree = layout_from_html(
            "<style>img { visibility: hidden; }</style><body><p>Before <img alt=\"Hidden image\"> After</p></body>",
        )?;
        let list = build_display_list(&tree);

        assert!(dump_layout_contains_image_box(&tree.root));
        assert!(!list.commands.iter().any(|command| {
            matches!(
                command,
                DisplayCommand::ImagePlaceholder { .. } | DisplayCommand::DrawImage { .. }
            )
        }));
        assert!(
            ordered_text(&list)
                .iter()
                .any(|text| text.contains("Before") || text.contains("After"))
        );
        Ok(())
    }

    #[test]
    fn paint_order_is_deterministic() {
        let list = build_display_list(&sample_layout_tree());
        let text_index = command_index(&list, |command| {
            matches!(command, DisplayCommand::DrawText { .. })
        });
        let underline_index = command_index(&list, |command| {
            matches!(command, DisplayCommand::Line { .. })
        });
        let image_index = command_index(&list, |command| {
            matches!(command, DisplayCommand::ImagePlaceholder { .. })
        });

        assert!(matches!(list.commands[0], DisplayCommand::FillRect { .. }));
        assert!(text_index < underline_index);
        assert!(underline_index < image_index);
        assert_eq!(list, build_display_list(&sample_layout_tree()));
    }

    #[test]
    fn mixed_text_block_text_order_follows_real_layout() -> WebbyResult<()> {
        let tree = layout_from_html("<body>before<div>block</div>after</body>")?;
        let list = build_display_list(&tree);

        assert_eq!(ordered_text(&list), vec!["before", "block", "after"]);
        Ok(())
    }

    #[test]
    fn positioned_boxes_preserve_document_paint_order() -> WebbyResult<()> {
        let tree = layout_from_html(
            "<style>.abs { position: absolute; left: 5px; top: 5px; }</style><body><div>before</div><div class=\"abs\">absolute</div><div>after</div></body>",
        )?;
        let list = build_display_list(&tree);

        assert_eq!(ordered_text(&list), vec!["before", "absolute", "after"]);
        Ok(())
    }

    #[test]
    fn flex_display_list_preserves_item_order() -> WebbyResult<()> {
        let tree = layout_from_html(
            "<style>nav { display: flex; gap: 4px; } a { width: 40px; }</style><body><nav><a href=\"/one\">One</a><a href=\"/two\">Two</a></nav></body>",
        )?;
        let list = build_display_list(&tree);

        assert_eq!(ordered_text(&list), vec!["One", "Two"]);
        Ok(())
    }

    #[test]
    fn table_display_list_contains_header_cell_paint_and_text() -> WebbyResult<()> {
        let tree =
            layout_from_html("<body><table><tr><th>Name</th><td>Webby</td></tr></table></body>")?;
        let list = build_display_list(&tree);

        assert!(
            list.commands
                .iter()
                .any(|command| matches!(command, DisplayCommand::StrokeRect { .. }))
        );
        assert!(list.commands.iter().any(|command| {
            matches!(command, DisplayCommand::FillRect { color, .. } if color.r == 238 && color.g == 240 && color.b == 244)
        }));
        assert!(list.commands.iter().any(
            |command| matches!(command, DisplayCommand::DrawText { text, .. } if text == "Name")
        ));
        assert!(list.commands.iter().any(
            |command| matches!(command, DisplayCommand::DrawText { text, .. } if text == "Webby")
        ));
        Ok(())
    }

    #[test]
    fn richer_element_display_list_contains_markers_pre_text_and_rule() -> WebbyResult<()> {
        let tree = layout_from_html(
            "<body><ul><li>Item</li></ul><pre>  code</pre><blockquote>Quote</blockquote><hr></body>",
        )?;
        let list = build_display_list(&tree);

        assert!(list.commands.iter().any(
            |command| matches!(command, DisplayCommand::DrawText { text, .. } if text == "• ")
        ));
        assert!(list.commands.iter().any(
            |command| matches!(command, DisplayCommand::DrawText { text, .. } if text == "  code")
        ));
        assert!(
            list.commands
                .iter()
                .filter(|command| matches!(command, DisplayCommand::StrokeRect { .. }))
                .count()
                >= 2
        );
        Ok(())
    }

    #[test]
    fn display_list_preserves_font_family_metadata() -> WebbyResult<()> {
        let tree = layout_from_html("<body><pre>  code</pre></body>")?;
        let list = build_display_list(&tree);

        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                DisplayCommand::DrawText {
                    text,
                    font_family: FontFamily::Monospace,
                    ..
                } if text == "  code"
            )
        }));
        Ok(())
    }

    #[test]
    fn inline_image_order_follows_real_layout() -> WebbyResult<()> {
        let tree = layout_from_html("<body>before <img width=\"10\" height=\"10\"> after</body>")?;
        let list = build_display_list(&tree);

        assert_eq!(
            ordered_text_and_images(&list),
            vec!["text:before", "image", "text:after"]
        );
        Ok(())
    }

    #[test]
    fn display_list_order_follows_ordered_layout_content() {
        let list = build_display_list(&ordered_mixed_layout_tree());

        assert_eq!(
            ordered_text_and_images(&list),
            vec!["text:first", "image", "text:last"]
        );
    }

    #[test]
    fn display_list_order_follows_inline_line_fragments() {
        let list = build_display_list(&line_fragment_layout_tree());

        assert_eq!(
            ordered_text_and_images(&list),
            vec!["text:first", "image", "text:last"]
        );
    }

    #[test]
    fn image_placeholder_produces_display_command() {
        let list = build_display_list(&sample_layout_tree());

        assert!(
            list.commands
                .iter()
                .any(|command| { matches!(command, DisplayCommand::ImagePlaceholder { .. }) })
        );
    }

    #[test]
    fn display_list_contains_image_command_with_decoded_metadata() {
        let list = build_display_list(&decoded_image_layout_tree());

        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                DisplayCommand::DrawImage {
                    image_width: 2,
                    image_height: 1,
                    pixels,
                    ..
                } if pixels == &[255, 0, 0, 255, 0, 255, 0, 255]
            )
        }));
    }

    #[test]
    fn link_text_preserves_href_metadata() {
        let list = build_display_list(&sample_layout_tree());

        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                DisplayCommand::DrawText {
                    text,
                    href: Some(href),
                    ..
                } if text == "Hello" && href == "/hello"
            )
        }));
    }

    #[test]
    fn text_visuals_are_carried_into_text_display_commands() {
        let list = build_display_list(&sample_layout_tree());

        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                DisplayCommand::DrawText {
                    text,
                    font_weight: FontWeight::Bold,
                    text_decoration: TextDecoration::Underline,
                    color: Color {
                        r: 0,
                        g: 0,
                        b: 238,
                        a: 255
                    },
                    ..
                } if text == "Hello"
            )
        }));
    }

    #[test]
    fn underline_line_command_is_authoritative_for_primitive_drawing() {
        let list = build_display_list(&sample_layout_tree());
        let semantic_underlines = list
            .commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    DisplayCommand::DrawText {
                        text_decoration: TextDecoration::Underline,
                        ..
                    }
                )
            })
            .count();
        let drawn_underlines = list
            .commands
            .iter()
            .filter(|command| matches!(command, DisplayCommand::Line { .. }))
            .count();

        assert_eq!(semantic_underlines, 1);
        assert_eq!(drawn_underlines, 1);
    }

    #[test]
    fn draw_text_underline_metadata_does_not_draw_underline_pixels() -> WebbyResult<()> {
        let text_only = super::DisplayList {
            commands: vec![DisplayCommand::DrawText {
                text: " ".to_string(),
                rect: Rect {
                    x: 1.0,
                    y: 1.0,
                    width: 8.0,
                    height: 8.0,
                },
                font_size: 8.0,
                font_weight: FontWeight::Normal,
                font_family: FontFamily::Sans,
                text_decoration: TextDecoration::Underline,
                color: Color::BLACK,
                href: None,
            }],
        };
        let mut text_surface = Surface::new(12, 12, Color::WHITE)?;
        text_surface.render_display_list(&text_only);

        let line_only = super::DisplayList {
            commands: vec![DisplayCommand::Line {
                from: super::Point { x: 1.0, y: 7.0 },
                to: super::Point { x: 9.0, y: 7.0 },
                color: Color::BLACK,
                width: 1.0,
            }],
        };
        let mut line_surface = Surface::new(12, 12, Color::WHITE)?;
        line_surface.render_display_list(&line_only);

        assert_eq!(pixel(&text_surface, 6, 7), [255, 255, 255, 255]);
        assert_eq!(pixel(&line_surface, 6, 7), [0, 0, 0, 255]);
        Ok(())
    }

    #[test]
    fn software_surface_handles_clipping_safely() -> WebbyResult<()> {
        let mut surface = Surface::new(4, 4, Color::WHITE)?;
        let list = super::DisplayList {
            commands: vec![DisplayCommand::FillRect {
                rect: Rect {
                    x: -2.0,
                    y: -2.0,
                    width: 4.0,
                    height: 4.0,
                },
                color: Color::BLACK,
            }],
        };

        surface.render_display_list(&list);

        assert_eq!(&surface.pixels[0..4], &[0, 0, 0, 255]);
        assert_eq!(&surface.pixels[60..64], &[255, 255, 255, 255]);
        Ok(())
    }

    #[test]
    fn scoped_clip_commands_limit_subtree_pixels() -> WebbyResult<()> {
        let list = super::DisplayList {
            commands: vec![
                DisplayCommand::PushClip {
                    rect: Rect {
                        x: 1.0,
                        y: 1.0,
                        width: 2.0,
                        height: 2.0,
                    },
                },
                DisplayCommand::FillRect {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 4.0,
                        height: 4.0,
                    },
                    color: Color::BLACK,
                },
                DisplayCommand::PopClip,
            ],
        };
        let surface = render_to_surface(&list, 4, 4)?;

        assert_eq!(pixel(&surface, 0, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&surface, 1, 1), [0, 0, 0, 255]);
        assert_eq!(pixel(&surface, 3, 3), [255, 255, 255, 255]);
        Ok(())
    }

    #[test]
    fn rendered_text_changes_pixels_in_expected_region() -> WebbyResult<()> {
        let list = super::DisplayList {
            commands: vec![DisplayCommand::DrawText {
                text: "A".to_string(),
                rect: Rect {
                    x: 4.0,
                    y: 4.0,
                    width: 40.0,
                    height: 24.0,
                },
                font_size: 24.0,
                font_weight: FontWeight::Normal,
                font_family: FontFamily::Sans,
                text_decoration: TextDecoration::None,
                color: Color::BLACK,
                href: None,
            }],
        };
        let surface = render_to_surface(&list, 48, 32)?;

        assert!(
            surface
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel != [255, 255, 255, 255])
        );
        Ok(())
    }

    #[test]
    fn text_color_affects_rendered_pixels() -> WebbyResult<()> {
        let list = super::DisplayList {
            commands: vec![DisplayCommand::DrawText {
                text: "A".to_string(),
                rect: Rect {
                    x: 4.0,
                    y: 4.0,
                    width: 40.0,
                    height: 24.0,
                },
                font_size: 24.0,
                font_weight: FontWeight::Normal,
                font_family: FontFamily::Sans,
                text_decoration: TextDecoration::None,
                color: Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                href: None,
            }],
        };
        let surface = render_to_surface(&list, 48, 32)?;

        assert!(
            surface
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel[0] > pixel[1] && pixel[0] > pixel[2])
        );
        Ok(())
    }

    #[test]
    fn software_renderer_draws_image_pixels() -> WebbyResult<()> {
        let list = super::DisplayList {
            commands: vec![DisplayCommand::DrawImage {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 2.0,
                    height: 1.0,
                },
                image_width: 2,
                image_height: 1,
                pixels: vec![255, 0, 0, 255, 0, 255, 0, 255],
            }],
        };
        let surface = render_to_surface(&list, 2, 1)?;

        assert_eq!(pixel(&surface, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&surface, 1, 0), [0, 255, 0, 255]);
        Ok(())
    }

    #[test]
    fn image_rendering_clips_safely_out_of_bounds() -> WebbyResult<()> {
        let list = super::DisplayList {
            commands: vec![DisplayCommand::DrawImage {
                rect: Rect {
                    x: -1.0,
                    y: 0.0,
                    width: 2.0,
                    height: 1.0,
                },
                image_width: 2,
                image_height: 1,
                pixels: vec![255, 0, 0, 255, 0, 255, 0, 255],
            }],
        };
        let surface = render_to_surface(&list, 1, 1)?;

        assert_ne!(pixel(&surface, 0, 0), [255, 255, 255, 255]);
        Ok(())
    }

    #[test]
    fn out_of_bounds_drawing_does_not_panic() -> WebbyResult<()> {
        let mut surface = Surface::new(3, 3, Color::WHITE)?;
        let list = super::DisplayList {
            commands: vec![
                DisplayCommand::FillRect {
                    rect: Rect {
                        x: 100.0,
                        y: 100.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    color: Color::BLACK,
                },
                DisplayCommand::Line {
                    from: super::Point { x: -20.0, y: 1.0 },
                    to: super::Point { x: 20.0, y: 1.0 },
                    color: Color::BLACK,
                    width: 1.0,
                },
                DisplayCommand::DrawText {
                    text: "Clip".to_string(),
                    rect: Rect {
                        x: -100.0,
                        y: -100.0,
                        width: 300.0,
                        height: 24.0,
                    },
                    font_size: 24.0,
                    font_weight: FontWeight::Normal,
                    font_family: FontFamily::Sans,
                    text_decoration: TextDecoration::None,
                    color: Color::BLACK,
                    href: None,
                },
            ],
        };

        surface.render_display_list(&list);

        assert_eq!(surface.pixels.len(), 36);
        Ok(())
    }

    #[test]
    fn tiny_viewport_rendering_does_not_panic() -> WebbyResult<()> {
        let list = build_display_list(&sample_layout_tree());
        let surface = render_to_surface(&list, 1, 1)?;

        assert_eq!(surface.pixels.len(), 4);
        Ok(())
    }

    #[test]
    fn deterministic_display_list_dump() {
        let list = build_display_list(&sample_layout_tree());
        let dump = dump_display_list(&list);

        assert_eq!(dump, dump_display_list(&list));
        assert!(dump.contains("display-list commands="));
        assert!(dump.contains("text \"Hello\""));
        assert!(dump.contains("href=\"/hello\""));
    }

    #[test]
    fn ppm_output_is_deterministic() -> WebbyResult<()> {
        let list = build_display_list(&sample_layout_tree());
        let surface = render_to_surface(&list, 8, 8)?;
        let ppm = surface.to_ppm();

        assert!(ppm.starts_with(b"P6\n8 8\n255\n"));
        assert_eq!(ppm.len(), b"P6\n8 8\n255\n".len() + 8 * 8 * 3);
        assert_eq!(ppm, surface.to_ppm());
        Ok(())
    }

    fn sample_layout_tree() -> LayoutTree {
        LayoutTree {
            root: LayoutBox {
                kind: LayoutKind::Document,
                dimensions: dimensions(0.0, 0.0, 100.0, 100.0),
                visuals: BoxVisuals {
                    background_color: VisualColor {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255,
                    },
                    border: BorderVisuals {
                        color: VisualColor {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 0,
                        },
                    },
                },
                positioning: Positioning::default(),
                flow: LayoutFlow::Block,
                scroll_container: None,
                contents: vec![
                    LayoutItem::Text(TextRun {
                        rect: Rect {
                            x: 2.0,
                            y: 2.0,
                            width: 40.0,
                            height: 16.0,
                        },
                        text: "Hello".to_string(),
                        font_size: 16.0,
                        visuals: TextVisuals {
                            color: VisualColor {
                                r: 0,
                                g: 0,
                                b: 238,
                                a: 255,
                            },
                            font_weight: webby_layout::FontWeight::Bold,
                            font_family: webby_layout::FontFamily::Sans,
                            text_decoration: webby_layout::TextDecoration::Underline,
                            is_link: true,
                            href: Some("/hello".to_string()),
                            link_node_id: Some(1),
                        },
                    }),
                    LayoutItem::Box(LayoutBox {
                        kind: LayoutKind::Image {
                            tag_name: "img".to_string(),
                            metadata: Box::new(ElementMetadata::default()),
                            src: None,
                            alt: None,
                            image: None,
                            graphics: Vec::new(),
                            visible: true,
                        },
                        dimensions: Dimensions {
                            content: Rect {
                                x: 10.0,
                                y: 30.0,
                                width: 20.0,
                                height: 10.0,
                            },
                            padding: EdgeSizes::ZERO,
                            border: EdgeSizes {
                                top: 1.0,
                                right: 1.0,
                                bottom: 1.0,
                                left: 1.0,
                            },
                            margin: EdgeSizes::ZERO,
                        },
                        visuals: BoxVisuals {
                            background_color: VisualColor {
                                r: 0,
                                g: 0,
                                b: 0,
                                a: 0,
                            },
                            border: BorderVisuals {
                                color: VisualColor {
                                    r: 180,
                                    g: 180,
                                    b: 180,
                                    a: 255,
                                },
                            },
                        },
                        positioning: Positioning::default(),
                        flow: LayoutFlow::Block,
                        scroll_container: None,
                        contents: Vec::new(),
                    }),
                ],
            },
            scroll_height: 100.0,
            links: Vec::new(),
            form_controls: Vec::new(),
        }
    }

    fn ordered_mixed_layout_tree() -> LayoutTree {
        LayoutTree {
            root: LayoutBox {
                kind: LayoutKind::Document,
                dimensions: dimensions(0.0, 0.0, 100.0, 60.0),
                visuals: transparent_box_visuals(),
                positioning: Positioning::default(),
                flow: LayoutFlow::Block,
                scroll_container: None,
                contents: vec![
                    LayoutItem::Text(TextRun {
                        rect: Rect {
                            x: 0.0,
                            y: 0.0,
                            width: 20.0,
                            height: 16.0,
                        },
                        text: "first".to_string(),
                        font_size: 16.0,
                        visuals: plain_text_visuals(),
                    }),
                    LayoutItem::Box(LayoutBox {
                        kind: LayoutKind::Image {
                            tag_name: "img".to_string(),
                            metadata: Box::new(ElementMetadata::default()),
                            src: None,
                            alt: None,
                            image: None,
                            graphics: Vec::new(),
                            visible: true,
                        },
                        dimensions: dimensions(22.0, 0.0, 10.0, 10.0),
                        visuals: transparent_box_visuals(),
                        positioning: Positioning::default(),
                        flow: LayoutFlow::Block,
                        scroll_container: None,
                        contents: Vec::new(),
                    }),
                    LayoutItem::Text(TextRun {
                        rect: Rect {
                            x: 34.0,
                            y: 0.0,
                            width: 20.0,
                            height: 16.0,
                        },
                        text: "last".to_string(),
                        font_size: 16.0,
                        visuals: plain_text_visuals(),
                    }),
                ],
            },
            scroll_height: 60.0,
            links: Vec::new(),
            form_controls: Vec::new(),
        }
    }

    fn decoded_image_layout_tree() -> LayoutTree {
        LayoutTree {
            root: LayoutBox {
                kind: LayoutKind::Document,
                dimensions: dimensions(0.0, 0.0, 20.0, 10.0),
                visuals: transparent_box_visuals(),
                positioning: Positioning::default(),
                flow: LayoutFlow::Block,
                scroll_container: None,
                contents: vec![LayoutItem::Box(LayoutBox {
                    kind: LayoutKind::Image {
                        tag_name: "img".to_string(),
                        metadata: Box::new(ElementMetadata::default()),
                        src: Some("photo.png".to_string()),
                        alt: None,
                        image: Some(ImageResource {
                            src: "photo.png".to_string(),
                            final_url: "https://example.test/photo.png".to_string(),
                            image: DecodedImage {
                                width: 2,
                                height: 1,
                                pixels: vec![255, 0, 0, 255, 0, 255, 0, 255],
                            },
                        }),
                        graphics: Vec::new(),
                        visible: true,
                    },
                    dimensions: dimensions(0.0, 0.0, 2.0, 1.0),
                    visuals: transparent_box_visuals(),
                    positioning: Positioning::default(),
                    flow: LayoutFlow::Block,
                    scroll_container: None,
                    contents: Vec::new(),
                })],
            },
            scroll_height: 10.0,
            links: Vec::new(),
            form_controls: Vec::new(),
        }
    }

    fn line_fragment_layout_tree() -> LayoutTree {
        LayoutTree {
            root: LayoutBox {
                kind: LayoutKind::Document,
                dimensions: dimensions(0.0, 0.0, 100.0, 60.0),
                visuals: transparent_box_visuals(),
                positioning: Positioning::default(),
                flow: LayoutFlow::Block,
                scroll_container: None,
                contents: vec![LayoutItem::LineBox(LineBox {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 54.0,
                        height: 18.0,
                    },
                    baseline: 14.0,
                    fragments: vec![
                        InlineFragment::Text(TextRun {
                            rect: Rect {
                                x: 0.0,
                                y: 0.0,
                                width: 20.0,
                                height: 16.0,
                            },
                            text: "first".to_string(),
                            font_size: 16.0,
                            visuals: plain_text_visuals(),
                        }),
                        InlineFragment::Box(LayoutBox {
                            kind: LayoutKind::Image {
                                tag_name: "img".to_string(),
                                metadata: Box::new(ElementMetadata::default()),
                                src: None,
                                alt: None,
                                image: None,
                                graphics: Vec::new(),
                                visible: true,
                            },
                            dimensions: dimensions(22.0, 0.0, 10.0, 10.0),
                            visuals: transparent_box_visuals(),
                            positioning: Positioning::default(),
                            flow: LayoutFlow::Block,
                            scroll_container: None,
                            contents: Vec::new(),
                        }),
                        InlineFragment::Text(TextRun {
                            rect: Rect {
                                x: 34.0,
                                y: 0.0,
                                width: 20.0,
                                height: 16.0,
                            },
                            text: "last".to_string(),
                            font_size: 16.0,
                            visuals: plain_text_visuals(),
                        }),
                    ],
                })],
            },
            scroll_height: 60.0,
            links: Vec::new(),
            form_controls: Vec::new(),
        }
    }

    fn dimensions(x: f32, y: f32, width: f32, height: f32) -> Dimensions {
        Dimensions {
            content: Rect {
                x,
                y,
                width,
                height,
            },
            padding: EdgeSizes::ZERO,
            border: EdgeSizes::ZERO,
            margin: EdgeSizes::ZERO,
        }
    }

    fn transparent_box_visuals() -> BoxVisuals {
        BoxVisuals {
            background_color: VisualColor {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            border: BorderVisuals {
                color: VisualColor {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
            },
        }
    }

    fn plain_text_visuals() -> TextVisuals {
        TextVisuals {
            color: VisualColor {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            font_weight: webby_layout::FontWeight::Normal,
            font_family: webby_layout::FontFamily::Sans,
            text_decoration: webby_layout::TextDecoration::None,
            is_link: false,
            href: None,
            link_node_id: None,
        }
    }

    fn layout_from_html(html: &str) -> WebbyResult<LayoutTree> {
        let document = webby_html::parse_document(html)?;
        let styled = webby_style::style_document(&document);
        webby_layout::layout_tree(&styled, Viewport::with_width(240.0)?)
    }

    fn ordered_text(list: &super::DisplayList) -> Vec<String> {
        list.commands
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::DrawText { text, .. } if !text.trim().is_empty() => {
                    Some(text.clone())
                }
                _ => None,
            })
            .collect()
    }

    fn ordered_text_and_images(list: &super::DisplayList) -> Vec<String> {
        list.commands
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::DrawText { text, .. } if !text.trim().is_empty() => {
                    Some(format!("text:{}", text.trim()))
                }
                DisplayCommand::ImagePlaceholder { .. } | DisplayCommand::DrawImage { .. } => {
                    Some("image".to_string())
                }
                _ => None,
            })
            .collect()
    }

    fn dump_layout_contains_image_box(layout_box: &LayoutBox) -> bool {
        if matches!(layout_box.kind, LayoutKind::Image { .. }) {
            return true;
        }

        layout_box.contents.iter().any(|item| match item {
            LayoutItem::Box(child) => dump_layout_contains_image_box(child),
            LayoutItem::LineBox(line) => line.fragments.iter().any(|fragment| match fragment {
                InlineFragment::Box(child) => dump_layout_contains_image_box(child),
                InlineFragment::Text(_) => false,
            }),
            LayoutItem::Text(_) => false,
        })
    }

    fn pixel(surface: &Surface, x: usize, y: usize) -> [u8; 4] {
        let index = (y * surface.width + x) * 4;
        [
            surface.pixels[index],
            surface.pixels[index + 1],
            surface.pixels[index + 2],
            surface.pixels[index + 3],
        ]
    }

    fn command_index(
        list: &super::DisplayList,
        matches_command: impl Fn(&DisplayCommand) -> bool,
    ) -> usize {
        for (index, command) in list.commands.iter().enumerate() {
            if matches_command(command) {
                return index;
            }
        }

        list.commands.len()
    }
}
