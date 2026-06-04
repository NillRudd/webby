//! Block and inline layout for styled Webby documents.
//!
//! This crate consumes `webby_style` trees and produces deterministic layout
//! geometry. It does not parse HTML, fetch resources, or draw pixels.

use webby_core::{WebbyError, WebbyResult};
use webby_style::{
    AlignItems as StyleAlignItems, BoxSizing as StyleBoxSizing, Color as StyleColor, CssSize,
    Display, Edges, FlexDirection as StyleFlexDirection, FontFamily as StyleFontFamily,
    FontWeight as StyleFontWeight, GridPlacement as StyleGridPlacement,
    GridTrack as StyleGridTrack, JustifyContent as StyleJustifyContent, Overflow as StyleOverflow,
    Position as StylePosition, StyledNode, TextAlign as StyleTextAlign,
    TextDecoration as StyleTextDecoration, Visibility as StyleVisibility,
    WhiteSpace as StyleWhiteSpace,
};
use webby_text::{FontFamily as TextFontFamily, FontWeight as TextFontWeight};

const DEFAULT_VIEWPORT_HEIGHT: f32 = 600.0;
const DEFAULT_IMAGE_WIDTH: f32 = 300.0;
const DEFAULT_IMAGE_HEIGHT: f32 = 150.0;

/// Rectangular viewport available to the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Viewport width in pixels.
    pub width: f32,
    /// Viewport height in pixels.
    pub height: f32,
}

impl Viewport {
    /// Creates a viewport when dimensions are finite and positive.
    pub fn new(width: f32, height: f32) -> WebbyResult<Self> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(WebbyError::invalid_input(
                "viewport width and height must be finite positive numbers",
            ));
        }

        Ok(Self { width, height })
    }

    /// Creates a viewport with Webby's default debug height.
    pub fn with_width(width: f32) -> WebbyResult<Self> {
        Self::new(width, DEFAULT_VIEWPORT_HEIGHT)
    }
}

/// Rectangle in layout coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left coordinate.
    pub x: f32,
    /// Top coordinate.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// Overflow behavior preserved for render and hit-testing consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Content may extend beyond the viewport.
    Visible,
    /// Content is clipped without scrolling.
    Hidden,
    /// Content is always scrollable.
    Scroll,
    /// Content is scrollable when it exceeds the viewport.
    Auto,
}

impl From<StyleOverflow> for Overflow {
    fn from(value: StyleOverflow) -> Self {
        match value {
            StyleOverflow::Visible => Self::Visible,
            StyleOverflow::Hidden => Self::Hidden,
            StyleOverflow::Scroll => Self::Scroll,
            StyleOverflow::Auto => Self::Auto,
        }
    }
}

/// Mutable app-owned offset for one layout-owned scroll container.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollOffset {
    /// Horizontal scroll offset.
    pub x: f32,
    /// Vertical scroll offset.
    pub y: f32,
}

/// Stable per-container scroll offsets keyed by DOM node id.
pub type ScrollOffsets = std::collections::BTreeMap<u64, ScrollOffset>;

/// Layout-owned geometry for one clipping or scrolling block container.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollContainer {
    /// Stable DOM node id.
    pub id: u64,
    /// Visible content viewport.
    pub viewport: Rect,
    /// Horizontal overflow behavior.
    pub overflow_x: Overflow,
    /// Vertical overflow behavior.
    pub overflow_y: Overflow,
    /// Maximum horizontal scroll offset.
    pub max_scroll_x: f32,
    /// Maximum vertical scroll offset.
    pub max_scroll_y: f32,
    /// Link range affected by this container.
    pub link_range: std::ops::Range<usize>,
    /// Form-control range affected by this container.
    pub control_range: std::ops::Range<usize>,
}

/// Visible scroll-container viewport after ancestor offsets and clips.
#[derive(Debug, Clone, PartialEq)]
pub struct VisibleScrollContainer {
    /// Stable DOM node id.
    pub id: u64,
    /// Visible clipped viewport.
    pub viewport: Rect,
    /// Maximum vertical offset.
    pub max_scroll_y: f32,
}

impl ScrollContainer {
    /// Clamps an app-owned offset to this container's supported axes.
    pub fn clamp_offset(&self, offset: ScrollOffset) -> ScrollOffset {
        ScrollOffset {
            x: if matches!(self.overflow_x, Overflow::Scroll | Overflow::Auto) {
                offset.x.clamp(0.0, self.max_scroll_x)
            } else {
                0.0
            },
            y: if matches!(self.overflow_y, Overflow::Scroll | Overflow::Auto) {
                offset.y.clamp(0.0, self.max_scroll_y)
            } else {
                0.0
            },
        }
    }
}

/// Four-sided sizes used by the CSS box model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSizes {
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Left edge.
    pub left: f32,
}

impl EdgeSizes {
    /// Zero on every side.
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    fn horizontal(self) -> f32 {
        self.left + self.right
    }

    fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

impl From<Edges> for EdgeSizes {
    fn from(value: Edges) -> Self {
        Self {
            top: value.top,
            right: value.right,
            bottom: value.bottom,
            left: value.left,
        }
    }
}

/// Complete CSS box-model dimensions for a layout box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dimensions {
    /// Content rectangle.
    pub content: Rect,
    /// Padding edges.
    pub padding: EdgeSizes,
    /// Border edges.
    pub border: EdgeSizes,
    /// Margin edges.
    pub margin: EdgeSizes,
}

/// Layout-facing positioning metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Positioning {
    /// CSS position mode.
    pub mode: PositionMode,
    /// Resolved visual x offset from normal flow.
    pub offset_x: f32,
    /// Resolved visual y offset from normal flow.
    pub offset_y: f32,
}

/// Supported positioning modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionMode {
    /// Normal document flow.
    Static,
    /// Normal flow with visual offset.
    Relative,
    /// Removed from normal block flow.
    Absolute,
}

/// Layout algorithm used for a box's children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutFlow {
    /// Normal block/inline flow.
    Block,
    /// Flex formatting context.
    Flex,
    /// Grid formatting context.
    Grid,
    /// Simple table formatting context.
    Table,
}

impl LayoutFlow {
    fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Flex => "flex",
            Self::Grid => "grid",
            Self::Table => "table",
        }
    }
}

impl PositionMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Relative => "relative",
            Self::Absolute => "absolute",
        }
    }
}

impl From<StylePosition> for PositionMode {
    fn from(value: StylePosition) -> Self {
        match value {
            StylePosition::Static => Self::Static,
            StylePosition::Relative => Self::Relative,
            StylePosition::Absolute => Self::Absolute,
        }
    }
}

impl Default for Positioning {
    fn default() -> Self {
        Self {
            mode: PositionMode::Static,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

impl Dimensions {
    /// Border-box rectangle.
    pub fn border_box(self) -> Rect {
        Rect {
            x: self.content.x - self.padding.left - self.border.left,
            y: self.content.y - self.padding.top - self.border.top,
            width: self.content.width + self.padding.horizontal() + self.border.horizontal(),
            height: self.content.height + self.padding.vertical() + self.border.vertical(),
        }
    }

    /// Margin-box rectangle.
    pub fn margin_box(self) -> Rect {
        let border_box = self.border_box();
        Rect {
            x: border_box.x - self.margin.left,
            y: border_box.y - self.margin.top,
            width: border_box.width + self.margin.horizontal(),
            height: border_box.height + self.margin.vertical(),
        }
    }
}

/// Source element metadata useful for inspection/debug tooling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ElementMetadata {
    /// Stable DOM node id, when this box maps to a source element.
    pub node_id: Option<u64>,
    /// Element id attribute, if present.
    pub id: Option<String>,
    /// Element classes split on ASCII whitespace, in source order.
    pub classes: Vec<String>,
}

/// Type and source information for a layout box.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutKind {
    /// Synthetic document root.
    Document,
    /// Block-level element.
    Block {
        /// Source tag name.
        tag_name: String,
        /// Source id/classes.
        metadata: Box<ElementMetadata>,
    },
    /// Inline replaced image element.
    Image {
        /// Source tag name.
        tag_name: String,
        /// Source id/classes.
        metadata: Box<ElementMetadata>,
        /// `src` attribute as written in HTML.
        src: Option<String>,
        /// `alt` attribute as written in HTML.
        alt: Option<String>,
        /// Decoded image metadata and pixels, when loading/decoding succeeded.
        image: Option<ImageResource>,
        /// Inline SVG or canvas drawing commands, in paint order.
        graphics: Vec<GraphicCommand>,
        /// Whether the image participates in painting and interaction.
        visible: bool,
    },
    /// Inline form control.
    FormControl {
        /// Source tag name.
        tag_name: String,
        /// Source id/classes.
        metadata: Box<ElementMetadata>,
        /// Form control kind.
        control_type: FormControlType,
        /// Form owner metadata.
        form: Option<FormMetadata>,
        /// Control name.
        name: Option<String>,
        /// Current/default value.
        value: String,
        /// Placeholder text for text/search inputs.
        placeholder: Option<String>,
        /// Whether this control is disabled.
        disabled: bool,
        /// Initial checked state for checkbox/radio controls.
        checked: bool,
        /// Select option values in document order.
        options: Vec<FormOption>,
    },
}

/// Select option metadata in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormOption {
    /// Submitted value.
    pub value: String,
    /// Visible label.
    pub label: String,
    /// Whether this option has the `selected` attribute.
    pub selected: bool,
    /// Whether this option is disabled.
    pub disabled: bool,
}

/// Deterministic vector/canvas drawing command carried from layout to render.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphicCommand {
    /// Fill a local rectangle.
    FillRect { rect: Rect, color: VisualColor },
    /// Stroke a local rectangle.
    StrokeRect {
        rect: Rect,
        color: VisualColor,
        width: f32,
    },
    /// Draw a local line.
    Line {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        color: VisualColor,
        width: f32,
    },
    /// Fill/stroke a local circle.
    Circle {
        cx: f32,
        cy: f32,
        radius: f32,
        fill: Option<VisualColor>,
        stroke: Option<VisualColor>,
        stroke_width: f32,
    },
}

/// Supported form control types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormControlType {
    /// Text input.
    Text,
    /// Search input.
    Search,
    /// Password input.
    Password,
    /// Email input.
    Email,
    /// Checkbox input.
    Checkbox,
    /// Radio input.
    Radio,
    /// Select dropdown.
    Select,
    /// Multi-line text area.
    Textarea,
    /// Submit input/button.
    Submit,
    /// Generic button.
    Button,
    /// Reset button.
    Reset,
}

/// Owning form metadata snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormMetadata {
    /// Stable form id assigned in layout order.
    pub id: usize,
    /// Stable DOM node id for the owning form element.
    pub node_id: u64,
    /// `action` attribute, if present.
    pub action: Option<String>,
    /// `method` attribute normalized to lowercase when present.
    pub method: Option<String>,
}

/// RGBA color snapshot owned by layout output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualColor {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

impl From<StyleColor> for VisualColor {
    fn from(value: StyleColor) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
            a: value.a,
        }
    }
}

/// Font weight snapshot for render-facing text runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// Normal text weight.
    Normal,
    /// Bold text weight.
    Bold,
}

impl From<StyleFontWeight> for FontWeight {
    fn from(value: StyleFontWeight) -> Self {
        match value {
            StyleFontWeight::Normal => Self::Normal,
            StyleFontWeight::Bold => Self::Bold,
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

/// Font family snapshot for render-facing text runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    /// Default sans/proportional family.
    Sans,
    /// Deterministic monospace family.
    Monospace,
}

impl From<StyleFontFamily> for FontFamily {
    fn from(value: StyleFontFamily) -> Self {
        match value {
            StyleFontFamily::Sans => Self::Sans,
            StyleFontFamily::Monospace => Self::Monospace,
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

/// Text decoration snapshot for render-facing text runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecoration {
    /// No decoration.
    None,
    /// Underline decoration.
    Underline,
}

impl From<StyleTextDecoration> for TextDecoration {
    fn from(value: StyleTextDecoration) -> Self {
        match value {
            StyleTextDecoration::None => Self::None,
            StyleTextDecoration::Underline => Self::Underline,
        }
    }
}

/// Border visuals needed by display-list generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderVisuals {
    /// Border color.
    pub color: VisualColor,
}

/// Box visuals needed by display-list generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxVisuals {
    /// Background fill color.
    pub background_color: VisualColor,
    /// Border visual data.
    pub border: BorderVisuals,
}

/// Text visuals needed by display-list generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextVisuals {
    /// Text color.
    pub color: VisualColor,
    /// Font weight.
    pub font_weight: FontWeight,
    /// Font family category.
    pub font_family: FontFamily,
    /// Text decoration.
    pub text_decoration: TextDecoration,
    /// Whether this text participates in link behavior.
    pub is_link: bool,
    /// Link target for linked text.
    pub href: Option<String>,
    /// Stable DOM node id for the owning link element.
    pub link_node_id: Option<u64>,
}

/// A laid-out text fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// Text bounds.
    pub rect: Rect,
    /// Visible text after whitespace collapsing.
    pub text: String,
    /// Font size used to measure this run.
    pub font_size: f32,
    /// Render-facing visual snapshot for this text.
    pub visuals: TextVisuals,
}

/// A single inline line box inside a block container.
#[derive(Debug, Clone, PartialEq)]
pub struct LineBox {
    /// Line bounds, including inline text and replaced elements.
    pub rect: Rect,
    /// Baseline offset from `rect.y`.
    pub baseline: f32,
    /// Ordered inline fragments on this line.
    pub fragments: Vec<InlineFragment>,
}

/// Ordered fragment inside a [`LineBox`].
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum InlineFragment {
    /// Text laid out on the line.
    Text(TextRun),
    /// Inline child box, currently used for image placeholders/resources.
    Box(LayoutBox),
}

/// Ordered content inside a layout box.
///
/// This is the authoritative paint/layout order consumed by rendering.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum LayoutItem {
    /// An inline line with ordered fragments.
    LineBox(LineBox),
    /// A text fragment.
    ///
    /// Compatibility item for hand-built tests and legacy callers. Layout
    /// produced by this crate emits inline text through [`LayoutItem::LineBox`].
    Text(TextRun),
    /// A child layout box.
    Box(LayoutBox),
}

/// A positioned layout box.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
    /// Source and box type.
    pub kind: LayoutKind,
    /// CSS box model dimensions.
    pub dimensions: Dimensions,
    /// Render-facing visual snapshot for this box.
    pub visuals: BoxVisuals,
    /// Positioning mode and resolved visual offsets.
    pub positioning: Positioning,
    /// Child layout algorithm.
    pub flow: LayoutFlow,
    /// Optional clipping/scrolling metadata for this block.
    pub scroll_container: Option<ScrollContainer>,
    /// Ordered content in paint/layout order.
    pub contents: Vec<LayoutItem>,
}

impl LayoutBox {
    /// Text runs in layout order.
    pub fn text_runs(&self) -> Vec<&TextRun> {
        let mut runs = Vec::new();
        collect_text_runs(&self.contents, &mut runs);
        runs
    }

    /// Child boxes in layout order.
    pub fn children(&self) -> Vec<&LayoutBox> {
        let mut children = Vec::new();
        collect_child_boxes(&self.contents, &mut children);
        children
    }
}

fn collect_text_runs<'a>(items: &'a [LayoutItem], runs: &mut Vec<&'a TextRun>) {
    for item in items {
        match item {
            LayoutItem::LineBox(line) => {
                for fragment in &line.fragments {
                    if let InlineFragment::Text(run) = fragment {
                        runs.push(run);
                    }
                }
            }
            LayoutItem::Text(run) => runs.push(run),
            LayoutItem::Box(_) => {}
        }
    }
}

fn collect_child_boxes<'a>(items: &'a [LayoutItem], children: &mut Vec<&'a LayoutBox>) {
    for item in items {
        match item {
            LayoutItem::LineBox(line) => {
                for fragment in &line.fragments {
                    if let InlineFragment::Box(layout_box) = fragment {
                        children.push(layout_box);
                    }
                }
            }
            LayoutItem::Text(_) => {}
            LayoutItem::Box(layout_box) => children.push(layout_box),
        }
    }
}

/// Clickable link hit area.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkHitBox {
    /// Clickable bounds.
    pub rect: Rect,
    /// Link target as written or resolved by later milestones.
    pub href: String,
    /// Stable DOM node id for the link element.
    pub node_id: Option<u64>,
}

/// Interactive form control hit area.
#[derive(Debug, Clone, PartialEq)]
pub struct FormControlHitBox {
    /// Stable control id assigned in layout order.
    pub id: usize,
    /// Clickable bounds.
    pub rect: Rect,
    /// Control type.
    pub control_type: FormControlType,
    /// Owning form metadata.
    pub form: Option<FormMetadata>,
    /// Control name.
    pub name: Option<String>,
    /// Control value.
    pub value: String,
    /// Placeholder text.
    pub placeholder: Option<String>,
    /// Whether this control is disabled.
    pub disabled: bool,
    /// Initial checked state for checkbox/radio controls.
    pub checked: bool,
    /// Select option values in document order.
    pub options: Vec<FormOption>,
}

/// Decoded image pixels supplied to layout by higher-level pipeline code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// Intrinsic pixel width.
    pub width: u32,
    /// Intrinsic pixel height.
    pub height: u32,
    /// RGBA8 pixels in row-major order.
    pub pixels: Vec<u8>,
}

/// Decoded image metadata keyed by the HTML `src` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResource {
    /// Source string as written in HTML.
    pub src: String,
    /// Final resolved URL after resource loading.
    pub final_url: String,
    /// Decoded image data.
    pub image: DecodedImage,
}

/// Image resources available during layout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageMap {
    images: std::collections::BTreeMap<String, ImageResource>,
}

impl ImageMap {
    /// Creates an empty image map.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Inserts one decoded image resource.
    pub fn insert(&mut self, resource: ImageResource) {
        self.images.insert(resource.src.clone(), resource);
    }

    /// Returns a decoded image for an HTML `src` value.
    pub fn get(&self, src: &str) -> Option<&ImageResource> {
        self.images.get(src)
    }
}

/// Layout result for a document.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTree {
    /// Root layout box.
    pub root: LayoutBox,
    /// Total scrollable document height.
    pub scroll_height: f32,
    /// Link hit rectangles.
    pub links: Vec<LinkHitBox>,
    /// Form control hit rectangles.
    pub form_controls: Vec<FormControlHitBox>,
}

impl LayoutTree {
    /// Returns scroll/clipping containers in deterministic layout order.
    pub fn scroll_containers(&self) -> Vec<&ScrollContainer> {
        let mut containers = Vec::new();
        collect_scroll_containers(&self.root, &mut containers);
        containers
    }

    /// Projects link hit regions through nested offsets and clips.
    pub fn visible_links(&self, offsets: &ScrollOffsets) -> Vec<LinkHitBox> {
        self.links
            .iter()
            .enumerate()
            .filter_map(|(index, hit)| {
                project_hit_rect(&self.root, hit.rect, offsets, HitRegion::Link(index)).map(
                    |rect| LinkHitBox {
                        rect,
                        href: hit.href.clone(),
                        node_id: hit.node_id,
                    },
                )
            })
            .collect()
    }

    /// Projects form hit regions through nested offsets and clips.
    pub fn visible_form_controls(&self, offsets: &ScrollOffsets) -> Vec<FormControlHitBox> {
        self.form_controls
            .iter()
            .enumerate()
            .filter_map(|(index, hit)| {
                project_hit_rect(&self.root, hit.rect, offsets, HitRegion::Control(index)).map(
                    |rect| {
                        let mut projected = hit.clone();
                        projected.rect = rect;
                        projected
                    },
                )
            })
            .collect()
    }

    /// Projects scroll-container viewports for pointer-based wheel routing.
    pub fn visible_scroll_containers(
        &self,
        offsets: &ScrollOffsets,
    ) -> Vec<VisibleScrollContainer> {
        let mut visible = Vec::new();
        collect_visible_scroll_containers(&self.root, offsets, 0.0, 0.0, None, &mut visible);
        visible
    }
}

fn collect_visible_scroll_containers(
    layout_box: &LayoutBox,
    offsets: &ScrollOffsets,
    mut dx: f32,
    mut dy: f32,
    mut clip: Option<Rect>,
    visible: &mut Vec<VisibleScrollContainer>,
) {
    if let Some(container) = &layout_box.scroll_container {
        let viewport = translate_rect(container.viewport, dx, dy);
        let Some(viewport) = clip
            .map(|current| intersect_rect(current, viewport))
            .unwrap_or(Some(viewport))
        else {
            return;
        };
        visible.push(VisibleScrollContainer {
            id: container.id,
            viewport,
            max_scroll_y: container.max_scroll_y,
        });
        clip = Some(viewport);
        let offset =
            container.clamp_offset(offsets.get(&container.id).copied().unwrap_or_default());
        dx -= offset.x;
        dy -= offset.y;
    }
    for_each_child_box(layout_box, |child| {
        collect_visible_scroll_containers(child, offsets, dx, dy, clip, visible);
    });
}

#[derive(Debug, Clone, Copy)]
enum HitRegion {
    Link(usize),
    Control(usize),
}

fn collect_scroll_containers<'a>(
    layout_box: &'a LayoutBox,
    containers: &mut Vec<&'a ScrollContainer>,
) {
    if let Some(container) = &layout_box.scroll_container {
        containers.push(container);
    }
    for_each_child_box(layout_box, |child| {
        collect_scroll_containers(child, containers)
    });
}

fn project_hit_rect(
    layout_box: &LayoutBox,
    rect: Rect,
    offsets: &ScrollOffsets,
    region: HitRegion,
) -> Option<Rect> {
    let mut dx = 0.0;
    let mut dy = 0.0;
    let mut clip = None;
    for container in {
        let mut containers = Vec::new();
        collect_scroll_containers(layout_box, &mut containers);
        containers
    } {
        if !container_contains_region(container, region) {
            continue;
        }
        let viewport = translate_rect(container.viewport, dx, dy);
        clip = Some(match clip {
            Some(current) => intersect_rect(current, viewport)?,
            None => viewport,
        });
        let offset =
            container.clamp_offset(offsets.get(&container.id).copied().unwrap_or_default());
        dx -= offset.x;
        dy -= offset.y;
    }
    let projected = translate_rect(rect, dx, dy);
    match clip {
        Some(clip) => intersect_rect(projected, clip),
        None => Some(projected),
    }
}

fn container_contains_region(container: &ScrollContainer, region: HitRegion) -> bool {
    match region {
        HitRegion::Link(index) => container.link_range.contains(&index),
        HitRegion::Control(index) => container.control_range.contains(&index),
    }
}

fn for_each_child_box<'a>(layout_box: &'a LayoutBox, mut visit: impl FnMut(&'a LayoutBox)) {
    for item in &layout_box.contents {
        match item {
            LayoutItem::LineBox(line) => {
                for fragment in &line.fragments {
                    if let InlineFragment::Box(child) = fragment {
                        visit(child);
                    }
                }
            }
            LayoutItem::Box(child) => visit(child),
            LayoutItem::Text(_) => {}
        }
    }
}

fn translate_rect(rect: Rect, dx: f32, dy: f32) -> Rect {
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

/// Builds a layout tree for styled content in the given viewport.
pub fn layout_tree(styled_root: &StyledNode<'_>, viewport: Viewport) -> WebbyResult<LayoutTree> {
    layout_tree_with_images(styled_root, viewport, &ImageMap::empty())
}

/// Builds a layout tree using decoded image metadata supplied by the caller.
pub fn layout_tree_with_images(
    styled_root: &StyledNode<'_>,
    viewport: Viewport,
    images: &ImageMap,
) -> WebbyResult<LayoutTree> {
    let containing_block = Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width,
        height: 0.0,
    };
    let mut state = LayoutState::default();
    let root = layout_block(
        styled_root,
        LayoutKind::Document,
        containing_block,
        0.0,
        images,
        None,
        &mut state,
    )?;
    let scroll_height = root.dimensions.margin_box().height.max(viewport.height);

    Ok(LayoutTree {
        root,
        scroll_height,
        links: state.links,
        form_controls: state.form_controls,
    })
}

/// Formats a layout tree for deterministic CLI/debug output.
pub fn dump_layout_tree(tree: &LayoutTree) -> String {
    let mut output = String::new();
    output.push_str("layout");
    output.push_str(" scroll-height=");
    output.push_str(&format_px(tree.scroll_height));
    output.push('\n');
    dump_box(&tree.root, 1, &mut output);
    if !tree.links.is_empty() {
        output.push_str("  links\n");
        for link in &tree.links {
            output.push_str("    link href=\"");
            output.push_str(&escape_dump_string(&link.href));
            output.push_str("\" rect=");
            output.push_str(&format_rect(link.rect));
            output.push('\n');
        }
    }
    if !tree.form_controls.is_empty() {
        output.push_str("  form-controls\n");
        for control in &tree.form_controls {
            output.push_str("    control id=");
            output.push_str(&control.id.to_string());
            output.push_str(" type=");
            output.push_str(control_type_name(control.control_type));
            output.push_str(" rect=");
            output.push_str(&format_rect(control.rect));
            if let Some(form) = &control.form {
                output.push_str(" form=");
                output.push_str(&form.id.to_string());
            }
            if let Some(name) = &control.name {
                output.push_str(" name=\"");
                output.push_str(&escape_dump_string(name));
                output.push('"');
            }
            output.push('\n');
        }
    }
    output
}

fn layout_block(
    node: &StyledNode<'_>,
    kind: LayoutKind,
    containing_block: Rect,
    top: f32,
    images: &ImageMap,
    inherited_form: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<LayoutBox> {
    let link_start = state.links.len();
    let control_start = state.form_controls.len();
    let form_context = form_context_for_node(node, inherited_form, state);
    let margin = EdgeSizes::from(node.style.margin);
    let padding = EdgeSizes::from(node.style.padding);
    let border = EdgeSizes {
        top: node.style.border.width,
        right: node.style.border.width,
        bottom: node.style.border.width,
        left: node.style.border.width,
    };
    let content_x = containing_block.x + margin.left + border.left + padding.left;
    let content_y = top + margin.top + border.top + padding.top;
    let used_horizontal = margin.horizontal() + border.horizontal() + padding.horizontal();
    let available_content_width = (containing_block.width - used_horizontal).max(0.0);
    let content_width = resolve_content_width(
        node,
        containing_block.width,
        available_content_width,
        border,
        padding,
    );
    let mut cursor_y = content_y;
    let mut contents = Vec::new();
    let mut inline_nodes = Vec::new();

    append_list_marker(node, content_x, content_y, &mut contents);

    if is_preformatted(node) {
        cursor_y = layout_preformatted_text(
            node,
            Rect {
                x: content_x,
                y: content_y,
                width: content_width,
                height: 0.0,
            },
            &mut contents,
        );
    } else if is_table(node) {
        cursor_y = layout_table_children(
            node,
            Rect {
                x: content_x,
                y: content_y,
                width: content_width,
                height: 0.0,
            },
            &mut contents,
            images,
            form_context,
            state,
        )?;
    } else if node.style.display == Display::Flex {
        cursor_y = layout_flex_children(
            node,
            Rect {
                x: content_x,
                y: content_y,
                width: content_width,
                height: 0.0,
            },
            &mut contents,
            images,
            form_context,
            state,
        )?;
    } else if node.style.display == Display::Grid {
        cursor_y = layout_grid_children(
            node,
            Rect {
                x: content_x,
                y: content_y,
                width: content_width,
                height: 0.0,
            },
            &mut contents,
            images,
            form_context,
            state,
        )?;
    } else {
        for child in &node.children {
            let block_form_control =
                child.style.display == Display::Block && is_form_control_node(child);
            if block_form_control {
                cursor_y = flush_inline_nodes(
                    &inline_nodes,
                    Rect {
                        x: content_x,
                        y: cursor_y,
                        width: content_width,
                        height: 0.0,
                    },
                    &mut contents,
                    InlineFlushContext {
                        images,
                        form_context: form_context.clone(),
                        inherited_link: link_ref(node),
                        text_align: node.style.text_align,
                        state,
                    },
                )?;
                inline_nodes.clear();
                inline_nodes.push(child);
                cursor_y = flush_inline_nodes(
                    &inline_nodes,
                    Rect {
                        x: content_x,
                        y: cursor_y,
                        width: content_width,
                        height: 0.0,
                    },
                    &mut contents,
                    InlineFlushContext {
                        images,
                        form_context: form_context.clone(),
                        inherited_link: link_ref(node),
                        text_align: node.style.text_align,
                        state,
                    },
                )?;
                inline_nodes.clear();
            } else if matches!(
                child.style.display,
                Display::Block | Display::Flex | Display::Grid
            ) {
                cursor_y = flush_inline_nodes(
                    &inline_nodes,
                    Rect {
                        x: content_x,
                        y: cursor_y,
                        width: content_width,
                        height: 0.0,
                    },
                    &mut contents,
                    InlineFlushContext {
                        images,
                        form_context: form_context.clone(),
                        inherited_link: link_ref(node),
                        text_align: node.style.text_align,
                        state,
                    },
                )?;
                inline_nodes.clear();

                let child_box = layout_block_child(
                    child,
                    Rect {
                        x: content_x,
                        y: content_y,
                        width: content_width,
                        height: 0.0,
                    },
                    cursor_y,
                    images,
                    form_context.clone(),
                    state,
                )?;
                let normal_margin_box = child_box.dimensions.margin_box();
                if child_box.positioning.mode != PositionMode::Absolute {
                    cursor_y = normal_margin_box.y - child_box.positioning.offset_y
                        + normal_margin_box.height;
                }
                contents.push(LayoutItem::Box(child_box));
            } else {
                inline_nodes.push(child);
            }
        }

        cursor_y = flush_inline_nodes(
            &inline_nodes,
            Rect {
                x: content_x,
                y: cursor_y,
                width: content_width,
                height: 0.0,
            },
            &mut contents,
            InlineFlushContext {
                images,
                form_context,
                inherited_link: link_ref(node),
                text_align: node.style.text_align,
                state,
            },
        )?;
    }

    let auto_content_height = (cursor_y - content_y).max(0.0);
    let content_height = resolve_content_height(
        node,
        containing_block.height,
        auto_content_height,
        border,
        padding,
    );
    let dimensions = Dimensions {
        content: Rect {
            x: content_x,
            y: content_y,
            width: content_width,
            height: content_height,
        },
        padding,
        border,
        margin,
    };
    let scroll_container = scroll_container_for(
        node,
        dimensions.content,
        &contents,
        link_start..state.links.len(),
        control_start..state.form_controls.len(),
    );

    Ok(LayoutBox {
        kind,
        dimensions,
        visuals: box_visuals(node),
        positioning: Positioning::default(),
        flow: flow_for(node),
        scroll_container,
        contents,
    })
}

fn scroll_container_for(
    node: &StyledNode<'_>,
    viewport: Rect,
    contents: &[LayoutItem],
    link_range: std::ops::Range<usize>,
    control_range: std::ops::Range<usize>,
) -> Option<ScrollContainer> {
    let overflow_x = Overflow::from(node.style.overflow_x);
    let overflow_y = Overflow::from(node.style.overflow_y);
    if overflow_x == Overflow::Visible && overflow_y == Overflow::Visible {
        return None;
    }
    let bounds = content_bounds(contents).unwrap_or(viewport);
    Some(ScrollContainer {
        id: node.node.id,
        viewport,
        overflow_x,
        overflow_y,
        max_scroll_x: (bounds.x + bounds.width - viewport.x - viewport.width).max(0.0),
        max_scroll_y: (bounds.y + bounds.height - viewport.y - viewport.height).max(0.0),
        link_range,
        control_range,
    })
}

fn content_bounds(contents: &[LayoutItem]) -> Option<Rect> {
    let mut bounds = None;
    for item in contents {
        let rect = match item {
            LayoutItem::LineBox(line) => line.rect,
            LayoutItem::Text(run) => run.rect,
            LayoutItem::Box(child) => child.dimensions.margin_box(),
        };
        bounds = Some(union_rect(bounds.unwrap_or(rect), rect));
    }
    bounds
}

fn union_rect(left: Rect, right: Rect) -> Rect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    Rect {
        x,
        y,
        width: (left.x + left.width).max(right.x + right.width) - x,
        height: (left.y + left.height).max(right.y + right.height) - y,
    }
}

fn is_form_control_node(node: &StyledNode<'_>) -> bool {
    matches!(
        node.tag_name(),
        Some("input" | "button" | "select" | "textarea")
    )
}

fn positioning_for(node: &StyledNode<'_>) -> Positioning {
    Positioning {
        mode: PositionMode::from(node.style.position),
        offset_x: 0.0,
        offset_y: 0.0,
    }
}

fn flow_for(node: &StyledNode<'_>) -> LayoutFlow {
    if is_table(node) {
        LayoutFlow::Table
    } else if node.style.display == Display::Flex {
        LayoutFlow::Flex
    } else if node.style.display == Display::Grid {
        LayoutFlow::Grid
    } else {
        LayoutFlow::Block
    }
}

fn layout_block_child(
    child: &StyledNode<'_>,
    containing_block: Rect,
    top: f32,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<LayoutBox> {
    let link_start = state.links.len();
    let control_start = state.form_controls.len();
    let mut child_box = layout_block(
        child,
        block_kind(child),
        containing_block,
        top,
        images,
        form_context,
        state,
    )?;
    let positioning = positioning_for(child);
    if positioning.mode == PositionMode::Absolute {
        apply_absolute_position(
            &mut child_box,
            child,
            positioning,
            containing_block,
            state,
            link_start,
            control_start,
        );
    } else {
        apply_relative_position(
            &mut child_box,
            child,
            positioning,
            containing_block.width,
            state,
            link_start,
            control_start,
        );
    }
    Ok(child_box)
}

fn append_list_marker(
    node: &StyledNode<'_>,
    content_x: f32,
    content_y: f32,
    contents: &mut Vec<LayoutItem>,
) {
    if node.tag_name() != Some("li") {
        return;
    }

    let marker = "• ".to_string();
    let font_weight = FontWeight::from(node.style.font_weight);
    let font_family = FontFamily::from(node.style.font_family);
    let width = measure_text(&marker, node.style.font_size, font_weight, font_family);
    let height = line_height_with_override(node.style.font_size, node.style.line_height);
    contents.push(LayoutItem::LineBox(LineBox {
        rect: Rect {
            x: content_x - width,
            y: content_y,
            width,
            height,
        },
        baseline: text_baseline(node.style.font_size),
        fragments: vec![InlineFragment::Text(TextRun {
            rect: Rect {
                x: content_x - width,
                y: content_y,
                width,
                height,
            },
            text: marker,
            font_size: node.style.font_size,
            visuals: text_visuals(node, None),
        })],
    }));
}

fn layout_preformatted_text(
    node: &StyledNode<'_>,
    content_rect: Rect,
    contents: &mut Vec<LayoutItem>,
) -> f32 {
    let text = collect_raw_text(node);
    if text.is_empty() {
        return content_rect.y;
    }

    let font_weight = FontWeight::from(node.style.font_weight);
    let font_family = FontFamily::from(node.style.font_family);
    let line_height = line_height_for(node);
    let baseline = text_baseline(node.style.font_size);
    let visuals = text_visuals(node, link_ref(node));
    let mut cursor_y = content_rect.y;
    for line in text.split('\n') {
        let width = measure_text(line, node.style.font_size, font_weight, font_family);
        let run = TextRun {
            rect: Rect {
                x: content_rect.x,
                y: cursor_y,
                width,
                height: line_height,
            },
            text: line.to_string(),
            font_size: node.style.font_size,
            visuals: visuals.clone(),
        };
        contents.push(LayoutItem::LineBox(LineBox {
            rect: run.rect,
            baseline,
            fragments: vec![InlineFragment::Text(run)],
        }));
        cursor_y += line_height;
    }
    cursor_y
}

fn collect_raw_text(node: &StyledNode<'_>) -> String {
    let mut output = String::new();
    collect_raw_text_into(node, &mut output);
    output
}

fn collect_raw_text_into(node: &StyledNode<'_>, output: &mut String) {
    if let Some(text) = node.text() {
        output.push_str(text);
    }
    for child in &node.children {
        collect_raw_text_into(child, output);
    }
}

struct TableGrid {
    columns: usize,
    column_width: f32,
}

fn layout_table_children(
    node: &StyledNode<'_>,
    content_rect: Rect,
    contents: &mut Vec<LayoutItem>,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<f32> {
    let rows = collect_table_rows(node);
    let columns = rows
        .iter()
        .map(|row| table_cells(row).len())
        .max()
        .unwrap_or(0);
    if columns == 0 {
        return Ok(content_rect.y);
    }
    let grid = TableGrid {
        columns,
        column_width: content_rect.width / columns as f32,
    };
    let mut cursor_y = content_rect.y;

    for child in visible_children(node) {
        if is_table_section(child) {
            let section = layout_table_section(
                child,
                content_rect,
                cursor_y,
                &grid,
                images,
                form_context.clone(),
                state,
            )?;
            cursor_y = section.dimensions.margin_box().y + section.dimensions.margin_box().height;
            contents.push(LayoutItem::Box(section));
        } else if is_table_row(child) {
            let row = layout_table_row(
                child,
                content_rect,
                cursor_y,
                &grid,
                images,
                form_context.clone(),
                state,
            )?;
            cursor_y = row.dimensions.margin_box().y + row.dimensions.margin_box().height;
            contents.push(LayoutItem::Box(row));
        }
    }

    Ok(cursor_y)
}

fn layout_table_section(
    node: &StyledNode<'_>,
    containing_block: Rect,
    top: f32,
    grid: &TableGrid,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<LayoutBox> {
    let margin = EdgeSizes::from(node.style.margin);
    let padding = EdgeSizes::from(node.style.padding);
    let border = EdgeSizes {
        top: node.style.border.width,
        right: node.style.border.width,
        bottom: node.style.border.width,
        left: node.style.border.width,
    };
    let content_x = containing_block.x + margin.left + border.left + padding.left;
    let content_y = top + margin.top + border.top + padding.top;
    let content_width =
        (containing_block.width - margin.horizontal() - border.horizontal() - padding.horizontal())
            .max(0.0);
    let mut cursor_y = content_y;
    let mut contents = Vec::new();

    for row_node in visible_children(node)
        .into_iter()
        .filter(|child| is_table_row(child))
    {
        let row = layout_table_row(
            row_node,
            Rect {
                x: content_x,
                y: content_y,
                width: content_width,
                height: 0.0,
            },
            cursor_y,
            grid,
            images,
            form_context.clone(),
            state,
        )?;
        cursor_y = row.dimensions.margin_box().y + row.dimensions.margin_box().height;
        contents.push(LayoutItem::Box(row));
    }

    Ok(LayoutBox {
        kind: block_kind(node),
        dimensions: Dimensions {
            content: Rect {
                x: content_x,
                y: content_y,
                width: content_width,
                height: (cursor_y - content_y).max(0.0),
            },
            padding,
            border,
            margin,
        },
        visuals: box_visuals(node),
        positioning: Positioning::default(),
        flow: LayoutFlow::Table,
        scroll_container: None,
        contents,
    })
}

fn layout_table_row(
    node: &StyledNode<'_>,
    containing_block: Rect,
    top: f32,
    grid: &TableGrid,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<LayoutBox> {
    let margin = EdgeSizes::from(node.style.margin);
    let padding = EdgeSizes::from(node.style.padding);
    let border = EdgeSizes {
        top: node.style.border.width,
        right: node.style.border.width,
        bottom: node.style.border.width,
        left: node.style.border.width,
    };
    let content_x = containing_block.x + margin.left + border.left + padding.left;
    let content_y = top + margin.top + border.top + padding.top;
    let content_width =
        (containing_block.width - margin.horizontal() - border.horizontal() - padding.horizontal())
            .max(0.0);
    let mut contents = Vec::new();
    let mut row_height: f32 = 0.0;

    for (index, cell) in table_cells(node).into_iter().take(grid.columns).enumerate() {
        let x = content_x + index as f32 * grid.column_width;
        let cell_box = layout_block_child(
            cell,
            Rect {
                x,
                y: content_y,
                width: grid.column_width.min(content_width),
                height: 0.0,
            },
            content_y,
            images,
            form_context.clone(),
            state,
        )?;
        row_height = row_height.max(cell_box.dimensions.margin_box().height);
        contents.push(LayoutItem::Box(cell_box));
    }

    Ok(LayoutBox {
        kind: block_kind(node),
        dimensions: Dimensions {
            content: Rect {
                x: content_x,
                y: content_y,
                width: content_width,
                height: row_height,
            },
            padding,
            border,
            margin,
        },
        visuals: box_visuals(node),
        positioning: Positioning::default(),
        flow: LayoutFlow::Table,
        scroll_container: None,
        contents,
    })
}

fn collect_table_rows<'a>(node: &'a StyledNode<'a>) -> Vec<&'a StyledNode<'a>> {
    let mut rows = Vec::new();
    for child in visible_children(node) {
        if is_table_row(child) {
            rows.push(child);
        } else if is_table_section(child) {
            rows.extend(
                visible_children(child)
                    .into_iter()
                    .filter(|row| is_table_row(row)),
            );
        }
    }
    rows
}

fn visible_children<'a>(node: &'a StyledNode<'a>) -> Vec<&'a StyledNode<'a>> {
    node.children
        .iter()
        .filter(|child| child.style.display != Display::None)
        .collect()
}

fn table_cells<'a>(row: &'a StyledNode<'a>) -> Vec<&'a StyledNode<'a>> {
    visible_children(row)
        .into_iter()
        .filter(|child| matches!(child.tag_name(), Some("th" | "td")))
        .collect()
}

fn is_table(node: &StyledNode<'_>) -> bool {
    node.tag_name() == Some("table")
}

fn is_preformatted(node: &StyledNode<'_>) -> bool {
    node.tag_name() == Some("pre")
}

fn is_table_section(node: &StyledNode<'_>) -> bool {
    matches!(node.tag_name(), Some("thead" | "tbody"))
}

fn is_table_row(node: &StyledNode<'_>) -> bool {
    node.tag_name() == Some("tr")
}

#[derive(Debug, Clone, Copy)]
struct GridPlacement {
    column: usize,
    row: usize,
    column_span: usize,
    row_span: usize,
}

fn layout_grid_children(
    node: &StyledNode<'_>,
    content_rect: Rect,
    contents: &mut Vec<LayoutItem>,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<f32> {
    let children = visible_children(node);
    if children.is_empty() {
        return Ok(content_rect.y);
    }

    let placements = grid_placements(&children, node.style.grid_template_columns.len().max(1));
    let column_count = grid_column_count(node, &placements);
    let row_count = grid_row_count(node, &placements);
    let column_widths = resolve_grid_tracks(
        &node.style.grid_template_columns,
        column_count,
        content_rect.width,
        node.style.column_gap,
        false,
    );

    let mut measured = Vec::new();
    let mut row_heights = resolve_grid_tracks(
        &node.style.grid_template_rows,
        row_count,
        0.0,
        node.style.row_gap,
        true,
    );
    let mut measure_state = state.clone();
    for (index, child) in children.iter().enumerate() {
        let placement = placements.get(index).copied().unwrap_or(GridPlacement {
            column: 0,
            row: index,
            column_span: 1,
            row_span: 1,
        });
        let child_width = grid_span_size(
            &column_widths,
            placement.column,
            placement.column_span,
            node.style.column_gap,
        );
        let child_box = layout_grid_child_box(
            child,
            Rect {
                x: grid_track_offset(
                    content_rect.x,
                    &column_widths,
                    node.style.column_gap,
                    placement.column,
                ),
                y: content_rect.y,
                width: child_width,
                height: 0.0,
            },
            content_rect.y,
            images,
            form_context.clone(),
            &mut measure_state,
        )?;
        let height = child_box.dimensions.margin_box().height;
        if grid_track_is_auto(&node.style.grid_template_rows, placement.row)
            && placement.row < row_heights.len()
        {
            row_heights[placement.row] = row_heights[placement.row].max(height);
        }
        measured.push((child, placement));
    }

    let mut bottom = content_rect.y;
    for (child, placement) in measured {
        let cell_x = grid_track_offset(
            content_rect.x,
            &column_widths,
            node.style.column_gap,
            placement.column,
        );
        let cell_y = grid_track_offset(
            content_rect.y,
            &row_heights,
            node.style.row_gap,
            placement.row,
        );
        let child_width = grid_span_size(
            &column_widths,
            placement.column,
            placement.column_span,
            node.style.column_gap,
        );
        let child_height = grid_span_size(
            &row_heights,
            placement.row,
            placement.row_span,
            node.style.row_gap,
        );
        let child_box = layout_grid_child_box(
            child,
            Rect {
                x: cell_x,
                y: cell_y,
                width: child_width,
                height: child_height,
            },
            cell_y,
            images,
            form_context.clone(),
            state,
        )?;
        if child_box.positioning.mode != PositionMode::Absolute {
            let margin_box = child_box.dimensions.margin_box();
            bottom = bottom.max(margin_box.y + margin_box.height);
        }
        contents.push(LayoutItem::Box(child_box));
    }

    let template_bottom = content_rect.y
        + row_heights.iter().sum::<f32>()
        + node.style.row_gap * row_count.saturating_sub(1) as f32;
    Ok(bottom.max(template_bottom))
}

fn layout_grid_child_box(
    child: &StyledNode<'_>,
    containing_block: Rect,
    top: f32,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<LayoutBox> {
    if is_replaced_node(child) {
        return Ok(layout_replaced_grid_item(
            child,
            containing_block,
            top,
            images,
        ));
    }
    layout_block_child(child, containing_block, top, images, form_context, state)
}

fn is_replaced_node(node: &StyledNode<'_>) -> bool {
    matches!(
        node.tag_name(),
        Some("img" | "svg" | "canvas" | "audio" | "video" | "iframe")
    )
}

fn layout_replaced_grid_item(
    node: &StyledNode<'_>,
    containing_block: Rect,
    top: f32,
    images: &ImageMap,
) -> LayoutBox {
    let tag_name = node.tag_name().unwrap_or("img").to_string();
    let src = media_image_source(node).map(str::to_string);
    let visible = node.style.visibility == StyleVisibility::Visible;
    let image = if visible && (tag_name == "img" || tag_name == "video" || tag_name == "iframe") {
        src.as_deref().and_then(|value| images.get(value)).cloned()
    } else {
        None
    };
    let width = replaced_width(node, image.as_ref()).min(containing_block.width.max(0.0));
    let height = replaced_height(node, image.as_ref());
    let padding = EdgeSizes::from(node.style.padding);
    let border = EdgeSizes {
        top: node.style.border.width,
        right: node.style.border.width,
        bottom: node.style.border.width,
        left: node.style.border.width,
    };
    let margin = EdgeSizes::from(node.style.margin);
    let content = Rect {
        x: containing_block.x + margin.left + border.left + padding.left,
        y: top + margin.top + border.top + padding.top,
        width,
        height,
    };
    let graphics = if visible {
        graphic_commands_for_node(node, width, height)
    } else {
        Vec::new()
    };

    LayoutBox {
        kind: LayoutKind::Image {
            tag_name,
            metadata: Box::new(element_metadata(node)),
            src,
            alt: image_attr(node, "alt").map(str::to_string),
            image,
            graphics,
            visible,
        },
        dimensions: Dimensions {
            content,
            padding,
            border,
            margin,
        },
        visuals: box_visuals(node),
        positioning: Positioning::default(),
        flow: LayoutFlow::Block,
        scroll_container: None,
        contents: Vec::new(),
    }
}

fn grid_placements(children: &[&StyledNode<'_>], auto_columns: usize) -> Vec<GridPlacement> {
    let mut placements = Vec::new();
    let mut occupied = Vec::new();
    let mut cursor_row = 0usize;
    let mut cursor_column = 0usize;

    for child in children {
        let span = grid_span(child.style.grid_column, child.style.grid_row);
        let explicit_column = grid_start(child.style.grid_column);
        let explicit_row = grid_start(child.style.grid_row);
        let placement = match (explicit_column, explicit_row) {
            (Some(column), Some(row)) => GridPlacement {
                column,
                row,
                column_span: span.0,
                row_span: span.1,
            },
            (Some(column), None) => {
                let row = first_free_row(&occupied, column, span.0, span.1);
                GridPlacement {
                    column,
                    row,
                    column_span: span.0,
                    row_span: span.1,
                }
            }
            (None, Some(row)) => {
                let column = first_free_column(&occupied, row, span.0, span.1);
                GridPlacement {
                    column,
                    row,
                    column_span: span.0,
                    row_span: span.1,
                }
            }
            (None, None) => {
                let (column, row) = first_free_auto_cell(
                    &occupied,
                    cursor_column,
                    cursor_row,
                    span.0,
                    span.1,
                    auto_columns,
                );
                cursor_column = column.saturating_add(span.0);
                cursor_row = row.saturating_add(cursor_column / auto_columns.max(1));
                cursor_column %= auto_columns.max(1);
                GridPlacement {
                    column,
                    row,
                    column_span: span.0,
                    row_span: span.1,
                }
            }
        };
        mark_grid_cells(&mut occupied, placement);
        placements.push(placement);
    }

    placements
}

fn grid_start(placement: StyleGridPlacement) -> Option<usize> {
    placement.start.map(|line| line.saturating_sub(1))
}

fn grid_span(column: StyleGridPlacement, row: StyleGridPlacement) -> (usize, usize) {
    (grid_placement_span(column), grid_placement_span(row))
}

fn grid_placement_span(placement: StyleGridPlacement) -> usize {
    match (placement.start, placement.end) {
        (Some(start), Some(end)) if end > start => end - start,
        _ => 1,
    }
}

fn first_free_row(
    occupied: &[(usize, usize)],
    column: usize,
    column_span: usize,
    row_span: usize,
) -> usize {
    let mut row = 0usize;
    while grid_area_occupied(occupied, column, row, column_span, row_span) {
        row = row.saturating_add(1);
    }
    row
}

fn first_free_column(
    occupied: &[(usize, usize)],
    row: usize,
    column_span: usize,
    row_span: usize,
) -> usize {
    let mut column = 0usize;
    while grid_area_occupied(occupied, column, row, column_span, row_span) {
        column = column.saturating_add(1);
    }
    column
}

fn first_free_auto_cell(
    occupied: &[(usize, usize)],
    mut column: usize,
    mut row: usize,
    column_span: usize,
    row_span: usize,
    auto_columns: usize,
) -> (usize, usize) {
    let auto_columns = auto_columns.max(1);
    while grid_area_occupied(occupied, column, row, column_span, row_span) {
        column = column.saturating_add(1);
        if column >= auto_columns {
            column = 0;
            row = row.saturating_add(1);
        }
    }
    (column, row)
}

fn grid_area_occupied(
    occupied: &[(usize, usize)],
    column: usize,
    row: usize,
    column_span: usize,
    row_span: usize,
) -> bool {
    for area_column in column..column.saturating_add(column_span.max(1)) {
        for area_row in row..row.saturating_add(row_span.max(1)) {
            if occupied.contains(&(area_column, area_row)) {
                return true;
            }
        }
    }
    false
}

fn mark_grid_cells(occupied: &mut Vec<(usize, usize)>, placement: GridPlacement) {
    for column in placement.column
        ..placement
            .column
            .saturating_add(placement.column_span.max(1))
    {
        for row in placement.row..placement.row.saturating_add(placement.row_span.max(1)) {
            occupied.push((column, row));
        }
    }
}

fn grid_column_count(node: &StyledNode<'_>, placements: &[GridPlacement]) -> usize {
    placements
        .iter()
        .map(|placement| placement.column.saturating_add(placement.column_span))
        .chain(std::iter::once(
            node.style.grid_template_columns.len().max(1),
        ))
        .max()
        .unwrap_or(1)
}

fn grid_row_count(node: &StyledNode<'_>, placements: &[GridPlacement]) -> usize {
    placements
        .iter()
        .map(|placement| placement.row.saturating_add(placement.row_span))
        .chain(std::iter::once(node.style.grid_template_rows.len().max(1)))
        .max()
        .unwrap_or(1)
}

fn resolve_grid_tracks(
    template: &[StyleGridTrack],
    count: usize,
    available: f32,
    gap: f32,
    auto_min_content: bool,
) -> Vec<f32> {
    let count = count.max(1);
    let total_gap = gap * count.saturating_sub(1) as f32;
    let usable = (available - total_gap).max(0.0);
    let mut sizes = vec![0.0; count];
    let mut fixed = 0.0;
    let mut fr = 0.0;
    let mut auto_count = 0usize;

    for (index, size) in sizes.iter_mut().enumerate() {
        match template.get(index).copied().unwrap_or(StyleGridTrack::Auto) {
            StyleGridTrack::Px(px) => {
                *size = px.max(0.0);
                fixed += *size;
            }
            StyleGridTrack::Percent(percent) => {
                *size = (usable * percent).max(0.0);
                fixed += *size;
            }
            StyleGridTrack::Fr(value) => fr += value.max(0.0),
            StyleGridTrack::Auto => auto_count = auto_count.saturating_add(1),
        }
    }

    let remaining = (usable - fixed).max(0.0);
    let auto_width = if fr == 0.0 && auto_count > 0 && !auto_min_content {
        remaining / auto_count as f32
    } else {
        0.0
    };
    for (index, size) in sizes.iter_mut().enumerate() {
        match template.get(index).copied().unwrap_or(StyleGridTrack::Auto) {
            StyleGridTrack::Fr(value) if fr > 0.0 => {
                *size = remaining * (value.max(0.0) / fr);
            }
            StyleGridTrack::Auto => {
                *size = auto_width;
            }
            StyleGridTrack::Px(_) | StyleGridTrack::Percent(_) | StyleGridTrack::Fr(_) => {}
        }
    }

    sizes
}

fn grid_track_is_auto(template: &[StyleGridTrack], index: usize) -> bool {
    matches!(
        template.get(index).copied().unwrap_or(StyleGridTrack::Auto),
        StyleGridTrack::Auto | StyleGridTrack::Fr(_)
    )
}

fn grid_track_offset(origin: f32, tracks: &[f32], gap: f32, index: usize) -> f32 {
    origin + tracks.iter().take(index).sum::<f32>() + gap * index as f32
}

fn grid_span_size(tracks: &[f32], start: usize, span: usize, gap: f32) -> f32 {
    let span = span.max(1);
    tracks.iter().skip(start).take(span).sum::<f32>() + gap * span.saturating_sub(1) as f32
}

fn layout_flex_children(
    node: &StyledNode<'_>,
    content_rect: Rect,
    contents: &mut Vec<LayoutItem>,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<f32> {
    let children = node
        .children
        .iter()
        .filter(|child| child.style.display != Display::None)
        .collect::<Vec<_>>();
    if children.is_empty() {
        return Ok(content_rect.y);
    }

    match node.style.flex_direction {
        StyleFlexDirection::Column => layout_flex_column(
            &children,
            node,
            content_rect,
            contents,
            images,
            form_context,
            state,
        ),
        StyleFlexDirection::Row => layout_flex_row(
            &children,
            node,
            content_rect,
            contents,
            images,
            form_context,
            state,
        ),
    }
}

fn layout_flex_column(
    children: &[&StyledNode<'_>],
    node: &StyledNode<'_>,
    content_rect: Rect,
    contents: &mut Vec<LayoutItem>,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<f32> {
    let mut cursor_y = content_rect.y;
    for (index, child) in children.iter().enumerate() {
        if index > 0 {
            cursor_y += node.style.gap;
        }
        let mut child_box = layout_block_child(
            child,
            Rect {
                x: content_rect.x,
                y: content_rect.y,
                width: content_rect.width,
                height: 0.0,
            },
            cursor_y,
            images,
            form_context.clone(),
            state,
        )?;
        if child_box.positioning.mode != PositionMode::Absolute {
            let margin_box = child_box.dimensions.margin_box();
            cursor_y = margin_box.y + margin_box.height;
            align_flex_item_cross_axis(
                &mut child_box,
                node,
                child,
                content_rect,
                state,
                (0, 0),
                true,
            );
        }
        contents.push(LayoutItem::Box(child_box));
    }
    Ok(cursor_y)
}

fn layout_flex_row(
    children: &[&StyledNode<'_>],
    node: &StyledNode<'_>,
    content_rect: Rect,
    contents: &mut Vec<LayoutItem>,
    images: &ImageMap,
    form_context: Option<FormMetadata>,
    state: &mut LayoutState,
) -> WebbyResult<f32> {
    let widths = flex_row_widths(children, node, content_rect.width);
    let total_gap = node.style.gap * children.len().saturating_sub(1) as f32;
    let total_width = widths.iter().sum::<f32>() + total_gap;
    let extra_space = (content_rect.width - total_width).max(0.0);
    let between_gap =
        if node.style.justify_content == StyleJustifyContent::SpaceBetween && children.len() > 1 {
            node.style.gap + extra_space / (children.len() - 1) as f32
        } else {
            node.style.gap
        };
    let mut cursor_x = content_rect.x
        + match node.style.justify_content {
            StyleJustifyContent::Center => extra_space / 2.0,
            StyleJustifyContent::FlexStart | StyleJustifyContent::SpaceBetween => 0.0,
        };
    let mut row_bottom = content_rect.y;
    let mut entries = Vec::new();

    for (index, child) in children.iter().enumerate() {
        if index > 0 {
            cursor_x += between_gap;
        }
        let link_start = state.links.len();
        let control_start = state.form_controls.len();
        let child_box = layout_block_child(
            child,
            Rect {
                x: cursor_x,
                y: content_rect.y,
                width: widths.get(index).copied().unwrap_or(0.0),
                height: 0.0,
            },
            content_rect.y,
            images,
            form_context.clone(),
            state,
        )?;
        let margin_box = child_box.dimensions.margin_box();
        if child_box.positioning.mode != PositionMode::Absolute {
            row_bottom = row_bottom.max(margin_box.y + margin_box.height);
        }
        entries.push((*child, child_box, link_start, control_start));
        cursor_x += widths.get(index).copied().unwrap_or(0.0);
    }

    let row_height = (row_bottom - content_rect.y).max(0.0);
    for (child, mut child_box, link_start, control_start) in entries {
        align_flex_item_cross_axis(
            &mut child_box,
            node,
            child,
            Rect {
                height: row_height,
                ..content_rect
            },
            state,
            (link_start, control_start),
            false,
        );
        contents.push(LayoutItem::Box(child_box));
    }
    Ok(content_rect.y + row_height)
}

fn flex_row_widths(
    children: &[&StyledNode<'_>],
    node: &StyledNode<'_>,
    available: f32,
) -> Vec<f32> {
    let total_gap = node.style.gap * children.len().saturating_sub(1) as f32;
    let mut fixed = 0.0;
    let mut grow = 0.0;
    let mut auto_count = 0usize;
    for child in children {
        if let Some(width) = child.style.width {
            fixed += resolve_css_size(width, available);
        } else if child.style.flex_grow > 0.0 {
            grow += child.style.flex_grow;
        } else {
            auto_count += 1;
        }
    }
    let remaining = (available - total_gap - fixed).max(0.0);
    let auto_width = if grow == 0.0 && auto_count > 0 {
        remaining / auto_count as f32
    } else {
        0.0
    };
    children
        .iter()
        .map(|child| {
            child
                .style
                .width
                .map(|width| resolve_css_size(width, available))
                .unwrap_or_else(|| {
                    if grow > 0.0 && child.style.flex_grow > 0.0 {
                        remaining * (child.style.flex_grow / grow)
                    } else {
                        auto_width
                    }
                })
        })
        .collect()
}

fn align_flex_item_cross_axis(
    layout_box: &mut LayoutBox,
    node: &StyledNode<'_>,
    child: &StyledNode<'_>,
    content_rect: Rect,
    state: &mut LayoutState,
    hit_starts: (usize, usize),
    column: bool,
) {
    stretch_flex_item_cross_axis(layout_box, node, child, content_rect, column);
    let margin_box = layout_box.dimensions.margin_box();
    let delta = if column {
        match node.style.align_items {
            StyleAlignItems::Center => {
                content_rect.x + (content_rect.width - margin_box.width).max(0.0) / 2.0
                    - margin_box.x
            }
            StyleAlignItems::FlexStart | StyleAlignItems::Stretch => content_rect.x - margin_box.x,
        }
    } else {
        match node.style.align_items {
            StyleAlignItems::Center => {
                content_rect.y + (content_rect.height - margin_box.height).max(0.0) / 2.0
                    - margin_box.y
            }
            StyleAlignItems::FlexStart | StyleAlignItems::Stretch => 0.0,
        }
    };
    if column {
        shift_box(layout_box, delta, 0.0);
        shift_hit_regions(state, hit_starts.0, hit_starts.1, delta, 0.0);
    } else {
        shift_box(layout_box, 0.0, delta);
        shift_hit_regions(state, hit_starts.0, hit_starts.1, 0.0, delta);
    }
}

fn stretch_flex_item_cross_axis(
    layout_box: &mut LayoutBox,
    node: &StyledNode<'_>,
    child: &StyledNode<'_>,
    content_rect: Rect,
    column: bool,
) {
    if node.style.align_items != StyleAlignItems::Stretch {
        return;
    }

    if column {
        if child.style.width.is_some() {
            return;
        }
        let occupied = layout_box.dimensions.margin.horizontal()
            + layout_box.dimensions.border.horizontal()
            + layout_box.dimensions.padding.horizontal();
        let target = (content_rect.width - occupied).max(0.0);
        layout_box.dimensions.content.width = layout_box.dimensions.content.width.max(target);
    } else {
        if child.style.height.is_some() {
            return;
        }
        let occupied = layout_box.dimensions.margin.vertical()
            + layout_box.dimensions.border.vertical()
            + layout_box.dimensions.padding.vertical();
        let target = (content_rect.height - occupied).max(0.0);
        layout_box.dimensions.content.height = layout_box.dimensions.content.height.max(target);
    }
}

fn apply_relative_position(
    layout_box: &mut LayoutBox,
    node: &StyledNode<'_>,
    mut positioning: Positioning,
    containing_width: f32,
    state: &mut LayoutState,
    link_start: usize,
    control_start: usize,
) {
    layout_box.positioning = positioning.clone();
    if positioning.mode != PositionMode::Relative {
        return;
    }

    let (dx, dy) = relative_offset(node, containing_width);
    positioning.offset_x = dx;
    positioning.offset_y = dy;
    layout_box.positioning = positioning;
    shift_box(layout_box, dx, dy);
    shift_hit_regions(state, link_start, control_start, dx, dy);
}

fn apply_absolute_position(
    layout_box: &mut LayoutBox,
    node: &StyledNode<'_>,
    mut positioning: Positioning,
    containing_block: Rect,
    state: &mut LayoutState,
    link_start: usize,
    control_start: usize,
) {
    layout_box.positioning = positioning.clone();
    let (dx, dy) = absolute_offset(layout_box, node, containing_block);
    positioning.offset_x = dx;
    positioning.offset_y = dy;
    layout_box.positioning = positioning;
    shift_box(layout_box, dx, dy);
    shift_hit_regions(state, link_start, control_start, dx, dy);
}

fn relative_offset(node: &StyledNode<'_>, containing_width: f32) -> (f32, f32) {
    let dx = offset_value(node.style.left, containing_width)
        .unwrap_or_else(|| -offset_value(node.style.right, containing_width).unwrap_or(0.0));
    let dy = offset_value(node.style.top, 0.0)
        .unwrap_or_else(|| -offset_value(node.style.bottom, 0.0).unwrap_or(0.0));
    (dx, dy)
}

fn absolute_offset(
    layout_box: &LayoutBox,
    node: &StyledNode<'_>,
    containing_block: Rect,
) -> (f32, f32) {
    let margin_box = layout_box.dimensions.margin_box();
    let target_x = offset_value(node.style.left, containing_block.width)
        .map(|left| containing_block.x + left)
        .or_else(|| {
            offset_value(node.style.right, containing_block.width)
                .map(|right| containing_block.x + containing_block.width - right - margin_box.width)
        })
        .unwrap_or(margin_box.x);
    let target_y = offset_value(node.style.top, containing_block.height)
        .map(|top| containing_block.y + top)
        .or_else(|| {
            offset_value(node.style.bottom, containing_block.height).map(|bottom| {
                containing_block.y + containing_block.height - bottom - margin_box.height
            })
        })
        .unwrap_or(margin_box.y);
    (target_x - margin_box.x, target_y - margin_box.y)
}

fn resolve_content_width(
    node: &StyledNode<'_>,
    containing_width: f32,
    available_content_width: f32,
    border: EdgeSizes,
    padding: EdgeSizes,
) -> f32 {
    let raw = match node.style.width {
        Some(size) => resolve_css_size(size, containing_width),
        None => available_content_width,
    };
    let content = match (node.style.width, node.style.box_sizing) {
        (Some(_), StyleBoxSizing::BorderBox) => {
            (raw - border.horizontal() - padding.horizontal()).max(0.0)
        }
        _ => raw,
    };
    clamp_size(
        content,
        node.style.min_width,
        node.style.max_width,
        containing_width,
    )
    .min(available_content_width)
    .max(0.0)
}

fn resolve_content_height(
    node: &StyledNode<'_>,
    containing_height: f32,
    auto_height: f32,
    border: EdgeSizes,
    padding: EdgeSizes,
) -> f32 {
    let raw = match node.style.height {
        Some(size) => resolve_css_size(size, containing_height),
        None => auto_height,
    };
    let content = match (node.style.height, node.style.box_sizing) {
        (Some(_), StyleBoxSizing::BorderBox) => {
            (raw - border.vertical() - padding.vertical()).max(0.0)
        }
        _ => raw,
    };
    clamp_size(
        content.max(0.0),
        node.style.min_height,
        node.style.max_height,
        containing_height,
    )
}

fn resolve_css_size(size: CssSize, containing_size: f32) -> f32 {
    match size {
        CssSize::Auto => containing_size.max(0.0),
        CssSize::Px(px) => px.max(0.0),
        CssSize::Percent(percent) => (containing_size * percent).max(0.0),
    }
}

fn clamp_size(
    value: f32,
    min_size: Option<CssSize>,
    max_size: Option<CssSize>,
    containing_size: f32,
) -> f32 {
    let mut clamped = value;
    if let Some(min_size) = min_size {
        clamped = clamped.max(resolve_css_size(min_size, containing_size));
    }
    if let Some(max_size) = max_size {
        clamped = clamped.min(resolve_css_size(max_size, containing_size));
    }
    clamped.max(0.0)
}

fn offset_value(value: Option<CssSize>, containing_size: f32) -> Option<f32> {
    value.map(|size| resolve_css_size(size, containing_size))
}

fn flush_inline_nodes(
    inline_nodes: &[&StyledNode<'_>],
    line_area: Rect,
    contents: &mut Vec<LayoutItem>,
    flush_context: InlineFlushContext<'_>,
) -> WebbyResult<f32> {
    if inline_nodes.is_empty() {
        return Ok(line_area.y);
    }

    let mut items = Vec::new();
    let mut collection = InlineCollectionState::default();
    for node in inline_nodes {
        collect_inline_items(
            node,
            flush_context.inherited_link,
            flush_context.form_context.as_ref(),
            flush_context.images,
            &mut collection,
            &mut items,
        );
    }

    if items.is_empty() {
        return Ok(line_area.y);
    }

    let mut context = InlineLayoutContext {
        start_x: line_area.x,
        cursor_x: line_area.x,
        cursor_y: line_area.y,
        available_width: line_area.width,
        line_ascent: 0.0,
        line_descent: 0.0,
        line_fragments: Vec::new(),
        text_align: flush_context.text_align,
        contents,
        state: flush_context.state,
    };

    for item in items {
        match item {
            InlineItem::Word(word) => context.place_word(word),
            InlineItem::Break(break_item) => context.force_break(break_item),
            InlineItem::Image(image) => context.place_image(image),
            InlineItem::Control(control) => context.place_control(control),
        }
    }

    Ok(context.finish())
}

struct InlineFlushContext<'a> {
    images: &'a ImageMap,
    form_context: Option<FormMetadata>,
    inherited_link: Option<LinkRef<'a>>,
    text_align: StyleTextAlign,
    state: &'a mut LayoutState,
}

#[derive(Debug, Clone, Copy)]
struct LinkRef<'a> {
    href: &'a str,
    node_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkTarget {
    href: String,
    node_id: u64,
}

fn collect_inline_items(
    node: &StyledNode<'_>,
    inherited_link: Option<LinkRef<'_>>,
    form_context: Option<&FormMetadata>,
    images: &ImageMap,
    state: &mut InlineCollectionState,
    items: &mut Vec<InlineItem>,
) {
    if node.style.display != Display::None
        && let Some(control) = form_control_item(node, form_context)
    {
        state.pending_space = false;
        items.push(InlineItem::Control(control));
        return;
    }

    match node.style.display {
        Display::None | Display::Block | Display::Flex | Display::Grid => {}
        Display::LineBreak => {
            state.pending_space = false;
            items.push(InlineItem::Break(BreakItem {
                line_height: line_height_for(node),
            }));
        }
        Display::Inline | Display::InlineBlock => {
            if matches!(
                node.tag_name(),
                Some("img" | "svg" | "canvas" | "audio" | "video" | "iframe")
            ) {
                let tag_name = node.tag_name().unwrap_or("img").to_string();
                let src = media_image_source(node).map(str::to_string);
                let alt = image_attr(node, "alt").map(str::to_string);
                let visible = node.style.visibility == StyleVisibility::Visible;
                let image = if visible
                    && (tag_name == "img" || tag_name == "video" || tag_name == "iframe")
                {
                    src.as_deref().and_then(|value| images.get(value)).cloned()
                } else {
                    None
                };
                let width = replaced_width(node, image.as_ref());
                let height = replaced_height(node, image.as_ref());
                let graphics = if visible {
                    graphic_commands_for_node(node, width, height)
                } else {
                    Vec::new()
                };
                state.pending_space = false;
                items.push(InlineItem::Image(ImageItem {
                    tag_name,
                    width,
                    height,
                    metadata: element_metadata(node),
                    padding: EdgeSizes::from(node.style.padding),
                    border: EdgeSizes {
                        top: node.style.border.width,
                        right: node.style.border.width,
                        bottom: node.style.border.width,
                        left: node.style.border.width,
                    },
                    margin: EdgeSizes::from(node.style.margin),
                    visuals: box_visuals(node),
                    link: visible
                        .then_some(inherited_link)
                        .flatten()
                        .map(|link| LinkTarget {
                            href: link.href.to_string(),
                            node_id: link.node_id,
                        }),
                    visible,
                    src,
                    alt,
                    image,
                    graphics,
                }));
                return;
            }

            let current_link = link_ref(node).or(inherited_link);
            if let Some(text) = node.text() {
                collect_words(
                    text,
                    node.style.font_size,
                    line_height_for(node),
                    text_visuals(node, current_link),
                    node.style.white_space,
                    state,
                    items,
                );
            }

            for child in &node.children {
                collect_inline_items(child, current_link, form_context, images, state, items);
            }
        }
    }
}

fn collect_words(
    text: &str,
    font_size: f32,
    line_height: f32,
    visuals: TextVisuals,
    white_space: StyleWhiteSpace,
    state: &mut InlineCollectionState,
    items: &mut Vec<InlineItem>,
) {
    if white_space == StyleWhiteSpace::Pre {
        collect_pre_words(text, font_size, line_height, visuals, items);
        return;
    }

    if text.chars().next().is_some_and(char::is_whitespace) {
        state.pending_space = true;
    }

    for segment in text.split_whitespace() {
        if state.pending_space && !items.is_empty() {
            items.push(InlineItem::Word(WordItem {
                text: " ".to_string(),
                font_size,
                line_height,
                visuals: visuals.clone(),
                is_space: true,
                allow_wrap: white_space != StyleWhiteSpace::NoWrap,
            }));
        }
        items.push(InlineItem::Word(WordItem {
            text: segment.to_string(),
            font_size,
            line_height,
            visuals: visuals.clone(),
            is_space: false,
            allow_wrap: white_space != StyleWhiteSpace::NoWrap,
        }));
        state.pending_space = true;
    }

    if text.chars().last().is_some_and(char::is_whitespace) {
        state.pending_space = true;
    }
}

fn collect_pre_words(
    text: &str,
    font_size: f32,
    line_height: f32,
    visuals: TextVisuals,
    items: &mut Vec<InlineItem>,
) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            items.push(InlineItem::Break(BreakItem { line_height }));
        }
        if !line.is_empty() {
            items.push(InlineItem::Word(WordItem {
                text: line.to_string(),
                font_size,
                line_height,
                visuals: visuals.clone(),
                is_space: false,
                allow_wrap: false,
            }));
        }
    }
}

fn link_href<'a>(node: &'a StyledNode<'_>) -> Option<&'a str> {
    if node.tag_name() != Some("a") {
        return None;
    }

    node.attributes()
        .and_then(|attributes| attributes.get("href"))
        .map(String::as_str)
}

fn link_ref<'a>(node: &'a StyledNode<'_>) -> Option<LinkRef<'a>> {
    link_href(node).map(|href| LinkRef {
        href,
        node_id: node.node.id,
    })
}

fn replaced_width(node: &StyledNode<'_>, image: Option<&ImageResource>) -> f32 {
    node.style
        .width
        .and_then(replaced_element_size)
        .or_else(|| image_dimension_attr(node, "width"))
        .or_else(|| image.map(|resource| resource.image.width as f32))
        .unwrap_or(DEFAULT_IMAGE_WIDTH)
}

fn replaced_height(node: &StyledNode<'_>, image: Option<&ImageResource>) -> f32 {
    node.style
        .height
        .and_then(replaced_element_size)
        .or_else(|| image_dimension_attr(node, "height"))
        .or_else(|| image.map(|resource| resource.image.height as f32))
        .unwrap_or(DEFAULT_IMAGE_HEIGHT)
}

fn replaced_element_size(size: CssSize) -> Option<f32> {
    match size {
        CssSize::Px(px) => Some(px.max(0.0)),
        CssSize::Auto | CssSize::Percent(_) => None,
    }
}

fn image_dimension_attr(node: &StyledNode<'_>, name: &str) -> Option<f32> {
    node.attributes()
        .and_then(|attributes| attributes.get(name))
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn image_attr<'a>(node: &'a StyledNode<'_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .and_then(|attributes| attributes.get(name))
        .map(String::as_str)
}

fn media_image_source<'a>(node: &'a StyledNode<'_>) -> Option<&'a str> {
    match node.tag_name() {
        Some("video") => image_attr(node, "poster"),
        Some("img" | "iframe") => image_attr(node, "src"),
        _ => None,
    }
}

fn graphic_commands_for_node(
    node: &StyledNode<'_>,
    viewport_width: f32,
    viewport_height: f32,
) -> Vec<GraphicCommand> {
    match node.tag_name() {
        Some("svg") => svg_graphic_commands(node, viewport_width, viewport_height),
        Some("canvas") => canvas_graphic_commands(node),
        _ => Vec::new(),
    }
}

fn svg_graphic_commands(
    node: &StyledNode<'_>,
    viewport_width: f32,
    viewport_height: f32,
) -> Vec<GraphicCommand> {
    let mut commands = Vec::new();
    let transform = SvgTransform::for_node(node, viewport_width, viewport_height);
    collect_svg_commands(node, transform, &mut commands);
    commands
}

fn collect_svg_commands(
    node: &StyledNode<'_>,
    transform: SvgTransform,
    commands: &mut Vec<GraphicCommand>,
) {
    for child in &node.children {
        match child.tag_name() {
            Some("rect") => {
                let rect = transform.rect(Rect {
                    x: numeric_attr(child, "x").unwrap_or(0.0),
                    y: numeric_attr(child, "y").unwrap_or(0.0),
                    width: numeric_attr(child, "width").unwrap_or(0.0).max(0.0),
                    height: numeric_attr(child, "height").unwrap_or(0.0).max(0.0),
                });
                if let Some(color) = paint_attr(child, "fill") {
                    commands.push(GraphicCommand::FillRect { rect, color });
                }
                if let Some(color) = paint_attr(child, "stroke") {
                    commands.push(GraphicCommand::StrokeRect {
                        rect,
                        color,
                        width: transform.stroke_width(
                            numeric_attr(child, "stroke-width").unwrap_or(1.0).max(0.0),
                        ),
                    });
                }
            }
            Some("line") => {
                if let Some(color) = paint_attr(child, "stroke") {
                    let from = transform.point(
                        numeric_attr(child, "x1").unwrap_or(0.0),
                        numeric_attr(child, "y1").unwrap_or(0.0),
                    );
                    let to = transform.point(
                        numeric_attr(child, "x2").unwrap_or(0.0),
                        numeric_attr(child, "y2").unwrap_or(0.0),
                    );
                    commands.push(GraphicCommand::Line {
                        from_x: from.0,
                        from_y: from.1,
                        to_x: to.0,
                        to_y: to.1,
                        color,
                        width: transform.stroke_width(
                            numeric_attr(child, "stroke-width").unwrap_or(1.0).max(0.0),
                        ),
                    });
                }
            }
            Some("circle") => {
                let center = transform.point(
                    numeric_attr(child, "cx").unwrap_or(0.0),
                    numeric_attr(child, "cy").unwrap_or(0.0),
                );
                commands.push(GraphicCommand::Circle {
                    cx: center.0,
                    cy: center.1,
                    radius: transform.radius(numeric_attr(child, "r").unwrap_or(0.0).max(0.0)),
                    fill: paint_attr(child, "fill"),
                    stroke: paint_attr(child, "stroke"),
                    stroke_width: transform
                        .stroke_width(numeric_attr(child, "stroke-width").unwrap_or(1.0).max(0.0)),
                });
            }
            _ => collect_svg_commands(child, transform, commands),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SvgTransform {
    min_x: f32,
    min_y: f32,
    scale_x: f32,
    scale_y: f32,
}

impl SvgTransform {
    fn for_node(node: &StyledNode<'_>, viewport_width: f32, viewport_height: f32) -> Self {
        let Some(view_box) = image_attr(node, "viewBox")
            .or_else(|| image_attr(node, "viewbox"))
            .and_then(parse_view_box)
        else {
            return Self::identity();
        };
        if viewport_width <= 0.0 || viewport_height <= 0.0 {
            return Self::identity();
        }
        Self {
            min_x: view_box.x,
            min_y: view_box.y,
            scale_x: viewport_width / view_box.width,
            scale_y: viewport_height / view_box.height,
        }
    }

    fn identity() -> Self {
        Self {
            min_x: 0.0,
            min_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
        }
    }

    fn point(self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.min_x) * self.scale_x,
            (y - self.min_y) * self.scale_y,
        )
    }

    fn rect(self, rect: Rect) -> Rect {
        let point = self.point(rect.x, rect.y);
        Rect {
            x: point.0,
            y: point.1,
            width: rect.width * self.scale_x,
            height: rect.height * self.scale_y,
        }
    }

    fn radius(self, radius: f32) -> f32 {
        radius * self.average_scale()
    }

    fn stroke_width(self, width: f32) -> f32 {
        width * self.average_scale()
    }

    fn average_scale(self) -> f32 {
        (self.scale_x.abs() + self.scale_y.abs()) / 2.0
    }
}

fn parse_view_box(value: &str) -> Option<Rect> {
    let numbers = value
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .map(parse_svg_number)
        .collect::<Option<Vec<_>>>()?;
    let [x, y, width, height] = numbers.as_slice() else {
        return None;
    };
    if *width <= 0.0 || *height <= 0.0 {
        return None;
    }
    Some(Rect {
        x: *x,
        y: *y,
        width: *width,
        height: *height,
    })
}

fn canvas_graphic_commands(node: &StyledNode<'_>) -> Vec<GraphicCommand> {
    let Some(commands) = image_attr(node, "data-webby-canvas") else {
        return Vec::new();
    };
    commands
        .split(';')
        .filter_map(canvas_graphic_command)
        .collect()
}

fn canvas_graphic_command(raw: &str) -> Option<GraphicCommand> {
    let parts = raw.split(',').collect::<Vec<_>>();
    match parts.first().copied() {
        Some("fillRect") if parts.len() == 6 => Some(GraphicCommand::FillRect {
            rect: Rect {
                x: parts.get(1)?.parse::<f32>().ok()?,
                y: parts.get(2)?.parse::<f32>().ok()?,
                width: parts.get(3)?.parse::<f32>().ok()?.max(0.0),
                height: parts.get(4)?.parse::<f32>().ok()?.max(0.0),
            },
            color: parse_graphic_color(parts.get(5)?).unwrap_or(VisualColor {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        }),
        Some("strokeRect") if parts.len() == 7 => Some(GraphicCommand::StrokeRect {
            rect: Rect {
                x: parts.get(1)?.parse::<f32>().ok()?,
                y: parts.get(2)?.parse::<f32>().ok()?,
                width: parts.get(3)?.parse::<f32>().ok()?.max(0.0),
                height: parts.get(4)?.parse::<f32>().ok()?.max(0.0),
            },
            color: parse_graphic_color(parts.get(5)?).unwrap_or(VisualColor {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
            width: parts.get(6)?.parse::<f32>().ok()?.max(0.0),
        }),
        Some("clearRect") if parts.len() == 5 => Some(GraphicCommand::FillRect {
            rect: Rect {
                x: parts.get(1)?.parse::<f32>().ok()?,
                y: parts.get(2)?.parse::<f32>().ok()?,
                width: parts.get(3)?.parse::<f32>().ok()?.max(0.0),
                height: parts.get(4)?.parse::<f32>().ok()?.max(0.0),
            },
            color: VisualColor {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }),
        _ => None,
    }
}

fn numeric_attr(node: &StyledNode<'_>, name: &str) -> Option<f32> {
    image_attr(node, name).and_then(parse_svg_number)
}

fn parse_svg_number(value: &str) -> Option<f32> {
    let trimmed = value.trim().trim_end_matches("px");
    trimmed
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite())
}

fn paint_attr(node: &StyledNode<'_>, name: &str) -> Option<VisualColor> {
    image_attr(node, name).and_then(parse_graphic_color)
}

fn parse_graphic_color(value: &str) -> Option<VisualColor> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => None,
        "black" => Some(VisualColor {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }),
        "white" => Some(VisualColor {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
        "red" => Some(VisualColor {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }),
        "green" => Some(VisualColor {
            r: 0,
            g: 128,
            b: 0,
            a: 255,
        }),
        "blue" => Some(VisualColor {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        }),
        other => parse_hex_color(other),
    }
}

fn parse_hex_color(value: &str) -> Option<VisualColor> {
    let hex = value.strip_prefix('#')?;
    let (r, g, b) = match hex.len() {
        3 => {
            let mut chars = hex.chars();
            let r = hex_nibble_pair(chars.next()?)?;
            let g = hex_nibble_pair(chars.next()?)?;
            let b = hex_nibble_pair(chars.next()?)?;
            (r, g, b)
        }
        6 => {
            let bytes = hex.as_bytes();
            let r = hex_byte(bytes.first().copied()?, bytes.get(1).copied()?)?;
            let g = hex_byte(bytes.get(2).copied()?, bytes.get(3).copied()?)?;
            let b = hex_byte(bytes.get(4).copied()?, bytes.get(5).copied()?)?;
            (r, g, b)
        }
        _ => return None,
    };
    Some(VisualColor { r, g, b, a: 255 })
}

fn hex_nibble_pair(character: char) -> Option<u8> {
    let value = character.to_digit(16)? as u8;
    Some(value * 16 + value)
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    Some(value(high)? * 16 + value(low)?)
}

fn form_context_for_node(
    node: &StyledNode<'_>,
    inherited: Option<FormMetadata>,
    state: &mut LayoutState,
) -> Option<FormMetadata> {
    if node.tag_name() != Some("form") {
        return inherited;
    }

    let id = state.next_form_id;
    state.next_form_id = state.next_form_id.saturating_add(1);
    Some(FormMetadata {
        id,
        node_id: node.node.id,
        action: image_attr(node, "action").map(str::to_string),
        method: image_attr(node, "method").map(|value| value.to_ascii_lowercase()),
    })
}

fn form_control_item(
    node: &StyledNode<'_>,
    form_context: Option<&FormMetadata>,
) -> Option<ControlItem> {
    let tag = node.tag_name()?;
    let control_type = match tag {
        "input" => input_control_type(image_attr(node, "type")),
        "button" => button_control_type(image_attr(node, "type")),
        "select" => FormControlType::Select,
        "textarea" => FormControlType::Textarea,
        _ => return None,
    };
    let options = select_options(node);
    let value = control_value(node, control_type);
    let placeholder = image_attr(node, "placeholder").map(str::to_string);
    let checked = image_attr(node, "checked").is_some();
    let disabled = image_attr(node, "disabled").is_some();
    let label = control_label(
        node,
        control_type,
        &value,
        placeholder.as_deref(),
        &options,
        checked,
    );
    let text_width = measure_text(
        &label,
        node.style.font_size,
        FontWeight::from(node.style.font_weight),
        FontFamily::from(node.style.font_family),
    );
    let width = node
        .style
        .width
        .and_then(replaced_element_size)
        .unwrap_or_else(|| (text_width + 16.0).max(default_control_width(control_type)));
    let height = node
        .style
        .height
        .and_then(replaced_element_size)
        .unwrap_or_else(|| line_height(node.style.font_size));

    Some(ControlItem {
        tag_name: tag.to_string(),
        metadata: element_metadata(node),
        control_type,
        form: form_context.cloned(),
        name: image_attr(node, "name").map(str::to_string),
        value,
        placeholder,
        disabled,
        checked,
        options,
        label,
        width,
        height,
        padding: EdgeSizes::from(node.style.padding),
        border: EdgeSizes {
            top: node.style.border.width,
            right: node.style.border.width,
            bottom: node.style.border.width,
            left: node.style.border.width,
        },
        margin: EdgeSizes::from(node.style.margin),
        visuals: box_visuals(node),
        text_visuals: text_visuals(node, None),
        interactive: node.style.visibility == StyleVisibility::Visible && !disabled,
    })
}

fn input_control_type(raw_type: Option<&str>) -> FormControlType {
    match raw_type.unwrap_or("text").to_ascii_lowercase().as_str() {
        "search" => FormControlType::Search,
        "password" => FormControlType::Password,
        "email" => FormControlType::Email,
        "checkbox" => FormControlType::Checkbox,
        "radio" => FormControlType::Radio,
        "submit" => FormControlType::Submit,
        _ => FormControlType::Text,
    }
}

fn button_control_type(raw_type: Option<&str>) -> FormControlType {
    match raw_type.unwrap_or("submit").to_ascii_lowercase().as_str() {
        "button" => FormControlType::Button,
        "reset" => FormControlType::Reset,
        _ => FormControlType::Submit,
    }
}

fn control_value(node: &StyledNode<'_>, control_type: FormControlType) -> String {
    if let Some(value) = image_attr(node, "value") {
        return value.to_string();
    }

    if node.tag_name() == Some("button") {
        return node
            .children
            .iter()
            .filter_map(|child| child.text())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
    }

    match control_type {
        FormControlType::Checkbox | FormControlType::Radio => "on".to_string(),
        FormControlType::Select => select_options(node)
            .into_iter()
            .find(|option| option.selected && !option.disabled)
            .or_else(|| {
                select_options(node)
                    .into_iter()
                    .find(|option| !option.disabled)
            })
            .map(|option| option.value)
            .unwrap_or_default(),
        FormControlType::Textarea => node
            .children
            .iter()
            .filter_map(|child| child.text())
            .collect::<Vec<_>>()
            .join(""),
        FormControlType::Submit => "Submit".to_string(),
        FormControlType::Reset => "Reset".to_string(),
        FormControlType::Button => "Button".to_string(),
        FormControlType::Text
        | FormControlType::Search
        | FormControlType::Password
        | FormControlType::Email => String::new(),
    }
}

fn control_label(
    node: &StyledNode<'_>,
    control_type: FormControlType,
    value: &str,
    placeholder: Option<&str>,
    options: &[FormOption],
    checked: bool,
) -> String {
    if matches!(
        control_type,
        FormControlType::Text
            | FormControlType::Search
            | FormControlType::Password
            | FormControlType::Email
            | FormControlType::Textarea
    ) {
        return if value.is_empty() {
            placeholder.unwrap_or("").to_string()
        } else if control_type == FormControlType::Password {
            "*".repeat(value.chars().count())
        } else {
            value.to_string()
        };
    }
    if control_type == FormControlType::Checkbox {
        return if checked { "[x]" } else { "[ ]" }.to_string();
    }
    if control_type == FormControlType::Radio {
        return if checked { "(o)" } else { "( )" }.to_string();
    }
    if control_type == FormControlType::Select {
        return options
            .iter()
            .find(|option| option.value == value)
            .or_else(|| options.iter().find(|option| !option.disabled))
            .map(|option| option.label.clone())
            .unwrap_or_default();
    }

    if node.tag_name() == Some("button") && !value.is_empty() {
        return value.to_string();
    }
    match control_type {
        FormControlType::Reset if value.is_empty() => "Reset".to_string(),
        FormControlType::Submit if value.is_empty() => "Submit".to_string(),
        FormControlType::Button if value.is_empty() => "Button".to_string(),
        _ => value.to_string(),
    }
}

fn default_control_width(control_type: FormControlType) -> f32 {
    match control_type {
        FormControlType::Text
        | FormControlType::Search
        | FormControlType::Password
        | FormControlType::Email
        | FormControlType::Select
        | FormControlType::Textarea => 180.0,
        FormControlType::Checkbox | FormControlType::Radio => 22.0,
        FormControlType::Submit | FormControlType::Button | FormControlType::Reset => 74.0,
    }
}

fn select_options(node: &StyledNode<'_>) -> Vec<FormOption> {
    if node.tag_name() != Some("select") {
        return Vec::new();
    }
    node.children
        .iter()
        .filter(|child| child.tag_name() == Some("option"))
        .map(|option| {
            let label = option
                .children
                .iter()
                .filter_map(|child| child.text())
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string();
            FormOption {
                value: image_attr(option, "value")
                    .map(str::to_string)
                    .unwrap_or_else(|| label.clone()),
                label,
                selected: image_attr(option, "selected").is_some(),
                disabled: image_attr(option, "disabled").is_some(),
            }
        })
        .collect()
}

fn block_kind(node: &StyledNode<'_>) -> LayoutKind {
    LayoutKind::Block {
        tag_name: node
            .tag_name()
            .map_or_else(|| "#document".to_string(), str::to_string),
        metadata: Box::new(element_metadata(node)),
    }
}

fn element_metadata(node: &StyledNode<'_>) -> ElementMetadata {
    let id = image_attr(node, "id").map(str::to_string);
    let classes = image_attr(node, "class")
        .map(|value| {
            value
                .split_ascii_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ElementMetadata {
        node_id: Some(node.node.id),
        id,
        classes,
    }
}

fn box_visuals(node: &StyledNode<'_>) -> BoxVisuals {
    if node.style.visibility == StyleVisibility::Hidden {
        return BoxVisuals {
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
        };
    }
    BoxVisuals {
        background_color: VisualColor::from(node.style.background_color),
        border: BorderVisuals {
            color: VisualColor::from(node.style.border.color),
        },
    }
}

fn text_visuals(node: &StyledNode<'_>, link: Option<LinkRef<'_>>) -> TextVisuals {
    if node.style.visibility == StyleVisibility::Hidden {
        return TextVisuals {
            color: VisualColor {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            font_weight: FontWeight::from(node.style.font_weight),
            font_family: FontFamily::from(node.style.font_family),
            text_decoration: TextDecoration::None,
            is_link: false,
            href: None,
            link_node_id: None,
        };
    }
    TextVisuals {
        color: VisualColor::from(node.style.color),
        font_weight: FontWeight::from(node.style.font_weight),
        font_family: FontFamily::from(node.style.font_family),
        text_decoration: TextDecoration::from(node.style.text_decoration),
        is_link: node.style.is_link,
        href: link.map(|value| value.href.to_string()),
        link_node_id: link.map(|value| value.node_id),
    }
}

fn measure_text(
    text: &str,
    font_size: f32,
    font_weight: FontWeight,
    font_family: FontFamily,
) -> f32 {
    webby_text::measure_text_with_family(
        text,
        font_size,
        TextFontWeight::from(font_weight),
        TextFontFamily::from(font_family),
    )
    .width
}

fn line_height_for(node: &StyledNode<'_>) -> f32 {
    line_height_with_override(node.style.font_size, node.style.line_height)
}

fn line_height(font_size: f32) -> f32 {
    webby_text::line_height(font_size)
}

fn line_height_with_override(font_size: f32, override_height: Option<f32>) -> f32 {
    override_height.unwrap_or_else(|| webby_text::line_height(font_size))
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

fn format_edges(edges: EdgeSizes) -> String {
    format!(
        "{:.1}/{:.1}/{:.1}/{:.1}",
        edges.top, edges.right, edges.bottom, edges.left
    )
}

fn dump_box(layout_box: &LayoutBox, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    output.push_str(&indent);
    output.push_str(match &layout_box.kind {
        LayoutKind::Document => "#document",
        LayoutKind::Block { tag_name, .. } => tag_name,
        LayoutKind::Image { tag_name, .. } => tag_name,
        LayoutKind::FormControl { tag_name, .. } => tag_name,
    });
    if let Some(metadata) = element_metadata_for_kind(&layout_box.kind) {
        append_metadata(metadata, output);
    }
    output.push_str(" kind=");
    output.push_str(match &layout_box.kind {
        LayoutKind::Document => "document",
        LayoutKind::Block { .. } => "block",
        LayoutKind::Image {
            graphics, tag_name, ..
        } if !graphics.is_empty() => tag_name,
        LayoutKind::Image { image: Some(_), .. } => "image",
        LayoutKind::Image { image: None, .. } => "image-placeholder",
        LayoutKind::FormControl { .. } => "form-control",
    });
    if let Some(metadata) = element_metadata_for_kind(&layout_box.kind)
        && let Some(node_id) = metadata.node_id
    {
        output.push_str(" node=");
        output.push_str(&node_id.to_string());
    }
    output.push_str(" content=");
    output.push_str(&format_rect(layout_box.dimensions.content));
    output.push_str(" padding=");
    output.push_str(&format_edges(layout_box.dimensions.padding));
    output.push_str(" border=");
    output.push_str(&format_edges(layout_box.dimensions.border));
    output.push_str(" border-color=");
    output.push_str(&format_color(layout_box.visuals.border.color));
    output.push_str(" background=");
    output.push_str(&format_color(layout_box.visuals.background_color));
    output.push_str(" margin=");
    output.push_str(&format_edges(layout_box.dimensions.margin));
    output.push_str(" position=");
    output.push_str(layout_box.positioning.mode.as_str());
    output.push_str(" offset=");
    output.push_str(&format_px(layout_box.positioning.offset_x));
    output.push(',');
    output.push_str(&format_px(layout_box.positioning.offset_y));
    output.push_str(" flow=");
    output.push_str(layout_box.flow.as_str());
    if let Some(container) = &layout_box.scroll_container {
        output.push_str(" scroll-container=");
        output.push_str(&container.id.to_string());
        output.push_str(" viewport=");
        output.push_str(&format_rect(container.viewport));
        output.push_str(" max-scroll=");
        output.push_str(&format_px(container.max_scroll_x));
        output.push(',');
        output.push_str(&format_px(container.max_scroll_y));
    }
    if let LayoutKind::Image {
        src, alt, image, ..
    } = &layout_box.kind
    {
        if let Some(src) = src {
            output.push_str(" src=\"");
            output.push_str(&escape_dump_string(src));
            output.push('"');
        }
        if let Some(alt) = alt {
            output.push_str(" alt=\"");
            output.push_str(&escape_dump_string(alt));
            output.push('"');
        }
        if let Some(image) = image {
            output.push_str(" intrinsic=");
            output.push_str(&image.image.width.to_string());
            output.push('x');
            output.push_str(&image.image.height.to_string());
        }
    }
    if let LayoutKind::FormControl {
        control_type,
        form,
        name,
        value,
        placeholder,
        ..
    } = &layout_box.kind
    {
        output.push_str(" control-type=");
        output.push_str(control_type_name(*control_type));
        if let Some(form) = form {
            output.push_str(" form=");
            output.push_str(&form.id.to_string());
        }
        if let Some(name) = name {
            output.push_str(" name=\"");
            output.push_str(&escape_dump_string(name));
            output.push('"');
        }
        output.push_str(" value=\"");
        output.push_str(&escape_dump_string(value));
        output.push('"');
        if let Some(placeholder) = placeholder {
            output.push_str(" placeholder=\"");
            output.push_str(&escape_dump_string(placeholder));
            output.push('"');
        }
    }
    output.push('\n');

    for item in &layout_box.contents {
        match item {
            LayoutItem::LineBox(line) => dump_line_box(line, depth + 1, output),
            LayoutItem::Text(run) => dump_text_run(run, depth, output),
            LayoutItem::Box(child) => dump_box(child, depth + 1, output),
        }
    }
}

fn element_metadata_for_kind(kind: &LayoutKind) -> Option<&ElementMetadata> {
    match kind {
        LayoutKind::Document => None,
        LayoutKind::Block { metadata, .. }
        | LayoutKind::Image { metadata, .. }
        | LayoutKind::FormControl { metadata, .. } => Some(metadata.as_ref()),
    }
}

fn append_metadata(metadata: &ElementMetadata, output: &mut String) {
    if let Some(id) = &metadata.id {
        output.push_str(" id=\"");
        output.push_str(&escape_dump_string(id));
        output.push('"');
    }
    if !metadata.classes.is_empty() {
        output.push_str(" class=\"");
        output.push_str(&escape_dump_string(&metadata.classes.join(" ")));
        output.push('"');
    }
}

fn dump_line_box(line: &LineBox, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    output.push_str(&indent);
    output.push_str("line rect=");
    output.push_str(&format_rect(line.rect));
    output.push_str(" baseline=");
    output.push_str(&format_px(line.baseline));
    output.push('\n');

    for fragment in &line.fragments {
        match fragment {
            InlineFragment::Text(run) => dump_text_run(run, depth, output),
            InlineFragment::Box(layout_box) => dump_box(layout_box, depth + 1, output),
        }
    }
}

fn dump_text_run(run: &TextRun, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    output.push_str(&indent);
    output.push_str("  text \"");
    output.push_str(&escape_dump_string(&run.text));
    output.push_str("\" rect=");
    output.push_str(&format_rect(run.rect));
    output.push_str(" font-size=");
    output.push_str(&format_px(run.font_size));
    output.push_str(" color=");
    output.push_str(&format_color(run.visuals.color));
    output.push_str(" font-weight=");
    output.push_str(match run.visuals.font_weight {
        FontWeight::Normal => "normal",
        FontWeight::Bold => "bold",
    });
    output.push_str(" text-decoration=");
    output.push_str(match run.visuals.text_decoration {
        TextDecoration::None => "none",
        TextDecoration::Underline => "underline",
    });
    output.push_str(" link=");
    output.push_str(if run.visuals.is_link { "true" } else { "false" });
    if let Some(href) = &run.visuals.href {
        output.push_str(" href=\"");
        output.push_str(&escape_dump_string(href));
        output.push('"');
    }
    output.push('\n');
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

fn format_color(color: VisualColor) -> String {
    format!("rgba({},{},{},{})", color.r, color.g, color.b, color.a)
}

fn control_type_name(control_type: FormControlType) -> &'static str {
    match control_type {
        FormControlType::Text => "text",
        FormControlType::Search => "search",
        FormControlType::Password => "password",
        FormControlType::Email => "email",
        FormControlType::Checkbox => "checkbox",
        FormControlType::Radio => "radio",
        FormControlType::Select => "select",
        FormControlType::Textarea => "textarea",
        FormControlType::Submit => "submit",
        FormControlType::Button => "button",
        FormControlType::Reset => "reset",
    }
}

#[derive(Debug, Default, Clone)]
struct LayoutState {
    links: Vec<LinkHitBox>,
    form_controls: Vec<FormControlHitBox>,
    next_form_id: usize,
    next_control_id: usize,
}

#[derive(Debug, Default)]
struct InlineCollectionState {
    pending_space: bool,
}

#[derive(Debug)]
enum InlineItem {
    Word(WordItem),
    Break(BreakItem),
    Image(ImageItem),
    Control(ControlItem),
}

#[derive(Debug)]
struct BreakItem {
    line_height: f32,
}

#[derive(Debug)]
struct WordItem {
    text: String,
    font_size: f32,
    line_height: f32,
    visuals: TextVisuals,
    is_space: bool,
    allow_wrap: bool,
}

#[derive(Debug)]
struct ImageItem {
    tag_name: String,
    src: Option<String>,
    alt: Option<String>,
    image: Option<ImageResource>,
    graphics: Vec<GraphicCommand>,
    visible: bool,
    width: f32,
    height: f32,
    metadata: ElementMetadata,
    padding: EdgeSizes,
    border: EdgeSizes,
    margin: EdgeSizes,
    visuals: BoxVisuals,
    link: Option<LinkTarget>,
}

#[derive(Debug)]
struct ControlItem {
    tag_name: String,
    metadata: ElementMetadata,
    control_type: FormControlType,
    form: Option<FormMetadata>,
    name: Option<String>,
    value: String,
    placeholder: Option<String>,
    disabled: bool,
    checked: bool,
    options: Vec<FormOption>,
    label: String,
    width: f32,
    height: f32,
    padding: EdgeSizes,
    border: EdgeSizes,
    margin: EdgeSizes,
    visuals: BoxVisuals,
    text_visuals: TextVisuals,
    interactive: bool,
}

struct InlineLayoutContext<'a> {
    start_x: f32,
    cursor_x: f32,
    cursor_y: f32,
    available_width: f32,
    line_ascent: f32,
    line_descent: f32,
    line_fragments: Vec<PendingInlineFragment>,
    text_align: StyleTextAlign,
    contents: &'a mut Vec<LayoutItem>,
    state: &'a mut LayoutState,
}

impl InlineLayoutContext<'_> {
    fn place_word(&mut self, word: WordItem) {
        let measured_width = measure_text(
            &word.text,
            word.font_size,
            word.visuals.font_weight,
            word.visuals.font_family,
        );
        let run_width = measured_width.min(self.available_width.max(0.0));
        let height = word.line_height;
        let baseline = text_baseline(word.font_size);

        if word.is_space && self.cursor_x <= self.start_x {
            return;
        }

        if word.is_space && self.cursor_x + measured_width > self.line_right() {
            return;
        }

        if word.allow_wrap
            && !word.is_space
            && self.cursor_x > self.start_x
            && self.cursor_x + measured_width > self.line_right()
        {
            self.flush_line(word.line_height);
        }

        let rect = Rect {
            x: self.cursor_x,
            y: self.cursor_y,
            width: run_width,
            height,
        };

        self.line_ascent = self.line_ascent.max(baseline);
        self.line_descent = self.line_descent.max((height - baseline).max(0.0));
        self.line_fragments
            .push(PendingInlineFragment::Text(TextRun {
                rect,
                text: word.text,
                font_size: word.font_size,
                visuals: word.visuals,
            }));
        self.cursor_x += run_width;
    }

    fn place_image(&mut self, image: ImageItem) {
        let flow_width = image.width
            + image.padding.horizontal()
            + image.border.horizontal()
            + image.margin.horizontal();
        let flow_height = image.height
            + image.padding.vertical()
            + image.border.vertical()
            + image.margin.vertical();

        if self.cursor_x > self.start_x && self.cursor_x + flow_width > self.line_right() {
            self.flush_line(flow_height.max(line_height(16.0)));
        }

        let rect = Rect {
            x: self.cursor_x + image.margin.left + image.border.left + image.padding.left,
            y: self.cursor_y + image.margin.top + image.border.top + image.padding.top,
            width: image.width.min(self.available_width.max(0.0)),
            height: image.height,
        };
        let dimensions = Dimensions {
            content: rect,
            padding: image.padding,
            border: image.border,
            margin: image.margin,
        };

        let mut image_contents = Vec::new();
        if image.image.is_none()
            && let Some(alt) = &image.alt
            && !alt.trim().is_empty()
        {
            image_contents.push(LayoutItem::Text(TextRun {
                rect: Rect {
                    x: rect.x + 4.0,
                    y: rect.y + 4.0,
                    width: measure_text(alt, 12.0, FontWeight::Normal, FontFamily::Sans)
                        .min(rect.width.max(0.0)),
                    height: line_height(12.0).min(rect.height.max(0.0)),
                },
                text: alt.clone(),
                font_size: 12.0,
                visuals: TextVisuals {
                    color: VisualColor {
                        r: 80,
                        g: 86,
                        b: 96,
                        a: 255,
                    },
                    font_weight: FontWeight::Normal,
                    font_family: FontFamily::Sans,
                    text_decoration: TextDecoration::None,
                    is_link: false,
                    href: None,
                    link_node_id: None,
                },
            }));
        }

        let layout_box = LayoutBox {
            kind: LayoutKind::Image {
                tag_name: image.tag_name,
                metadata: Box::new(image.metadata),
                src: image.src,
                alt: image.alt,
                image: image.image,
                graphics: image.graphics,
                visible: image.visible,
            },
            dimensions,
            visuals: image.visuals,
            positioning: Positioning::default(),
            flow: LayoutFlow::Block,
            scroll_container: None,
            contents: image_contents,
        };
        let baseline = flow_height;
        self.line_ascent = self.line_ascent.max(baseline);
        self.line_descent = self.line_descent.max(0.0);
        self.line_fragments.push(PendingInlineFragment::Box {
            layout_box: Box::new(layout_box),
            link: image.link,
            flow_height,
            margin_top: image.margin.top,
            border_top: image.border.top,
            padding_top: image.padding.top,
        });
        self.cursor_x += flow_width.min(self.available_width.max(0.0));
    }

    fn place_control(&mut self, control: ControlItem) {
        let flow_width = control.width
            + control.padding.horizontal()
            + control.border.horizontal()
            + control.margin.horizontal();
        let flow_height = control.height
            + control.padding.vertical()
            + control.border.vertical()
            + control.margin.vertical();

        if self.cursor_x > self.start_x && self.cursor_x + flow_width > self.line_right() {
            self.flush_line(flow_height.max(line_height(14.0)));
        }

        let rect = Rect {
            x: self.cursor_x + control.margin.left + control.border.left + control.padding.left,
            y: self.cursor_y + control.margin.top + control.border.top + control.padding.top,
            width: control.width.min(self.available_width.max(0.0)),
            height: control.height,
        };
        let dimensions = Dimensions {
            content: rect,
            padding: control.padding,
            border: control.border,
            margin: control.margin,
        };
        let text_rect = Rect {
            x: rect.x + 4.0,
            y: rect.y + ((rect.height - line_height(14.0)).max(0.0) / 2.0),
            width: measure_text(
                &control.label,
                14.0,
                control.text_visuals.font_weight,
                control.text_visuals.font_family,
            )
            .min((rect.width - 8.0).max(0.0)),
            height: line_height(14.0).min(rect.height.max(0.0)),
        };
        let mut contents = Vec::new();
        if !control.label.is_empty() {
            contents.push(LayoutItem::Text(TextRun {
                rect: text_rect,
                text: control.label.clone(),
                font_size: 14.0,
                visuals: control.text_visuals.clone(),
            }));
        }
        let layout_box = LayoutBox {
            kind: LayoutKind::FormControl {
                tag_name: control.tag_name,
                metadata: Box::new(control.metadata),
                control_type: control.control_type,
                form: control.form.clone(),
                name: control.name.clone(),
                value: control.value.clone(),
                placeholder: control.placeholder.clone(),
                disabled: control.disabled,
                checked: control.checked,
                options: control.options.clone(),
            },
            dimensions,
            visuals: control.visuals,
            positioning: Positioning::default(),
            flow: LayoutFlow::Block,
            scroll_container: None,
            contents,
        };
        let id = self.state.next_control_id;
        self.state.next_control_id = self.state.next_control_id.saturating_add(1);
        self.line_ascent = self.line_ascent.max(flow_height);
        self.line_descent = self.line_descent.max(0.0);
        self.line_fragments.push(PendingInlineFragment::Control {
            layout_box: Box::new(layout_box),
            hit: control.interactive.then_some(FormControlHitBox {
                id,
                rect: dimensions.border_box(),
                control_type: control.control_type,
                form: control.form,
                name: control.name,
                value: control.value,
                placeholder: control.placeholder,
                disabled: control.disabled,
                checked: control.checked,
                options: control.options,
            }),
            flow_height,
            margin_top: control.margin.top,
            border_top: control.border.top,
            padding_top: control.padding.top,
        });
        self.cursor_x += flow_width.min(self.available_width.max(0.0));
    }

    fn force_break(&mut self, break_item: BreakItem) {
        self.flush_line(break_item.line_height);
    }

    fn finish(mut self) -> f32 {
        self.flush_line(0.0);
        self.cursor_y
    }

    fn flush_line(&mut self, fallback_height: f32) {
        if self.line_fragments.is_empty() {
            if fallback_height > 0.0 {
                self.cursor_x = self.start_x;
                self.cursor_y += fallback_height;
            }
            self.line_ascent = 0.0;
            self.line_descent = 0.0;
            return;
        }

        let line_height = (self.line_ascent + self.line_descent).max(fallback_height);
        let baseline = self
            .line_ascent
            .max((line_height - self.line_descent).max(0.0));
        let mut fragments = Vec::new();
        let mut line_left = f32::INFINITY;
        let mut line_right = self.start_x;
        let link_start = self.state.links.len();
        let control_start = self.state.form_controls.len();

        for pending in self.line_fragments.drain(..) {
            match pending {
                PendingInlineFragment::Text(mut run) => {
                    let run_baseline = text_baseline(run.font_size);
                    run.rect.y = self.cursor_y + baseline - run_baseline;
                    if run.visuals.is_link
                        && let Some(href) = &run.visuals.href
                    {
                        self.state.links.push(LinkHitBox {
                            rect: run.rect,
                            href: href.clone(),
                            node_id: run.visuals.link_node_id,
                        });
                    }
                    line_left = line_left.min(run.rect.x);
                    line_right = line_right.max(run.rect.x + run.rect.width);
                    fragments.push(InlineFragment::Text(run));
                }
                PendingInlineFragment::Box {
                    mut layout_box,
                    link,
                    flow_height,
                    margin_top,
                    border_top,
                    padding_top,
                } => {
                    let flow_top = self.cursor_y + baseline - flow_height;
                    let content_y = flow_top + margin_top + border_top + padding_top;
                    let delta_y = content_y - layout_box.dimensions.content.y;
                    shift_box_y(&mut layout_box, delta_y);
                    if let Some(link) = link {
                        self.state.links.push(LinkHitBox {
                            rect: layout_box.dimensions.border_box(),
                            href: link.href,
                            node_id: Some(link.node_id),
                        });
                    }
                    let margin_box = layout_box.dimensions.margin_box();
                    line_left = line_left.min(margin_box.x);
                    line_right = line_right.max(margin_box.x + margin_box.width);
                    fragments.push(InlineFragment::Box(*layout_box));
                }
                PendingInlineFragment::Control {
                    mut layout_box,
                    mut hit,
                    flow_height,
                    margin_top,
                    border_top,
                    padding_top,
                } => {
                    let flow_top = self.cursor_y + baseline - flow_height;
                    let content_y = flow_top + margin_top + border_top + padding_top;
                    let delta_y = content_y - layout_box.dimensions.content.y;
                    shift_box_y(&mut layout_box, delta_y);
                    if let Some(hit) = &mut hit {
                        hit.rect = layout_box.dimensions.border_box();
                        self.state.form_controls.push(hit.clone());
                    }
                    let margin_box = layout_box.dimensions.margin_box();
                    line_left = line_left.min(margin_box.x);
                    line_right = line_right.max(margin_box.x + margin_box.width);
                    fragments.push(InlineFragment::Box(*layout_box));
                }
            }
        }

        let line_x = if line_left.is_finite() {
            line_left
        } else {
            self.start_x
        };
        let line_width = (line_right - line_x).max(0.0);
        let delta_x = match self.text_align {
            StyleTextAlign::Left => 0.0,
            StyleTextAlign::Center => (self.available_width - line_width).max(0.0) / 2.0,
            StyleTextAlign::Right => (self.available_width - line_width).max(0.0),
        };
        if delta_x > 0.0 {
            shift_inline_fragments(&mut fragments, delta_x);
            shift_hit_regions(self.state, link_start, control_start, delta_x, 0.0);
        }
        self.contents.push(LayoutItem::LineBox(LineBox {
            rect: Rect {
                x: line_x + delta_x,
                y: self.cursor_y,
                width: line_width,
                height: line_height,
            },
            baseline,
            fragments,
        }));
        self.cursor_x = self.start_x;
        self.cursor_y += line_height;
        self.line_ascent = 0.0;
        self.line_descent = 0.0;
    }

    fn line_right(&self) -> f32 {
        self.start_x + self.available_width
    }
}

fn shift_inline_fragments(fragments: &mut [InlineFragment], delta_x: f32) {
    for fragment in fragments {
        match fragment {
            InlineFragment::Text(run) => {
                run.rect.x += delta_x;
            }
            InlineFragment::Box(layout_box) => {
                shift_box(layout_box, delta_x, 0.0);
            }
        }
    }
}

#[derive(Debug)]
enum PendingInlineFragment {
    Text(TextRun),
    Box {
        layout_box: Box<LayoutBox>,
        link: Option<LinkTarget>,
        flow_height: f32,
        margin_top: f32,
        border_top: f32,
        padding_top: f32,
    },
    Control {
        layout_box: Box<LayoutBox>,
        hit: Option<FormControlHitBox>,
        flow_height: f32,
        margin_top: f32,
        border_top: f32,
        padding_top: f32,
    },
}

fn text_baseline(font_size: f32) -> f32 {
    font_size * 0.86
}

fn shift_box_y(layout_box: &mut LayoutBox, delta_y: f32) {
    shift_box(layout_box, 0.0, delta_y);
}

fn shift_box(layout_box: &mut LayoutBox, delta_x: f32, delta_y: f32) {
    if delta_x == 0.0 && delta_y == 0.0 {
        return;
    }

    layout_box.dimensions.content.x += delta_x;
    layout_box.dimensions.content.y += delta_y;
    for item in &mut layout_box.contents {
        match item {
            LayoutItem::LineBox(line) => {
                line.rect.x += delta_x;
                line.rect.y += delta_y;
                for fragment in &mut line.fragments {
                    match fragment {
                        InlineFragment::Text(run) => {
                            run.rect.x += delta_x;
                            run.rect.y += delta_y;
                        }
                        InlineFragment::Box(child) => shift_box(child, delta_x, delta_y),
                    }
                }
            }
            LayoutItem::Text(run) => {
                run.rect.x += delta_x;
                run.rect.y += delta_y;
            }
            LayoutItem::Box(child) => shift_box(child, delta_x, delta_y),
        }
    }
}

fn shift_hit_regions(
    state: &mut LayoutState,
    link_start: usize,
    control_start: usize,
    delta_x: f32,
    delta_y: f32,
) {
    if delta_x == 0.0 && delta_y == 0.0 {
        return;
    }
    for link in state.links.iter_mut().skip(link_start) {
        link.rect.x += delta_x;
        link.rect.y += delta_y;
    }
    for control in state.form_controls.iter_mut().skip(control_start) {
        control.rect.x += delta_x;
        control.rect.y += delta_y;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DecodedImage, FontFamily, FontWeight, FormControlType, GraphicCommand, ImageMap,
        ImageResource, InlineFragment, LayoutFlow, LayoutItem, LayoutKind, PositionMode,
        TextDecoration, Viewport, VisualColor, dump_layout_tree, layout_tree,
        layout_tree_with_images, line_height, measure_text,
    };
    use webby_core::{WebbyError, WebbyResult};
    use webby_html::parse_document;
    use webby_style::{
        Border as StyleBorder, Color as StyleColor, Edges as StyleEdges, StyledNode, style_document,
    };
    use webby_text::{FontFamily as WebbyTextFontFamily, FontWeight as WebbyTextFontWeight};

    #[test]
    fn viewport_accepts_positive_dimensions() {
        assert!(Viewport::new(800.0, 600.0).is_ok());
    }

    #[test]
    fn viewport_rejects_zero_dimensions() {
        let result = Viewport::new(0.0, 600.0);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn viewport_rejects_nan_dimensions() {
        let result = Viewport::new(f32::NAN, 600.0);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn generated_viewport_boundaries_are_rejected_or_laid_out_safely() -> WebbyResult<()> {
        for width in [
            f32::NEG_INFINITY,
            -1.0,
            0.0,
            f32::NAN,
            0.001,
            1.0,
            32.0,
            1_000_000.0,
            f32::INFINITY,
        ] {
            match Viewport::with_width(width) {
                Ok(_) => {
                    let first = layout_html("<body><p>boundary layout text</p></body>", width)?;
                    let second = layout_html("<body><p>boundary layout text</p></body>", width)?;

                    assert_eq!(dump_layout_tree(&first), dump_layout_tree(&second));
                }
                Err(error) => {
                    assert!(matches!(error, WebbyError::InvalidInput { .. }));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn single_block_layout_uses_viewport_width() -> WebbyResult<()> {
        let tree = layout_html("<body><div>Hello</div></body>", 400.0)?;
        let div = find_box(&tree.root, "div")?;

        assert_eq!(div.dimensions.content.width, 384.0);
        assert_eq!(div.text_runs().len(), 1);
        Ok(())
    }

    #[test]
    fn nested_block_layout_stacks_vertically() -> WebbyResult<()> {
        let tree = layout_html("<body><div><p>One</p><p>Two</p></div></body>", 400.0)?;
        let paragraphs = find_boxes(&tree.root, "p");

        assert_eq!(paragraphs.len(), 2);
        assert!(paragraphs[1].dimensions.margin_box().y > paragraphs[0].dimensions.margin_box().y);
        Ok(())
    }

    #[test]
    fn margin_padding_and_border_dimensions_are_applied() -> WebbyResult<()> {
        let tree = layout_html("<body><img width=\"100\" height=\"40\"></body>", 400.0)?;
        let body = find_box(&tree.root, "body")?;
        let image = find_image(&tree.root)?;

        assert_eq!(body.dimensions.margin.left, 8.0);
        assert_eq!(image.dimensions.content.width, 100.0);
        assert_eq!(image.dimensions.content.height, 40.0);
        assert_eq!(image.dimensions.border.left, 1.0);
        Ok(())
    }

    #[test]
    fn explicit_block_margin_padding_border_dimensions_are_applied() -> WebbyResult<()> {
        let document = parse_document("<body><div>Box</div></body>")?;
        let mut styled = style_document(&document);
        let applied = apply_styled_tag_mut(&mut styled, "div", &mut |div| {
            div.style.margin = StyleEdges::trbl(3.0, 4.0, 5.0, 6.0);
            div.style.padding = StyleEdges::trbl(7.0, 8.0, 9.0, 10.0);
            div.style.border = StyleBorder {
                width: 2.0,
                color: StyleColor {
                    r: 10,
                    g: 20,
                    b: 30,
                    a: 255,
                },
            };
            div.style.background_color = StyleColor {
                r: 240,
                g: 241,
                b: 242,
                a: 255,
            };
        });
        assert!(applied);

        let tree = layout_tree(&styled, Viewport::with_width(400.0)?)?;
        let div = find_box(&tree.root, "div")?;

        assert_eq!(div.dimensions.margin.left, 6.0);
        assert_eq!(div.dimensions.padding.top, 7.0);
        assert_eq!(div.dimensions.border.right, 2.0);
        assert_eq!(
            div.visuals.border.color,
            VisualColor {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            }
        );
        assert_eq!(
            div.visuals.background_color,
            VisualColor {
                r: 240,
                g: 241,
                b: 242,
                a: 255,
            }
        );
        Ok(())
    }

    #[test]
    fn css_width_height_margin_padding_and_font_size_affect_layout() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>
                div { width: 120px; height: 40px; margin: 4px; padding: 6px; border: 2px solid red; }
                p { font-size: 30px; margin: 0; }
            </style><body><div>Box</div><p>Large</p></body>",
            400.0,
        )?;
        let div = find_box(&tree.root, "div")?;
        let p = find_box(&tree.root, "p")?;

        assert_eq!(div.dimensions.content.width, 120.0);
        assert_eq!(div.dimensions.content.height, 40.0);
        assert_eq!(div.dimensions.margin.top, 4.0);
        assert_eq!(div.dimensions.padding.left, 6.0);
        assert_eq!(div.dimensions.border.left, 2.0);
        assert_eq!(p.text_runs()[0].font_size, 30.0);
        Ok(())
    }

    #[test]
    fn css_edge_shorthand_variants_affect_layout() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>
                body { margin: 0; }
                div { margin: 1px 2px 3px 4px; padding: 5px 6px 7px; border: 2px solid red; }
            </style><body><div>Box</div></body>",
            300.0,
        )?;
        let div = find_box(&tree.root, "div")?;

        assert_eq!(div.dimensions.margin.top, 1.0);
        assert_eq!(div.dimensions.margin.right, 2.0);
        assert_eq!(div.dimensions.margin.bottom, 3.0);
        assert_eq!(div.dimensions.margin.left, 4.0);
        assert_eq!(div.dimensions.padding.top, 5.0);
        assert_eq!(div.dimensions.padding.right, 6.0);
        assert_eq!(div.dimensions.padding.bottom, 7.0);
        assert_eq!(div.dimensions.padding.left, 6.0);
        assert_eq!(div.dimensions.border.left, 2.0);
        Ok(())
    }

    #[test]
    fn box_sizing_border_box_resolves_content_width_and_height() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>
                body { margin: 0; }
                div {
                    width: 100px;
                    height: 80px;
                    padding: 10px;
                    border: 5px solid red;
                    box-sizing: border-box;
                }
            </style><body><div>Box</div></body>",
            300.0,
        )?;
        let div = find_box(&tree.root, "div")?;

        assert_eq!(div.dimensions.content.width, 70.0);
        assert_eq!(div.dimensions.content.height, 50.0);
        assert_eq!(div.dimensions.border_box().width, 100.0);
        assert_eq!(div.dimensions.border_box().height, 80.0);
        Ok(())
    }

    #[test]
    fn auto_width_uses_available_block_width() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>body { margin: 0; } div { width: auto; margin: 4px; padding: 6px; border: 2px solid red; }</style><body><div>Box</div></body>",
            200.0,
        )?;
        let div = find_box(&tree.root, "div")?;

        assert_eq!(div.dimensions.content.width, 176.0);
        assert_eq!(div.dimensions.margin_box().width, 200.0);
        Ok(())
    }

    #[test]
    fn percentage_width_resolves_against_containing_block() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>body { margin: 0; } div { width: 50%; }</style><body><div>Box</div></body>",
            240.0,
        )?;
        let div = find_box(&tree.root, "div")?;

        assert_eq!(div.dimensions.content.width, 120.0);
        Ok(())
    }

    #[test]
    fn min_and_max_width_clamp_resolved_content_width() -> WebbyResult<()> {
        let min_tree = layout_html(
            "<style>body { margin: 0; } div { width: 50%; min-width: 140px; max-width: 180px; }</style><body><div>Box</div></body>",
            200.0,
        )?;
        let max_tree = layout_html(
            "<style>body { margin: 0; } div { width: 50%; min-width: 140px; max-width: 180px; }</style><body><div>Box</div></body>",
            500.0,
        )?;

        assert_eq!(
            find_box(&min_tree.root, "div")?.dimensions.content.width,
            140.0
        );
        assert_eq!(
            find_box(&max_tree.root, "div")?.dimensions.content.width,
            180.0
        );
        Ok(())
    }

    #[test]
    fn layout_output_preserves_text_color() -> WebbyResult<()> {
        let tree = layout_html("<body><p><a href=\"/x\">Link</a></p></body>", 400.0)?;
        let p = find_box(&tree.root, "p")?;

        assert_eq!(
            p.text_runs()[0].visuals.color,
            VisualColor {
                r: 0,
                g: 0,
                b: 238,
                a: 255,
            }
        );
        Ok(())
    }

    #[test]
    fn layout_output_preserves_font_weight() -> WebbyResult<()> {
        let tree = layout_html("<body><h1>Title</h1></body>", 400.0)?;
        let h1 = find_box(&tree.root, "h1")?;

        assert_eq!(h1.text_runs()[0].visuals.font_weight, FontWeight::Bold);
        Ok(())
    }

    #[test]
    fn layout_output_preserves_text_decoration() -> WebbyResult<()> {
        let tree = layout_html("<body><a href=\"/x\">Link</a></body>", 400.0)?;
        let body = find_box(&tree.root, "body")?;

        assert_eq!(
            body.text_runs()[0].visuals.text_decoration,
            TextDecoration::Underline
        );
        Ok(())
    }

    #[test]
    fn layout_output_preserves_block_background_color() -> WebbyResult<()> {
        let tree = layout_html("<body>Text</body>", 400.0)?;
        let body = find_box(&tree.root, "body")?;

        assert_eq!(
            body.visuals.background_color,
            VisualColor {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            }
        );
        Ok(())
    }

    #[test]
    fn layout_output_preserves_border_color_and_widths() -> WebbyResult<()> {
        let tree = layout_html("<body><img width=\"20\" height=\"10\"></body>", 400.0)?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.border.left, 1.0);
        assert_eq!(
            image.visuals.border.color,
            VisualColor {
                r: 180,
                g: 180,
                b: 180,
                a: 255,
            }
        );
        Ok(())
    }

    #[test]
    fn parent_height_is_derived_from_children() -> WebbyResult<()> {
        let tree = layout_html("<body><div><p>One</p><p>Two</p></div></body>", 400.0)?;
        let div = find_box(&tree.root, "div")?;

        assert!(div.dimensions.content.height > line_height(16.0));
        Ok(())
    }

    #[test]
    fn heading_text_has_larger_line_height() -> WebbyResult<()> {
        let tree = layout_html("<body><h1>Title</h1><p>Body</p></body>", 400.0)?;
        let h1 = find_box(&tree.root, "h1")?;
        let p = find_box(&tree.root, "p")?;

        assert!(h1.text_runs()[0].rect.height > p.text_runs()[0].rect.height);
        Ok(())
    }

    #[test]
    fn paragraph_spacing_comes_from_margins() -> WebbyResult<()> {
        let tree = layout_html("<body><p>One</p><p>Two</p></body>", 400.0)?;
        let paragraphs = find_boxes(&tree.root, "p");

        assert_eq!(paragraphs[0].dimensions.margin.top, 8.0);
        assert!(paragraphs[1].dimensions.margin_box().y > paragraphs[0].dimensions.content.y);
        Ok(())
    }

    #[test]
    fn text_wraps_when_width_is_exceeded() -> WebbyResult<()> {
        let tree = layout_html("<body><p>one two three four five six</p></body>", 70.0)?;
        let p = find_box(&tree.root, "p")?;
        let first_y = p.text_runs()[0].rect.y;

        assert!(p.text_runs().iter().any(|run| run.rect.y > first_y));
        Ok(())
    }

    #[test]
    fn layout_wrapping_changes_with_font_metrics() -> WebbyResult<()> {
        let small = layout_html(
            "<style>p { font-size: 10px; margin: 0; }</style><body><p>alpha beta gamma delta</p></body>",
            180.0,
        )?;
        let large = layout_html(
            "<style>p { font-size: 30px; margin: 0; }</style><body><p>alpha beta gamma delta</p></body>",
            180.0,
        )?;
        let small_lines = distinct_text_run_y_count(find_box(&small.root, "p")?);
        let large_lines = distinct_text_run_y_count(find_box(&large.root, "p")?);

        assert!(large_lines > small_lines);
        Ok(())
    }

    #[test]
    fn text_run_width_matches_shared_text_metrics() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>p { font-size: 22px; font-weight: bold; margin: 0; }</style><body><p>Metric</p></body>",
            400.0,
        )?;
        let p = find_box(&tree.root, "p")?;
        let text_runs = p.text_runs();
        let run = text_runs.first().ok_or_else(|| WebbyError::Layout {
            message: "expected text run".to_string(),
        })?;
        let expected =
            webby_text::measure_text(&run.text, run.font_size, WebbyTextFontWeight::Bold);

        assert_eq!(run.visuals.font_weight, FontWeight::Bold);
        assert_eq!(run.rect.width, expected.width);
        Ok(())
    }

    #[test]
    fn br_forces_line_break() -> WebbyResult<()> {
        let tree = layout_html("<body><p>Hello<br>Webby</p></body>", 400.0)?;
        let p = find_box(&tree.root, "p")?;

        assert!(p.text_runs()[1].rect.y > p.text_runs()[0].rect.y);
        Ok(())
    }

    #[test]
    fn br_inside_paragraph_heading_link_and_span_forces_line_break() -> WebbyResult<()> {
        let paragraph = layout_html("<body><p>One<br>Two</p></body>", 400.0)?;
        assert_second_run_is_lower(find_box(&paragraph.root, "p")?);

        let heading = layout_html("<body><h1>One<br>Two</h1></body>", 400.0)?;
        assert_second_run_is_lower(find_box(&heading.root, "h1")?);

        let link = layout_html("<body><p><a href=\"/x\">One<br>Two</a></p></body>", 400.0)?;
        assert_second_run_is_lower(find_box(&link.root, "p")?);
        assert!(link.links.iter().all(|hit| hit.href == "/x"));

        let span = layout_html("<body><p><span>One<br>Two</span></p></body>", 400.0)?;
        assert_second_run_is_lower(find_box(&span.root, "p")?);
        Ok(())
    }

    #[test]
    fn leading_br_inside_h1_uses_h1_line_height() -> WebbyResult<()> {
        let tree = layout_html("<body><h1><br>Title</h1></body>", 400.0)?;
        let h1 = find_box(&tree.root, "h1")?;

        assert_near(
            h1.text_runs()[0].rect.y,
            h1.dimensions.content.y + line_height(32.0),
        );
        Ok(())
    }

    #[test]
    fn consecutive_br_in_large_font_uses_current_line_height() -> WebbyResult<()> {
        let tree = layout_html("<body><h2><br><br>Title</h2></body>", 400.0)?;
        let h2 = find_box(&tree.root, "h2")?;

        assert_near(
            h2.text_runs()[0].rect.y,
            h2.dimensions.content.y + line_height(24.0) * 2.0,
        );
        Ok(())
    }

    #[test]
    fn hidden_elements_do_not_appear_in_layout() -> WebbyResult<()> {
        let tree = layout_html(
            "<html><head><title>Hidden</title><script>bad()</script></head><body>Visible</body></html>",
            400.0,
        )?;
        let dump = dump_layout_tree(&tree);

        assert!(!dump.contains("Hidden"));
        assert!(!dump.contains("script"));
        assert!(dump.contains("Visible"));
        Ok(())
    }

    #[test]
    fn display_none_removes_content_from_layout_and_hit_testing() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.gone { display: none; }</style><body><p class=\"gone\"><a href=\"/hidden\">Hidden</a><input name=\"q\"></p><p>Shown</p></body>",
            300.0,
        )?;
        let dump = dump_layout_tree(&tree);

        assert!(!dump.contains("Hidden"));
        assert!(dump.contains("Shown"));
        assert!(tree.links.is_empty());
        assert!(tree.form_controls.is_empty());
        Ok(())
    }

    #[test]
    fn visibility_hidden_preserves_layout_but_removes_paint_and_interaction() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.ghost { visibility: hidden; }</style><body><p class=\"ghost\"><a href=\"/hidden\">Hidden</a><input name=\"q\"></p></body>",
            300.0,
        )?;
        let paragraph = find_box(&tree.root, "p")?;
        let text_runs = paragraph.text_runs();
        let run = text_runs.first().ok_or_else(|| WebbyError::Layout {
            message: "expected hidden text run to preserve layout".to_string(),
        })?;

        assert!(paragraph.dimensions.content.height > 0.0);
        assert_eq!(run.visuals.color.a, 0);
        assert_eq!(run.visuals.href, None);
        assert!(tree.links.is_empty());
        assert!(tree.form_controls.is_empty());
        Ok(())
    }

    #[test]
    fn relative_position_offsets_box_and_hit_regions_without_corrupting_flow() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.rel { position: relative; left: 12px; top: 9px; }</style><body><div>Before</div><div class=\"rel\"><a href=\"/rel\">Relative</a><input name=\"q\"></div><div>After</div></body>",
            320.0,
        )?;
        let boxes = find_boxes(&tree.root, "div");
        let _before = boxes.first().ok_or_else(|| WebbyError::Layout {
            message: "expected before div".to_string(),
        })?;
        let relative = boxes.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected relative div".to_string(),
        })?;
        let after = boxes.get(2).ok_or_else(|| WebbyError::Layout {
            message: "expected after div".to_string(),
        })?;

        assert_eq!(relative.positioning.mode, PositionMode::Relative);
        assert_near(relative.positioning.offset_x, 12.0);
        assert_near(relative.positioning.offset_y, 9.0);
        let normal_relative_y = relative.dimensions.margin_box().y - relative.positioning.offset_y;
        assert_near(
            after.dimensions.margin_box().y,
            normal_relative_y + relative.dimensions.margin_box().height,
        );
        assert_eq!(tree.links.len(), 1);
        assert!(tree.links[0].rect.x >= relative.dimensions.content.x);
        assert_eq!(tree.form_controls.len(), 1);
        assert!(tree.form_controls[0].rect.x >= relative.dimensions.content.x);
        Ok(())
    }

    #[test]
    fn absolute_position_is_removed_from_block_flow_and_keeps_hit_regions() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.abs { position: absolute; left: 20px; top: 40px; width: 80px; }</style><body><div>Before</div><div class=\"abs\"><a href=\"/abs\">Absolute</a></div><div>After</div></body>",
            320.0,
        )?;
        let boxes = find_boxes(&tree.root, "div");
        let before = boxes.first().ok_or_else(|| WebbyError::Layout {
            message: "expected before div".to_string(),
        })?;
        let absolute = boxes.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected absolute div".to_string(),
        })?;
        let after = boxes.get(2).ok_or_else(|| WebbyError::Layout {
            message: "expected after div".to_string(),
        })?;

        assert_eq!(absolute.positioning.mode, PositionMode::Absolute);
        assert_near(absolute.dimensions.margin_box().x, 28.0);
        assert_near(absolute.dimensions.margin_box().y, 48.0);
        assert_near(
            after.dimensions.margin_box().y,
            before.dimensions.margin_box().y + before.dimensions.margin_box().height,
        );
        assert_eq!(tree.links.len(), 1);
        assert!(tree.links[0].rect.x >= absolute.dimensions.content.x);
        assert!(tree.links[0].rect.y >= absolute.dimensions.content.y);
        Ok(())
    }

    #[test]
    fn positioned_layout_dump_is_deterministic_and_explicit() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>p { position: relative; left: 4px; top: 3px; }</style><body><p>Offset</p></body>",
            300.0,
        )?;
        let dump = dump_layout_tree(&tree);

        assert_eq!(dump, dump_layout_tree(&tree));
        assert!(dump.contains("position=relative"));
        assert!(dump.contains("offset=4.0,3.0"));
        Ok(())
    }

    #[test]
    fn flex_row_lays_out_items_with_gap_and_preserves_order() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>nav { display: flex; gap: 10px; } a { width: 40px; }</style><body><nav><a href=\"/one\">One</a><a href=\"/two\">Two</a></nav></body>",
            300.0,
        )?;
        let nav = find_box(&tree.root, "nav")?;
        let links = find_boxes(nav, "a");
        let first = links.first().ok_or_else(|| WebbyError::Layout {
            message: "expected first flex link".to_string(),
        })?;
        let second = links.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected second flex link".to_string(),
        })?;

        assert_eq!(nav.flow, LayoutFlow::Flex);
        assert_eq!(tree.links.len(), 2);
        assert_near(
            second.dimensions.margin_box().x,
            first.dimensions.margin_box().x + first.dimensions.margin_box().width + 10.0,
        );
        assert_eq!(tree.links[0].href, "/one");
        assert_eq!(tree.links[1].href, "/two");
        Ok(())
    }

    #[test]
    fn flex_grow_distributes_remaining_row_width() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.row { display: flex; gap: 10px; } .grow { flex: 1; }</style><body><div class=\"row\"><div class=\"grow\">A</div><div class=\"grow\">B</div></div></body>",
            220.0,
        )?;
        let row = find_box(&tree.root, "div")?;
        let items = find_boxes(row, "div");
        let first = items.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected first flex item".to_string(),
        })?;
        let second = items.get(2).ok_or_else(|| WebbyError::Layout {
            message: "expected second flex item".to_string(),
        })?;

        assert_near(
            first.dimensions.content.width,
            second.dimensions.content.width,
        );
        assert_near(
            first.dimensions.content.width + second.dimensions.content.width + 10.0,
            row.dimensions.content.width,
        );
        Ok(())
    }

    #[test]
    fn flex_column_stacks_items_with_gap() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.col { display: flex; flex-direction: column; gap: 7px; } p { margin: 0; }</style><body><div class=\"col\"><p>One</p><p>Two</p></div></body>",
            260.0,
        )?;
        let column = find_box(&tree.root, "div")?;
        let items = find_boxes(column, "p");
        let first = items.first().ok_or_else(|| WebbyError::Layout {
            message: "expected first flex item".to_string(),
        })?;
        let second = items.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected second flex item".to_string(),
        })?;

        assert_eq!(column.flow, LayoutFlow::Flex);
        assert_near(
            second.dimensions.margin_box().y,
            first.dimensions.margin_box().y + first.dimensions.margin_box().height + 7.0,
        );
        Ok(())
    }

    #[test]
    fn flex_row_justify_center_offsets_items() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.row { display: flex; justify-content: center; gap: 10px; } .item { width: 40px; }</style><body><div class=\"row\"><div class=\"item\">A</div><div class=\"item\">B</div></div></body>",
            260.0,
        )?;
        let row = find_box(&tree.root, "div")?;
        let items = find_boxes(row, "div");
        let first = items.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected first flex item".to_string(),
        })?;

        let expected_x = row.dimensions.content.x
            + (row.dimensions.content.width - 40.0 - 10.0 - 40.0).max(0.0) / 2.0;
        assert_near(first.dimensions.margin_box().x, expected_x);
        Ok(())
    }

    #[test]
    fn flex_row_justify_space_between_distributes_extra_space() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.row { display: flex; justify-content: space-between; gap: 10px; } .item { width: 40px; }</style><body><div class=\"row\"><div class=\"item\">A</div><div class=\"item\">B</div></div></body>",
            260.0,
        )?;
        let row = find_box(&tree.root, "div")?;
        let items = find_boxes(row, "div");
        let first = items.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected first flex item".to_string(),
        })?;
        let second = items.get(2).ok_or_else(|| WebbyError::Layout {
            message: "expected second flex item".to_string(),
        })?;

        assert_near(
            second.dimensions.margin_box().x,
            row.dimensions.content.x + row.dimensions.content.width - 40.0,
        );
        assert!(second.dimensions.margin_box().x > first.dimensions.margin_box().x + 50.0);
        Ok(())
    }

    #[test]
    fn flex_row_align_center_centers_shorter_item() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.row { display: flex; align-items: center; gap: 5px; } .item { width: 40px; } .tall { height: 60px; }</style><body><div class=\"row\"><div class=\"item\">A</div><div class=\"item tall\">B</div></div></body>",
            260.0,
        )?;
        let row = find_box(&tree.root, "div")?;
        let items = find_boxes(row, "div");
        let short = items.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected short flex item".to_string(),
        })?;
        let tall = items.get(2).ok_or_else(|| WebbyError::Layout {
            message: "expected tall flex item".to_string(),
        })?;

        assert_near(tall.dimensions.content.height, 60.0);
        assert_near(
            short.dimensions.margin_box().y,
            row.dimensions.content.y
                + (tall.dimensions.margin_box().height - short.dimensions.margin_box().height)
                    / 2.0,
        );
        Ok(())
    }

    #[test]
    fn flex_row_align_stretch_expands_auto_height_items() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.row { display: flex; align-items: stretch; gap: 5px; } .item { width: 40px; } .tall { height: 60px; }</style><body><div class=\"row\"><div class=\"item\">A</div><div class=\"item tall\">B</div></div></body>",
            260.0,
        )?;
        let row = find_box(&tree.root, "div")?;
        let items = find_boxes(row, "div");
        let short = items.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected short flex item".to_string(),
        })?;
        let tall = items.get(2).ok_or_else(|| WebbyError::Layout {
            message: "expected tall flex item".to_string(),
        })?;

        assert_near(tall.dimensions.content.height, 60.0);
        assert_near(
            short.dimensions.margin_box().height,
            tall.dimensions.margin_box().height,
        );
        Ok(())
    }

    #[test]
    fn flex_dump_exposes_flex_flow() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>nav { display: flex; gap: 4px; }</style><body><nav><a href=\"/\">Home</a></nav></body>",
            300.0,
        )?;
        let dump = dump_layout_tree(&tree);

        assert!(dump.contains("nav kind=block"));
        assert!(dump.contains("flow=flex"));
        Ok(())
    }

    #[test]
    fn grid_two_column_layout_uses_gap_and_source_order() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.grid { display: grid; grid-template-columns: 100px 1fr; gap: 10px; } .card { margin: 0; }</style><body><main class=\"grid\"><section class=\"card\">One</section><section class=\"card\">Two</section></main></body>",
            260.0,
        )?;
        let main = find_box(&tree.root, "main")?;
        let items = find_boxes(main, "section");
        let first = items.first().ok_or_else(|| WebbyError::Layout {
            message: "expected first grid item".to_string(),
        })?;
        let second = items.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected second grid item".to_string(),
        })?;

        assert_eq!(main.flow, LayoutFlow::Grid);
        assert_near(first.dimensions.content.width, 100.0);
        assert_near(second.dimensions.content.width, 134.0);
        assert_near(
            second.dimensions.margin_box().x,
            first.dimensions.margin_box().x + first.dimensions.margin_box().width + 10.0,
        );
        Ok(())
    }

    #[test]
    fn grid_card_layout_auto_places_rows() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.cards { display: grid; grid-template-columns: 1fr 1fr; column-gap: 8px; row-gap: 6px; } article { margin: 0; }</style><body><section class=\"cards\"><article>A</article><article>B</article><article>C</article></section></body>",
            208.0,
        )?;
        let grid = find_box(&tree.root, "section")?;
        let cards = find_boxes(grid, "article");
        let first = cards.first().ok_or_else(|| WebbyError::Layout {
            message: "expected first card".to_string(),
        })?;
        let second = cards.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected second card".to_string(),
        })?;
        let third = cards.get(2).ok_or_else(|| WebbyError::Layout {
            message: "expected third card".to_string(),
        })?;

        assert_near(
            first.dimensions.content.width,
            second.dimensions.content.width,
        );
        assert_near(
            second.dimensions.margin_box().x,
            first.dimensions.margin_box().x + first.dimensions.margin_box().width + 8.0,
        );
        assert!(third.dimensions.margin_box().y > first.dimensions.margin_box().y);
        Ok(())
    }

    #[test]
    fn grid_explicit_placement_supports_page_regions() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>
                .page { display: grid; grid-template-columns: 80px 1fr; grid-template-rows: 30px auto 25px; gap: 5px; }
                header { grid-column: 1 / 3; grid-row: 1; }
                aside { grid-column: 1; grid-row: 2; }
                main { grid-column: 2; grid-row: 2; }
                footer { grid-column: 1 / 3; grid-row: 3; }
            </style><body><div class=\"page\"><header>Head</header><aside>Side</aside><main>Content</main><footer>Foot</footer></div></body>",
            245.0,
        )?;
        let page = find_box(&tree.root, "div")?;
        let header = find_box(page, "header")?;
        let aside = find_box(page, "aside")?;
        let main = find_box(page, "main")?;
        let footer = find_box(page, "footer")?;

        assert_eq!(page.flow, LayoutFlow::Grid);
        assert_near(
            header.dimensions.content.width,
            page.dimensions.content.width,
        );
        assert_near(aside.dimensions.content.x, page.dimensions.content.x);
        assert!(main.dimensions.content.x > aside.dimensions.content.x);
        assert!(footer.dimensions.content.y > main.dimensions.content.y);
        Ok(())
    }

    #[test]
    fn grid_hit_regions_for_links_forms_and_images_are_positioned() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("pic.png", 12, 8));
        let tree = layout_html_with_images(
            "<style>.grid { display: grid; grid-template-columns: 80px 80px 80px; gap: 5px; } img { width: 12px; height: 8px; } input { width: 40px; }</style><body><div class=\"grid\"><a href=\"/a\">Link</a><form><input name=\"q\" value=\"v\"></form><img src=\"pic.png\" alt=\"Pic\"></div></body>",
            300.0,
            &images,
        )?;
        let grid = find_box(&tree.root, "div")?;
        let image = find_image_box(grid)?;

        assert_eq!(tree.links.len(), 1);
        assert_eq!(tree.form_controls.len(), 1);
        assert!(tree.links[0].rect.x >= grid.dimensions.content.x);
        assert!(tree.form_controls[0].rect.x > tree.links[0].rect.x);
        assert!(image.dimensions.content.x > tree.form_controls[0].rect.x);
        Ok(())
    }

    #[test]
    fn grid_layout_handles_tiny_viewport_and_dump_is_explicit() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.grid { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; }</style><body><div class=\"grid\"><p>A</p><p>B</p></div></body>",
            1.0,
        )?;
        let dump = dump_layout_tree(&tree);

        assert_eq!(dump, dump_layout_tree(&tree));
        assert!(dump.contains("flow=grid"));
        Ok(())
    }

    #[test]
    fn table_layout_builds_rows_columns_and_distinct_header_cells() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><table><thead><tr><th>Name</th><th>Kind</th></tr></thead><tbody><tr><td>Webby</td><td>Browser</td></tr></tbody></table></body>",
            320.0,
        )?;
        let table = find_box(&tree.root, "table")?;
        let headers = find_boxes(table, "th");
        let cells = find_boxes(table, "td");
        let first_header = headers.first().ok_or_else(|| WebbyError::Layout {
            message: "expected first table header".to_string(),
        })?;
        let second_header = headers.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected second table header".to_string(),
        })?;
        let first_cell = cells.first().ok_or_else(|| WebbyError::Layout {
            message: "expected first body cell".to_string(),
        })?;

        assert_eq!(table.flow, LayoutFlow::Table);
        assert_near(first_header.dimensions.margin_box().width, 152.0);
        assert_near(
            second_header.dimensions.margin_box().x,
            first_header.dimensions.margin_box().x + 152.0,
        );
        assert!(first_header.visuals.background_color != first_cell.visuals.background_color);
        assert!(first_header.dimensions.border.left > 0.0);
        Ok(())
    }

    #[test]
    fn table_dump_is_deterministic_and_exposes_table_flow() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><table><tr><th>A</th><td>B</td></tr></table></body>",
            240.0,
        )?;
        let dump = dump_layout_tree(&tree);

        assert_eq!(dump, dump_layout_tree(&tree));
        assert!(dump.contains("table kind=block"));
        assert!(dump.contains("tr kind=block"));
        assert!(dump.contains("th kind=block"));
        assert!(dump.contains("td kind=block"));
        assert!(dump.contains("flow=table"));
        Ok(())
    }

    #[test]
    fn wide_table_uses_deterministic_columns_without_panicking() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><table><tr><td>One</td><td>Two</td><td>Three</td><td>Four</td></tr></table></body>",
            40.0,
        )?;
        let table = find_box(&tree.root, "table")?;
        let cells = find_boxes(table, "td");

        assert_eq!(cells.len(), 4);
        for pair in cells.windows(2) {
            let first = pair.first().ok_or_else(|| WebbyError::Layout {
                message: "expected first table cell".to_string(),
            })?;
            let second = pair.get(1).ok_or_else(|| WebbyError::Layout {
                message: "expected second table cell".to_string(),
            })?;
            assert!(second.dimensions.margin_box().x >= first.dimensions.margin_box().x);
        }
        Ok(())
    }

    #[test]
    fn list_items_include_deterministic_marker_fragments() -> WebbyResult<()> {
        let tree = layout_html("<body><ul><li>First</li><li>Second</li></ul></body>", 260.0)?;
        let list = find_box(&tree.root, "ul")?;
        let items = find_boxes(list, "li");
        let first = items.first().ok_or_else(|| WebbyError::Layout {
            message: "expected first list item".to_string(),
        })?;

        let text = first
            .text_runs()
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>();
        assert!(text.contains(&"• "));
        assert!(text.contains(&"First"));
        Ok(())
    }

    #[test]
    fn preformatted_text_preserves_spaces_and_line_breaks() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><pre>fn main() {\n  let x = 1;\n}</pre></body>",
            320.0,
        )?;
        let pre = find_box(&tree.root, "pre")?;
        let text = pre
            .text_runs()
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>();

        assert_eq!(text, vec!["fn main() {", "  let x = 1;", "}"]);
        assert!(pre.dimensions.padding.left > 0.0);
        assert!(pre.visuals.background_color.a > 0);
        Ok(())
    }

    #[test]
    fn css_line_height_affects_inline_line_height() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>body { margin: 0; } p { margin: 0; line-height: 30px; }</style><body><p>Line</p></body>",
            320.0,
        )?;
        let paragraph = find_box(&tree.root, "p")?;
        let line = first_line(paragraph)?;

        assert_eq!(line.rect.height, 30.0);
        assert_eq!(paragraph.dimensions.content.height, 30.0);
        Ok(())
    }

    #[test]
    fn text_align_center_and_right_shift_simple_line_boxes() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>
                body { margin: 0; }
                p { margin: 0; width: 200px; }
                .center { text-align: center; }
                .right { text-align: right; }
            </style><body><p class=\"center\">Hi</p><p class=\"right\">Hi</p></body>",
            300.0,
        )?;
        let paragraphs = find_boxes(&tree.root, "p");
        let center = paragraphs.first().ok_or_else(|| WebbyError::Layout {
            message: "expected center paragraph".to_string(),
        })?;
        let right = paragraphs.get(1).ok_or_else(|| WebbyError::Layout {
            message: "expected right paragraph".to_string(),
        })?;
        let center_line = first_line(center)?;
        let right_line = first_line(right)?;

        assert!(center_line.rect.x > center.dimensions.content.x);
        assert!(right_line.rect.x > center_line.rect.x);
        assert_near(
            center_line.rect.x - center.dimensions.content.x,
            (center.dimensions.content.width - center_line.rect.width) / 2.0,
        );
        assert_near(
            right_line.rect.x - right.dimensions.content.x,
            right.dimensions.content.width - right_line.rect.width,
        );
        Ok(())
    }

    #[test]
    fn white_space_nowrap_keeps_collapsed_text_on_one_line() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>body { margin: 0; } p { margin: 0; width: 40px; white-space: nowrap; }</style><body><p>alpha beta gamma</p></body>",
            120.0,
        )?;
        let paragraph = find_box(&tree.root, "p")?;
        let line_count = paragraph
            .contents
            .iter()
            .filter(|item| matches!(item, LayoutItem::LineBox(_)))
            .count();

        assert_eq!(line_count, 1);
        assert_eq!(
            inline_fragment_order(paragraph),
            vec!["text:alpha", "text:beta", "text:gamma"]
        );
        Ok(())
    }

    #[test]
    fn monospace_text_run_width_matches_shared_measurement() -> WebbyResult<()> {
        let tree = layout_html("<body><p><code>iiii</code></p></body>", 320.0)?;
        let paragraph = find_box(&tree.root, "p")?;
        let run = paragraph
            .text_runs()
            .into_iter()
            .find(|run| run.text == "iiii")
            .ok_or_else(|| WebbyError::Layout {
                message: "expected code text run".to_string(),
            })?;
        let expected = webby_text::measure_text_with_family(
            &run.text,
            run.font_size,
            WebbyTextFontWeight::Normal,
            WebbyTextFontFamily::Monospace,
        );

        assert_eq!(run.visuals.font_family, FontFamily::Monospace);
        assert_eq!(run.rect.width, expected.width);
        Ok(())
    }

    #[test]
    fn blockquote_and_hr_have_visible_box_geometry() -> WebbyResult<()> {
        let tree = layout_html("<body><blockquote>Quote</blockquote><hr></body>", 320.0)?;
        let quote = find_box(&tree.root, "blockquote")?;
        let rule = find_box(&tree.root, "hr")?;

        assert!(quote.dimensions.border.left > 0.0);
        assert!(quote.dimensions.padding.left > 0.0);
        assert!(rule.dimensions.border.top > 0.0);
        assert!(rule.dimensions.margin_box().height > 0.0);
        Ok(())
    }

    #[test]
    fn whitespace_is_collapsed_for_layout() -> WebbyResult<()> {
        let tree = layout_html("<body><p>Hello     Webby\nagain</p></body>", 400.0)?;
        let p = find_box(&tree.root, "p")?;
        let text = p
            .text_runs()
            .iter()
            .map(|run| run.text.as_str())
            .collect::<String>();

        assert_eq!(text, "Hello Webby again");
        Ok(())
    }

    #[test]
    fn whitespace_only_nodes_do_not_create_bogus_boxes() -> WebbyResult<()> {
        let tree = layout_html("<body>   <div>Visible</div>  </body>", 400.0)?;
        let body = find_box(&tree.root, "body")?;

        assert!(body.text_runs().is_empty());
        Ok(())
    }

    #[test]
    fn wrapping_drops_trailing_line_space_runs() -> WebbyResult<()> {
        let tree = layout_html("<body><p>one two three four</p></body>", 70.0)?;
        let p = find_box(&tree.root, "p")?;
        let content_right = p.dimensions.content.x + p.dimensions.content.width;

        assert!(
            p.text_runs()
                .iter()
                .all(|run| run.rect.x + run.rect.width <= content_right)
        );
        Ok(())
    }

    #[test]
    fn img_uses_default_placeholder_dimensions() -> WebbyResult<()> {
        let tree = layout_html("<body><img src=\"x.png\"></body>", 500.0)?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 300.0);
        assert_eq!(image.dimensions.content.height, 150.0);
        Ok(())
    }

    #[test]
    fn img_uses_numeric_width_and_height_attributes() -> WebbyResult<()> {
        let tree = layout_html("<body><img width=\"120\" height=\"80\"></body>", 500.0)?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 120.0);
        assert_eq!(image.dimensions.content.height, 80.0);
        Ok(())
    }

    #[test]
    fn img_invalid_dimensions_fall_back_to_defaults() -> WebbyResult<()> {
        let tree = layout_html("<body><img width=\"wide\" height=\"-1\"></body>", 500.0)?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 300.0);
        assert_eq!(image.dimensions.content.height, 150.0);
        Ok(())
    }

    #[test]
    fn decoded_intrinsic_dimensions_affect_layout() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("photo.png", 42, 17));
        let tree = layout_html_with_images("<body><img src=\"photo.png\"></body>", 500.0, &images)?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 42.0);
        assert_eq!(image.dimensions.content.height, 17.0);
        Ok(())
    }

    #[test]
    fn width_height_attributes_override_intrinsic_dimensions() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("photo.png", 42, 17));
        let tree = layout_html_with_images(
            "<body><img src=\"photo.png\" width=\"120\" height=\"80\"></body>",
            500.0,
            &images,
        )?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 120.0);
        assert_eq!(image.dimensions.content.height, 80.0);
        Ok(())
    }

    #[test]
    fn css_width_height_override_intrinsic_dimensions() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("photo.png", 42, 17));
        let tree = layout_html_with_images(
            "<style>img { width: 33px; height: 22px; }</style><body><img src=\"photo.png\"></body>",
            500.0,
            &images,
        )?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 33.0);
        assert_eq!(image.dimensions.content.height, 22.0);
        Ok(())
    }

    #[test]
    fn css_width_height_override_html_width_height() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("photo.png", 42, 17));
        let tree = layout_html_with_images(
            "<style>img { width: 33px; height: 22px; }</style><body><img src=\"photo.png\" width=\"120\" height=\"80\"></body>",
            500.0,
            &images,
        )?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 33.0);
        assert_eq!(image.dimensions.content.height, 22.0);
        Ok(())
    }

    #[test]
    fn css_width_preserves_intrinsic_height_when_height_is_unspecified() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("photo.png", 42, 17));
        let tree = layout_html_with_images(
            "<style>img { width: 33px; }</style><body><img src=\"photo.png\"></body>",
            500.0,
            &images,
        )?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 33.0);
        assert_eq!(image.dimensions.content.height, 17.0);
        Ok(())
    }

    #[test]
    fn html_width_preserves_intrinsic_height_when_height_is_unspecified() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("photo.png", 42, 17));
        let tree = layout_html_with_images(
            "<body><img src=\"photo.png\" width=\"120\"></body>",
            500.0,
            &images,
        )?;
        let image = find_image(&tree.root)?;

        assert_eq!(image.dimensions.content.width, 120.0);
        assert_eq!(image.dimensions.content.height, 17.0);
        Ok(())
    }

    #[test]
    fn failed_image_uses_placeholder_with_alt_text() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><img src=\"missing.png\" alt=\"Missing image\"></body>",
            500.0,
        )?;
        let image = find_image(&tree.root)?;

        assert!(matches!(image.kind, LayoutKind::Image { image: None, .. }));
        assert_eq!(image.text_runs()[0].text, "Missing image");
        Ok(())
    }

    #[test]
    fn svg_width_height_and_shapes_are_preserved_for_rendering() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><svg width=\"40\" height=\"20\"><rect x=\"2\" y=\"3\" width=\"10\" height=\"8\" fill=\"red\"/><line x1=\"0\" y1=\"0\" x2=\"20\" y2=\"10\" stroke=\"#00f\" stroke-width=\"2\"/></svg></body>",
            500.0,
        )?;
        let svg = find_image(&tree.root)?;

        assert_eq!(svg.dimensions.content.width, 40.0);
        assert_eq!(svg.dimensions.content.height, 20.0);
        assert!(matches!(&svg.kind, LayoutKind::Image { graphics, .. } if graphics.len() == 2));
        Ok(())
    }

    #[test]
    fn svg_view_box_scales_commands_into_viewport() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><svg width=\"20\" height=\"10\" viewBox=\"10 5 40 20\"><rect x=\"30\" y=\"15\" width=\"20\" height=\"10\" fill=\"red\"/></svg></body>",
            500.0,
        )?;
        let svg = find_image(&tree.root)?;
        let LayoutKind::Image { graphics, .. } = &svg.kind else {
            return Err(WebbyError::invalid_input("expected svg image layout box"));
        };
        let Some(GraphicCommand::FillRect { rect, .. }) = graphics.first() else {
            return Err(WebbyError::invalid_input("expected svg rect command"));
        };

        assert_eq!(rect.x, 10.0);
        assert_eq!(rect.y, 5.0);
        assert_eq!(rect.width, 10.0);
        assert_eq!(rect.height, 5.0);
        Ok(())
    }

    #[test]
    fn canvas_width_height_and_commands_are_preserved_for_rendering() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><canvas width=\"50\" height=\"25\" data-webby-canvas=\"fillRect,1,2,10,8,green;clearRect,2,3,4,5\"></canvas></body>",
            500.0,
        )?;
        let canvas = find_image(&tree.root)?;

        assert_eq!(canvas.dimensions.content.width, 50.0);
        assert_eq!(canvas.dimensions.content.height, 25.0);
        assert!(matches!(&canvas.kind, LayoutKind::Image { graphics, .. } if graphics.len() == 2));
        Ok(())
    }

    #[test]
    fn audio_and_video_use_deterministic_media_placeholder_boxes() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><audio controls></audio><video width=\"160\" height=\"90\" controls></video></body>",
            500.0,
        )?;
        let dump = dump_layout_tree(&tree);
        let mut media = Vec::new();
        collect_image_boxes(&tree.root, &mut media);

        assert!(dump.contains("audio kind=image-placeholder"));
        assert!(dump.contains("video kind=image-placeholder"));
        assert_eq!(media.len(), 2);
        assert!(
            matches!(&media[1].kind, LayoutKind::Image { tag_name, .. } if tag_name == "video")
        );
        assert_eq!(media[1].dimensions.content.width, 160.0);
        assert_eq!(media[1].dimensions.content.height, 90.0);
        Ok(())
    }

    #[test]
    fn video_poster_image_affects_layout_like_decoded_image() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("poster.png", 42, 17));
        let tree = layout_html_with_images(
            "<body><video poster=\"poster.png\"></video></body>",
            500.0,
            &images,
        )?;
        let video = find_image(&tree.root)?;

        assert_eq!(video.dimensions.content.width, 42.0);
        assert_eq!(video.dimensions.content.height, 17.0);
        assert!(
            matches!(&video.kind, LayoutKind::Image { tag_name, image: Some(_), .. } if tag_name == "video")
        );
        Ok(())
    }

    #[test]
    fn iframe_uses_replaced_box_dimensions_and_loaded_surface_image() -> WebbyResult<()> {
        let mut images = ImageMap::empty();
        images.insert(test_image("child.html", 90, 40));
        let tree = layout_html_with_images(
            "<body><iframe src=\"child.html\" width=\"120\" height=\"70\" name=\"child\"></iframe></body>",
            320.0,
            &images,
        )?;
        let iframe = find_image(&tree.root)?;

        assert_eq!(iframe.dimensions.content.width, 120.0);
        assert_eq!(iframe.dimensions.content.height, 70.0);
        assert!(
            matches!(&iframe.kind, LayoutKind::Image { tag_name, src: Some(src), image: Some(_), .. } if tag_name == "iframe" && src == "child.html")
        );
        Ok(())
    }

    #[test]
    fn form_controls_expose_layout_hit_regions_and_metadata() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><form action=\"/find\" method=\"get\"><input type=\"search\" name=\"q\" value=\"rust\"><input type=\"submit\" name=\"go\" value=\"Search\"></form></body>",
            500.0,
        )?;

        assert_eq!(tree.form_controls.len(), 2);
        let text = &tree.form_controls[0];
        assert_eq!(text.control_type, FormControlType::Search);
        assert_eq!(text.name.as_deref(), Some("q"));
        assert_eq!(text.value, "rust");
        assert_eq!(
            text.form.as_ref().and_then(|form| form.action.as_deref()),
            Some("/find")
        );
        assert_eq!(
            text.form.as_ref().and_then(|form| form.method.as_deref()),
            Some("get")
        );
        assert!(text.rect.width > 0.0);
        assert!(text.rect.height > 0.0);

        let submit = &tree.form_controls[1];
        assert_eq!(submit.control_type, FormControlType::Submit);
        assert_eq!(submit.name.as_deref(), Some("go"));
        assert_eq!(submit.value, "Search");
        assert_eq!(
            submit.form.as_ref().map(|form| form.id),
            text.form.as_ref().map(|form| form.id)
        );
        Ok(())
    }

    #[test]
    fn richer_form_controls_preserve_state_and_options() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><form><input type=\"checkbox\" name=\"ok\" checked><input type=\"radio\" name=\"mode\" value=\"a\"><input type=\"password\" name=\"pw\" value=\"secret\"><input type=\"email\" name=\"mail\" value=\"a@example.test\"><select name=\"choice\"><option value=\"one\">One</option><option value=\"two\" selected>Two</option></select><textarea name=\"note\">Hello</textarea><button type=\"reset\">Reset</button><input type=\"text\" name=\"disabled\" disabled></form></body>",
            800.0,
        )?;

        assert!(
            tree.form_controls
                .iter()
                .any(|control| control.control_type == FormControlType::Checkbox
                    && control.checked
                    && control.value == "on")
        );
        assert!(
            tree.form_controls
                .iter()
                .any(|control| control.control_type == FormControlType::Password
                    && control.value == "secret")
        );
        let select = tree
            .form_controls
            .iter()
            .find(|control| control.control_type == FormControlType::Select)
            .ok_or_else(|| WebbyError::invalid_input("missing select"))?;
        assert_eq!(select.value, "two");
        assert_eq!(select.options.len(), 2);
        assert!(tree.form_controls.iter().any(|control| {
            control.control_type == FormControlType::Textarea && control.value == "Hello"
        }));
        assert!(tree.form_controls.iter().any(|control| {
            control.control_type == FormControlType::Reset && control.value == "Reset"
        }));
        assert!(find_disabled_form_control(&tree.root));
        Ok(())
    }

    #[test]
    fn block_styled_form_controls_keep_hit_regions() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>input { display: block; }</style><body><form><input type=\"email\" name=\"email\"><input type=\"password\" name=\"password\"><button type=\"submit\">Sign in</button></form></body>",
            360.0,
        )?;

        assert_eq!(tree.form_controls.len(), 3);
        assert_eq!(tree.form_controls[0].control_type, FormControlType::Email);
        assert_eq!(
            tree.form_controls[1].control_type,
            FormControlType::Password
        );
        assert_eq!(tree.form_controls[2].control_type, FormControlType::Submit);
        assert!(tree.form_controls[1].rect.y > tree.form_controls[0].rect.y);
        Ok(())
    }

    #[test]
    fn form_control_dump_is_deterministic_and_clear() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><form><input type=\"text\" name=\"q\" placeholder=\"Search\"><button type=\"submit\" name=\"go\" value=\"1\">Go</button></form></body>",
            500.0,
        )?;
        let dump = dump_layout_tree(&tree);

        assert_eq!(dump, dump_layout_tree(&tree));
        assert!(dump.contains("input kind=form-control"));
        assert!(dump.contains("control-type=text"));
        assert!(dump.contains("name=\"q\""));
        assert!(dump.contains("form-controls"));
        assert!(dump.contains("type=submit"));
        Ok(())
    }

    #[test]
    fn layout_dump_exposes_inspection_metadata() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p id=\"intro\" class=\"card highlighted\">Inspect me</p></body>",
            260.0,
        )?;
        let dump = dump_layout_tree(&tree);

        assert!(dump.contains("p id=\"intro\" class=\"card highlighted\" kind=block"));
        Ok(())
    }

    #[test]
    fn overflow_hidden_clips_descendant_link_hit_regions() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.clip { height: 20px; overflow: hidden; } .spacer { height: 80px; }</style><body><div class=\"clip\"><div class=\"spacer\"></div><a href=\"/hidden\">Hidden</a></div></body>",
            260.0,
        )?;

        assert_eq!(tree.scroll_containers().len(), 1);
        assert_eq!(tree.links.len(), 1);
        assert!(tree.visible_links(&super::ScrollOffsets::new()).is_empty());
        Ok(())
    }

    #[test]
    fn overflow_auto_projects_scrolled_link_into_visible_viewport() -> WebbyResult<()> {
        let tree = layout_html(
            "<style>.clip { height: 20px; overflow-y: auto; } .spacer { height: 80px; }</style><body><div class=\"clip\"><div class=\"spacer\"></div><a href=\"/shown\">Shown</a></div></body>",
            260.0,
        )?;
        let container = tree
            .scroll_containers()
            .first()
            .copied()
            .ok_or_else(|| WebbyError::invalid_input("missing scroll container"))?;
        let mut offsets = super::ScrollOffsets::new();
        offsets.insert(
            container.id,
            super::ScrollOffset {
                x: 0.0,
                y: container.max_scroll_y,
            },
        );

        let visible = tree.visible_links(&offsets);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].href, "/shown");
        assert!(visible[0].rect.y >= container.viewport.y);
        assert!(visible[0].rect.y < container.viewport.y + container.viewport.height);
        Ok(())
    }

    #[test]
    fn mixed_inline_text_span_link_image_and_br_preserve_source_order() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p>alpha <span>beta</span> <a href=\"/g\">gamma</a><img width=\"12\" height=\"8\"> delta<br>omega</p></body>",
            500.0,
        )?;
        let p = find_box(&tree.root, "p")?;
        let order = inline_fragment_order(p);

        assert_eq!(
            order,
            vec![
                "text:alpha",
                "text: ",
                "text:beta",
                "text: ",
                "text:gamma",
                "image",
                "text: ",
                "text:delta",
                "text:omega"
            ]
        );
        Ok(())
    }

    #[test]
    fn inline_image_aligns_predictably_with_text_line_box() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p>before <img width=\"20\" height=\"30\"> after</p></body>",
            500.0,
        )?;
        let p = find_box(&tree.root, "p")?;
        let line = first_line(p)?;
        let image = find_image(p)?;

        assert!(line.rect.height >= image.dimensions.margin_box().height);
        assert_near(
            image.dimensions.margin_box().y + image.dimensions.margin_box().height,
            line.rect.y + line.baseline,
        );
        Ok(())
    }

    #[test]
    fn nested_span_inside_link_remains_clickable() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p><a href=\"/nested\">outer <span>inner text</span></a></p></body>",
            500.0,
        )?;

        assert!(tree.links.iter().all(|hit| hit.href == "/nested"));
        assert!(tree.links.iter().any(|hit| hit.rect.width > 0.0));
        assert!(
            find_box(&tree.root, "p")?
                .text_runs()
                .iter()
                .all(|run| { run.visuals.href.as_deref() == Some("/nested") })
        );
        Ok(())
    }

    #[test]
    fn link_containing_inline_image_has_hit_region() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p><a href=\"/image\"><img width=\"20\" height=\"10\" alt=\"icon\"></a></p></body>",
            500.0,
        )?;

        assert_eq!(tree.links.len(), 1);
        assert_eq!(tree.links[0].href, "/image");
        assert!(tree.links[0].rect.width >= 20.0);
        assert!(tree.links[0].rect.height >= 10.0);
        Ok(())
    }

    #[test]
    fn tiny_viewport_width_does_not_panic() -> WebbyResult<()> {
        let tree = layout_html("<body><p>supercalifragilistic</p></body>", 1.0)?;
        let p = find_box(&tree.root, "p")?;

        assert_eq!(p.text_runs()[0].rect.width, 0.0);
        Ok(())
    }

    #[test]
    fn deterministic_layout_dump_contains_core_geometry() -> WebbyResult<()> {
        let tree = layout_html("<body><p>Hello <a href=\"/x\">link</a></p></body>", 400.0)?;
        let dump = dump_layout_tree(&tree);

        assert_eq!(dump, dump_layout_tree(&tree));
        assert!(dump.contains("layout scroll-height="));
        assert!(dump.contains("p kind=block"));
        assert!(dump.contains("text \"Hello\""));
        assert!(dump.contains("href=\"/x\""));
        Ok(())
    }

    #[test]
    fn link_rectangles_are_preserved() -> WebbyResult<()> {
        let tree = layout_html("<body><p><a href=\"/docs\">Docs</a></p></body>", 400.0)?;

        assert_eq!(tree.links.len(), 1);
        assert_eq!(tree.links[0].href, "/docs");
        assert_eq!(
            tree.links[0].rect.width,
            measure_text("Docs", 16.0, FontWeight::Normal, FontFamily::Sans)
        );
        Ok(())
    }

    #[test]
    fn linked_text_exposes_href_through_text_visuals() -> WebbyResult<()> {
        let tree = layout_html("<body><p><a href=\"/docs\">Docs</a></p></body>", 400.0)?;
        let p = find_box(&tree.root, "p")?;

        assert!(p.text_runs()[0].visuals.is_link);
        assert_eq!(p.text_runs()[0].visuals.href.as_deref(), Some("/docs"));
        assert_eq!(tree.links[0].href, "/docs");
        Ok(())
    }

    #[test]
    fn multi_word_link_hit_rectangles_include_spaces() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p><a href=\"/docs\">Webby docs</a></p></body>",
            400.0,
        )?;

        assert_eq!(tree.links.len(), 3);
        assert!(tree.links.iter().all(|link| link.href == "/docs"));
        assert!(tree.links.iter().any(|link| {
            link.rect.width == measure_text(" ", 16.0, FontWeight::Normal, FontFamily::Sans)
        }));
        Ok(())
    }

    #[test]
    fn wrapped_link_hit_rectangles_keep_href() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p><a href=\"/docs\">alpha beta gamma</a></p></body>",
            70.0,
        )?;
        let first_y = tree.links[0].rect.y;

        assert!(tree.links.iter().all(|link| link.href == "/docs"));
        assert!(tree.links.iter().any(|link| link.rect.y > first_y));
        Ok(())
    }

    #[test]
    fn multi_line_link_has_clickable_regions_on_each_line() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p><a href=\"/docs\">alpha beta gamma delta epsilon</a></p></body>",
            80.0,
        )?;
        let rows = distinct_link_y_count(&tree.links);

        assert!(rows > 1);
        assert!(tree.links.iter().all(|link| link.href == "/docs"));
        Ok(())
    }

    #[test]
    fn wrapped_inline_content_preserves_source_order() -> WebbyResult<()> {
        let tree = layout_html(
            "<body><p>alpha <span>beta</span> gamma delta</p></body>",
            80.0,
        )?;
        let order = inline_fragment_order(find_box(&tree.root, "p")?);

        assert_eq!(
            order,
            vec![
                "text:alpha",
                "text: ",
                "text:beta",
                "text: ",
                "text:gamma",
                "text: ",
                "text:delta"
            ]
        );
        Ok(())
    }

    #[test]
    fn layout_dump_exposes_inline_line_fragments() -> WebbyResult<()> {
        let tree = layout_html("<body><p>Hello <span>inline</span></p></body>", 400.0)?;
        let dump = dump_layout_tree(&tree);

        assert!(dump.contains("line rect="));
        assert!(dump.contains("baseline="));
        assert!(dump.contains("text \"Hello\""));
        assert!(dump.contains("text \"inline\""));
        Ok(())
    }

    fn layout_html(html: &str, width: f32) -> WebbyResult<super::LayoutTree> {
        let document = parse_document(html)?;
        let styled = style_document(&document);
        layout_tree(&styled, Viewport::with_width(width)?)
    }

    fn layout_html_with_images(
        html: &str,
        width: f32,
        images: &ImageMap,
    ) -> WebbyResult<super::LayoutTree> {
        let document = parse_document(html)?;
        let styled = style_document(&document);
        layout_tree_with_images(&styled, Viewport::with_width(width)?, images)
    }

    fn test_image(src: &str, width: u32, height: u32) -> ImageResource {
        ImageResource {
            src: src.to_string(),
            final_url: format!("https://example.test/{src}"),
            image: DecodedImage {
                width,
                height,
                pixels: vec![255; (width as usize) * (height as usize) * 4],
            },
        }
    }

    fn find_box<'a>(
        layout_box: &'a super::LayoutBox,
        tag_name: &str,
    ) -> WebbyResult<&'a super::LayoutBox> {
        if matches!(&layout_box.kind, LayoutKind::Block { tag_name: current, .. } if current == tag_name)
        {
            return Ok(layout_box);
        }

        for child in layout_box.children() {
            if let Ok(found) = find_box(child, tag_name) {
                return Ok(found);
            }
        }

        Err(WebbyError::Layout {
            message: format!("expected layout box for {tag_name}"),
        })
    }

    fn find_boxes<'a>(
        layout_box: &'a super::LayoutBox,
        tag_name: &str,
    ) -> Vec<&'a super::LayoutBox> {
        let mut matches = Vec::new();
        collect_boxes(layout_box, tag_name, &mut matches);
        matches
    }

    fn find_image_box(layout_box: &super::LayoutBox) -> WebbyResult<&super::LayoutBox> {
        if matches!(layout_box.kind, LayoutKind::Image { .. }) {
            return Ok(layout_box);
        }

        for child in layout_box.children() {
            if let Ok(found) = find_image_box(child) {
                return Ok(found);
            }
        }

        Err(WebbyError::Layout {
            message: "expected image layout box".to_string(),
        })
    }

    fn find_disabled_form_control(layout_box: &super::LayoutBox) -> bool {
        if matches!(
            &layout_box.kind,
            LayoutKind::FormControl { disabled: true, .. }
        ) {
            return true;
        }

        layout_box
            .children()
            .iter()
            .any(|child| find_disabled_form_control(child))
    }

    fn distinct_text_run_y_count(layout_box: &super::LayoutBox) -> usize {
        let mut rows = Vec::new();
        for run in layout_box.text_runs() {
            let y = (run.rect.y * 10.0).round() as i32;
            if !rows.contains(&y) {
                rows.push(y);
            }
        }
        rows.len()
    }

    fn distinct_link_y_count(links: &[super::LinkHitBox]) -> usize {
        let mut rows = Vec::new();
        for link in links {
            let y = (link.rect.y * 10.0).round() as i32;
            if !rows.contains(&y) {
                rows.push(y);
            }
        }
        rows.len()
    }

    fn assert_second_run_is_lower(layout_box: &super::LayoutBox) {
        let runs = layout_box.text_runs();
        assert!(runs.len() >= 2);
        assert!(runs[1].rect.y > runs[0].rect.y);
    }

    fn first_line(layout_box: &super::LayoutBox) -> WebbyResult<&super::LineBox> {
        layout_box
            .contents
            .iter()
            .find_map(|item| match item {
                LayoutItem::LineBox(line) => Some(line),
                LayoutItem::Text(_) | LayoutItem::Box(_) => None,
            })
            .ok_or_else(|| WebbyError::Layout {
                message: "expected inline line box".to_string(),
            })
    }

    fn inline_fragment_order(layout_box: &super::LayoutBox) -> Vec<String> {
        let mut order = Vec::new();
        for item in &layout_box.contents {
            match item {
                LayoutItem::LineBox(line) => {
                    for fragment in &line.fragments {
                        match fragment {
                            InlineFragment::Text(run) => order.push(format!("text:{}", run.text)),
                            InlineFragment::Box(_) => order.push("image".to_string()),
                        }
                    }
                }
                LayoutItem::Text(run) => order.push(format!("text:{}", run.text)),
                LayoutItem::Box(_) => order.push("image".to_string()),
            }
        }
        order
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {actual} to be near {expected}"
        );
    }

    fn collect_boxes<'a>(
        layout_box: &'a super::LayoutBox,
        tag_name: &str,
        matches: &mut Vec<&'a super::LayoutBox>,
    ) {
        if matches!(&layout_box.kind, LayoutKind::Block { tag_name: current, .. } if current == tag_name)
        {
            matches.push(layout_box);
        }

        for child in layout_box.children() {
            collect_boxes(child, tag_name, matches);
        }
    }

    fn find_image(layout_box: &super::LayoutBox) -> WebbyResult<&super::LayoutBox> {
        if matches!(layout_box.kind, LayoutKind::Image { .. }) {
            return Ok(layout_box);
        }

        for child in layout_box.children() {
            if let Ok(found) = find_image(child) {
                return Ok(found);
            }
        }

        Err(WebbyError::Layout {
            message: "expected image".to_string(),
        })
    }

    fn collect_image_boxes<'a>(
        layout_box: &'a super::LayoutBox,
        matches: &mut Vec<&'a super::LayoutBox>,
    ) {
        if matches!(layout_box.kind, LayoutKind::Image { .. }) {
            matches.push(layout_box);
        }

        for child in layout_box.children() {
            collect_image_boxes(child, matches);
        }
    }

    fn apply_styled_tag_mut<'a>(
        node: &mut StyledNode<'a>,
        tag_name: &str,
        update: &mut impl FnMut(&mut StyledNode<'a>),
    ) -> bool {
        if node.tag_name() == Some(tag_name) {
            update(node);
            return true;
        }

        for child in &mut node.children {
            if apply_styled_tag_mut(child, tag_name, update) {
                return true;
            }
        }

        false
    }
}
