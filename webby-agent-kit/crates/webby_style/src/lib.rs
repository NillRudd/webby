//! Default user-agent style tree for Webby's DOM-to-layout pipeline.

use std::collections::BTreeMap;

use webby_css::{
    Combinator, CssWideKeyword, Declaration, GridPlacement as CssGridPlacement,
    GridTrack as CssGridTrack, MediaFeature, MediaQuery, MediaType, Orientation, Property,
    PseudoClass, Selector, SelectorPart, Size, Specificity, Stylesheet,
    TransitionProperty as CssTransitionProperty,
    TransitionTimingFunction as CssTransitionTimingFunction, Value,
};
use webby_dom::{Document, ElementData, Node, NodeKind};

/// CSS-like display classification used by layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// Stacks vertically.
    Block,
    /// Participates in inline text flow.
    Inline,
    /// Participates in inline flow with a box-like formatting context.
    InlineBlock,
    /// Flex container.
    Flex,
    /// Grid container.
    Grid,
    /// Represents a forced line break.
    LineBreak,
    /// Not displayed.
    None,
}

impl Display {
    fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Inline => "inline",
            Self::InlineBlock => "inline-block",
            Self::Flex => "flex",
            Self::Grid => "grid",
            Self::LineBreak => "line-break",
            Self::None => "none",
        }
    }
}

impl From<webby_css::Display> for Display {
    fn from(value: webby_css::Display) -> Self {
        match value {
            webby_css::Display::Block => Self::Block,
            webby_css::Display::Inline => Self::Inline,
            webby_css::Display::InlineBlock => Self::InlineBlock,
            webby_css::Display::Flex => Self::Flex,
            webby_css::Display::Grid => Self::Grid,
            webby_css::Display::None => Self::None,
        }
    }
}

/// Flex main-axis direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    /// Left-to-right row.
    Row,
    /// Top-to-bottom column.
    Column,
}

impl From<webby_css::FlexDirection> for FlexDirection {
    fn from(value: webby_css::FlexDirection) -> Self {
        match value {
            webby_css::FlexDirection::Row => Self::Row,
            webby_css::FlexDirection::Column => Self::Column,
        }
    }
}

impl FlexDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Column => "column",
        }
    }
}

/// Flex main-axis alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    /// Start of main axis.
    FlexStart,
    /// Centered on main axis.
    Center,
    /// Remaining space distributed between items.
    SpaceBetween,
}

impl From<webby_css::JustifyContent> for JustifyContent {
    fn from(value: webby_css::JustifyContent) -> Self {
        match value {
            webby_css::JustifyContent::FlexStart => Self::FlexStart,
            webby_css::JustifyContent::Center => Self::Center,
            webby_css::JustifyContent::SpaceBetween => Self::SpaceBetween,
        }
    }
}

impl JustifyContent {
    fn as_str(self) -> &'static str {
        match self {
            Self::FlexStart => "flex-start",
            Self::Center => "center",
            Self::SpaceBetween => "space-between",
        }
    }
}

/// Flex cross-axis alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    /// Stretch items on cross axis.
    Stretch,
    /// Center items on cross axis.
    Center,
    /// Start of cross axis.
    FlexStart,
}

impl From<webby_css::AlignItems> for AlignItems {
    fn from(value: webby_css::AlignItems) -> Self {
        match value {
            webby_css::AlignItems::Stretch => Self::Stretch,
            webby_css::AlignItems::Center => Self::Center,
            webby_css::AlignItems::FlexStart => Self::FlexStart,
        }
    }
}

impl AlignItems {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stretch => "stretch",
            Self::Center => "center",
            Self::FlexStart => "flex-start",
        }
    }
}

/// Visibility classification used by layout/render/hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Visible and interactive.
    Visible,
    /// Keeps layout but does not paint or interact.
    Hidden,
}

/// CSS overflow behavior for one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Content may paint outside the box.
    Visible,
    /// Content is clipped without scrolling.
    Hidden,
    /// Content is clipped and scrollable.
    Scroll,
    /// Content is clipped and scrollable when needed.
    Auto,
}

impl Overflow {
    fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Scroll => "scroll",
            Self::Auto => "auto",
        }
    }
}

impl From<webby_css::Overflow> for Overflow {
    fn from(value: webby_css::Overflow) -> Self {
        match value {
            webby_css::Overflow::Visible => Self::Visible,
            webby_css::Overflow::Hidden => Self::Hidden,
            webby_css::Overflow::Scroll => Self::Scroll,
            webby_css::Overflow::Auto => Self::Auto,
        }
    }
}

impl Visibility {
    fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
        }
    }
}

impl From<webby_css::Visibility> for Visibility {
    fn from(value: webby_css::Visibility) -> Self {
        match value {
            webby_css::Visibility::Visible => Self::Visible,
            webby_css::Visibility::Hidden => Self::Hidden,
        }
    }
}

/// CSS positioning mode consumed by layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Normal document flow.
    Static,
    /// Normal flow with a visual offset.
    Relative,
    /// Removed from normal flow and positioned against a containing block.
    Absolute,
}

impl Position {
    fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Relative => "relative",
            Self::Absolute => "absolute",
        }
    }
}

impl From<webby_css::Position> for Position {
    fn from(value: webby_css::Position) -> Self {
        match value {
            webby_css::Position::Static => Self::Static,
            webby_css::Position::Relative => Self::Relative,
            webby_css::Position::Absolute => Self::Absolute,
        }
    }
}

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
    /// Transparent color.
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Opaque black.
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    /// Opaque white.
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Browser default link blue.
    pub const LINK_BLUE: Self = Self {
        r: 0,
        g: 0,
        b: 238,
        a: 255,
    };

    fn as_rgba(self) -> String {
        format!("rgba({},{},{},{})", self.r, self.g, self.b, self.a)
    }
}

impl From<webby_css::Color> for Color {
    fn from(value: webby_css::Color) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
            a: value.a,
        }
    }
}

/// Font weight for text rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// Normal text weight.
    Normal,
    /// Bold text weight.
    Bold,
}

impl FontWeight {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Bold => "bold",
        }
    }
}

impl From<webby_css::FontWeight> for FontWeight {
    fn from(value: webby_css::FontWeight) -> Self {
        match value {
            webby_css::FontWeight::Normal => Self::Normal,
            webby_css::FontWeight::Bold => Self::Bold,
        }
    }
}

/// Font family category for text measurement/rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    /// Default sans/proportional family.
    Sans,
    /// Deterministic monospace family.
    Monospace,
}

impl FontFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sans => "sans",
            Self::Monospace => "monospace",
        }
    }
}

impl From<webby_css::FontFamily> for FontFamily {
    fn from(value: webby_css::FontFamily) -> Self {
        match value {
            webby_css::FontFamily::Sans => Self::Sans,
            webby_css::FontFamily::Monospace => Self::Monospace,
        }
    }
}

/// Text decoration for text rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecoration {
    /// No decoration.
    None,
    /// Underline decoration.
    Underline,
}

impl TextDecoration {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Underline => "underline",
        }
    }
}

impl From<webby_css::TextDecoration> for TextDecoration {
    fn from(value: webby_css::TextDecoration) -> Self {
        match value {
            webby_css::TextDecoration::None => Self::None,
            webby_css::TextDecoration::Underline => Self::Underline,
        }
    }
}

/// Text alignment inside block containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    /// Left/start aligned.
    Left,
    /// Center aligned.
    Center,
    /// Right/end aligned.
    Right,
}

impl TextAlign {
    fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

impl From<webby_css::TextAlign> for TextAlign {
    fn from(value: webby_css::TextAlign) -> Self {
        match value {
            webby_css::TextAlign::Left => Self::Left,
            webby_css::TextAlign::Center => Self::Center,
            webby_css::TextAlign::Right => Self::Right,
        }
    }
}

/// White-space handling for inline text collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    /// Collapse whitespace and wrap normally.
    Normal,
    /// Preserve spaces and line breaks.
    Pre,
    /// Collapse whitespace but avoid wrapping.
    NoWrap,
}

impl WhiteSpace {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Pre => "pre",
            Self::NoWrap => "nowrap",
        }
    }
}

impl From<webby_css::WhiteSpace> for WhiteSpace {
    fn from(value: webby_css::WhiteSpace) -> Self {
        match value {
            webby_css::WhiteSpace::Normal => Self::Normal,
            webby_css::WhiteSpace::Pre => Self::Pre,
            webby_css::WhiteSpace::NoWrap => Self::NoWrap,
        }
    }
}

/// Four-sided edge sizes in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Left edge.
    pub left: f32,
}

impl Edges {
    /// Zero on every side.
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    /// Creates edge sizes in top/right/bottom/left order.
    pub const fn trbl(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    fn as_debug_value(self) -> String {
        format!(
            "{:.1}/{:.1}/{:.1}/{:.1}",
            self.top, self.right, self.bottom, self.left
        )
    }
}

impl From<webby_css::Edges> for Edges {
    fn from(value: webby_css::Edges) -> Self {
        Self {
            top: value.top,
            right: value.right,
            bottom: value.bottom,
            left: value.left,
        }
    }
}

/// Border style for future layout/rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    /// Border width in CSS pixels.
    pub width: f32,
    /// Border color.
    pub color: Color,
}

/// CSS box-sizing behavior used by layout sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSizing {
    /// Width and height describe the content box.
    ContentBox,
    /// Width and height describe the border box.
    BorderBox,
}

impl BoxSizing {
    fn as_str(self) -> &'static str {
        match self {
            Self::ContentBox => "content-box",
            Self::BorderBox => "border-box",
        }
    }
}

impl From<webby_css::BoxSizing> for BoxSizing {
    fn from(value: webby_css::BoxSizing) -> Self {
        match value {
            webby_css::BoxSizing::ContentBox => Self::ContentBox,
            webby_css::BoxSizing::BorderBox => Self::BorderBox,
        }
    }
}

/// CSS sizing value resolved by layout when containing block dimensions are known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CssSize {
    /// Automatic size.
    Auto,
    /// Fixed CSS pixels.
    Px(f32),
    /// Percentage, stored as a 0.0-1.0 ratio.
    Percent(f32),
}

impl CssSize {
    fn as_debug_value(self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Px(px) => format!("{px:.1}px"),
            Self::Percent(percent) => format!("{:.1}%", percent * 100.0),
        }
    }
}

impl From<Size> for CssSize {
    fn from(value: Size) -> Self {
        match value {
            Size::Auto => Self::Auto,
            Size::Px(px) => Self::Px(px),
            Size::Percent(percent) => Self::Percent(percent),
        }
    }
}

/// Computed CSS grid track sizing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrack {
    /// Fixed CSS pixels.
    Px(f32),
    /// Percentage of the grid container content size.
    Percent(f32),
    /// Fraction of remaining space.
    Fr(f32),
    /// Automatic track sized from its items.
    Auto,
}

impl GridTrack {
    fn as_debug_value(self) -> String {
        match self {
            Self::Px(px) => format!("{px:.1}px"),
            Self::Percent(percent) => format!("{:.1}%", percent * 100.0),
            Self::Fr(fr) => format!("{fr:.1}fr"),
            Self::Auto => "auto".to_string(),
        }
    }
}

impl From<CssGridTrack> for GridTrack {
    fn from(value: CssGridTrack) -> Self {
        match value {
            CssGridTrack::Px(px) => Self::Px(px),
            CssGridTrack::Percent(percent) => Self::Percent(percent),
            CssGridTrack::Fr(fr) => Self::Fr(fr),
            CssGridTrack::Auto => Self::Auto,
        }
    }
}

/// Computed grid line placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridPlacement {
    /// 1-based start grid line.
    pub start: Option<usize>,
    /// 1-based end grid line.
    pub end: Option<usize>,
}

/// Supported transition target properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionProperty {
    /// `all`
    All,
    /// Text color.
    Color,
    /// Background color.
    BackgroundColor,
    /// `left`
    Left,
    /// `top`
    Top,
}

impl From<CssTransitionProperty> for TransitionProperty {
    fn from(value: CssTransitionProperty) -> Self {
        match value {
            CssTransitionProperty::All => Self::All,
            CssTransitionProperty::Color => Self::Color,
            CssTransitionProperty::BackgroundColor => Self::BackgroundColor,
            CssTransitionProperty::Left => Self::Left,
            CssTransitionProperty::Top => Self::Top,
        }
    }
}

/// Supported transition timing functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionTimingFunction {
    /// Linear interpolation.
    Linear,
    /// Deterministic ease interpolation.
    Ease,
}

impl From<CssTransitionTimingFunction> for TransitionTimingFunction {
    fn from(value: CssTransitionTimingFunction) -> Self {
        match value {
            CssTransitionTimingFunction::Linear => Self::Linear,
            CssTransitionTimingFunction::Ease => Self::Ease,
        }
    }
}

/// Computed transition metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    /// Properties eligible for transition.
    pub properties: Vec<TransitionProperty>,
    /// Duration in milliseconds.
    pub duration_ms: u32,
    /// Delay in milliseconds.
    pub delay_ms: u32,
    /// Timing function.
    pub timing_function: TransitionTimingFunction,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            properties: Vec::new(),
            duration_ms: 0,
            delay_ms: 0,
            timing_function: TransitionTimingFunction::Ease,
        }
    }
}

impl GridPlacement {
    /// Returns true when no explicit line was supplied.
    pub fn is_auto(self) -> bool {
        self.start.is_none() && self.end.is_none()
    }

    fn as_debug_value(self) -> String {
        match (self.start, self.end) {
            (None, None) => "auto".to_string(),
            (Some(start), None) => start.to_string(),
            (Some(start), Some(end)) => format!("{start}/{end}"),
            (None, Some(end)) => format!("auto/{end}"),
        }
    }
}

impl From<CssGridPlacement> for GridPlacement {
    fn from(value: CssGridPlacement) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

impl Default for Border {
    fn default() -> Self {
        Self {
            width: 0.0,
            color: Color::TRANSPARENT,
        }
    }
}

/// Computed style values consumed by layout and rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    /// Display behavior.
    pub display: Display,
    /// Visibility behavior.
    pub visibility: Visibility,
    /// Horizontal overflow behavior.
    pub overflow_x: Overflow,
    /// Vertical overflow behavior.
    pub overflow_y: Overflow,
    /// CSS positioning mode.
    pub position: Position,
    /// Optional top offset.
    pub top: Option<CssSize>,
    /// Optional right offset.
    pub right: Option<CssSize>,
    /// Optional bottom offset.
    pub bottom: Option<CssSize>,
    /// Optional left offset.
    pub left: Option<CssSize>,
    /// Text color.
    pub color: Color,
    /// Background color.
    pub background_color: Color,
    /// Font size in CSS pixels.
    pub font_size: f32,
    /// Optional resolved line-height in CSS pixels.
    pub line_height: Option<f32>,
    /// Font family category.
    pub font_family: FontFamily,
    /// Font weight.
    pub font_weight: FontWeight,
    /// Text decoration.
    pub text_decoration: TextDecoration,
    /// Text alignment for inline content.
    pub text_align: TextAlign,
    /// White-space behavior.
    pub white_space: WhiteSpace,
    /// Outer spacing.
    pub margin: Edges,
    /// Inner spacing.
    pub padding: Edges,
    /// Border.
    pub border: Border,
    /// Optional preferred content width in CSS pixels.
    pub width: Option<CssSize>,
    /// Optional preferred content height in CSS pixels.
    pub height: Option<CssSize>,
    /// Optional minimum width.
    pub min_width: Option<CssSize>,
    /// Optional maximum width.
    pub max_width: Option<CssSize>,
    /// Optional minimum height.
    pub min_height: Option<CssSize>,
    /// Optional maximum height.
    pub max_height: Option<CssSize>,
    /// Box sizing mode for explicit width/height.
    pub box_sizing: BoxSizing,
    /// Flex main-axis direction for flex containers.
    pub flex_direction: FlexDirection,
    /// Flex item/container gap in CSS pixels.
    pub gap: f32,
    /// Row gap for grid containers.
    pub row_gap: f32,
    /// Column gap for grid containers.
    pub column_gap: f32,
    /// Explicit grid columns.
    pub grid_template_columns: Vec<GridTrack>,
    /// Explicit grid rows.
    pub grid_template_rows: Vec<GridTrack>,
    /// Grid column placement for grid items.
    pub grid_column: GridPlacement,
    /// Grid row placement for grid items.
    pub grid_row: GridPlacement,
    /// Flex main-axis alignment.
    pub justify_content: JustifyContent,
    /// Flex cross-axis alignment.
    pub align_items: AlignItems,
    /// Flex item grow factor.
    pub flex_grow: f32,
    /// Transition metadata for app-owned animation timing.
    pub transition: Transition,
    /// Whether the node should be treated as a link.
    pub is_link: bool,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            visibility: Visibility::Visible,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            position: Position::Static,
            top: None,
            right: None,
            bottom: None,
            left: None,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            font_size: 16.0,
            line_height: None,
            font_family: FontFamily::Sans,
            font_weight: FontWeight::Normal,
            text_decoration: TextDecoration::None,
            text_align: TextAlign::Left,
            white_space: WhiteSpace::Normal,
            margin: Edges::ZERO,
            padding: Edges::ZERO,
            border: Border::default(),
            width: None,
            height: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            box_sizing: BoxSizing::ContentBox,
            flex_direction: FlexDirection::Row,
            gap: 0.0,
            row_gap: 0.0,
            column_gap: 0.0,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_column: GridPlacement::default(),
            grid_row: GridPlacement::default(),
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            flex_grow: 0.0,
            transition: Transition::default(),
            is_link: false,
        }
    }
}

/// Styled tree node. It references DOM nodes and owns computed child styles.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledNode<'a> {
    /// Source DOM node.
    pub node: &'a Node,
    /// Computed style for this node.
    pub style: ComputedStyle,
    /// Styled children in DOM order, excluding hidden DOM subtrees.
    pub children: Vec<StyledNode<'a>>,
}

/// Viewport context used for responsive CSS matching.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleContext {
    /// Viewport width in CSS pixels.
    pub viewport_width: f32,
    /// Viewport height in CSS pixels.
    pub viewport_height: f32,
    /// Interaction state supplied by the browser shell.
    pub interaction: StyleInteraction,
}

/// Interaction-dependent selector state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyleInteraction {
    /// DOM node ids currently matching `:hover`.
    pub hovered_node_ids: Vec<webby_dom::NodeId>,
    /// DOM node ids currently matching `:focus`.
    pub focused_node_ids: Vec<webby_dom::NodeId>,
    /// DOM node ids currently matching `:active`.
    pub active_node_ids: Vec<webby_dom::NodeId>,
}

impl StyleContext {
    /// Creates a finite positive style context.
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            viewport_width: viewport_width.max(1.0),
            viewport_height: viewport_height.max(1.0),
            interaction: StyleInteraction::default(),
        }
    }

    /// Returns this context with interaction state attached.
    pub fn with_interaction(mut self, interaction: StyleInteraction) -> Self {
        self.interaction = interaction.normalized();
        self
    }
}

impl StyleInteraction {
    /// Creates empty interaction state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a hovered DOM node id.
    pub fn with_hovered_node(mut self, node_id: webby_dom::NodeId) -> Self {
        self.hovered_node_ids.push(node_id);
        self
    }

    /// Adds a focused DOM node id.
    pub fn with_focused_node(mut self, node_id: webby_dom::NodeId) -> Self {
        self.focused_node_ids.push(node_id);
        self
    }

    /// Adds an active DOM node id.
    pub fn with_active_node(mut self, node_id: webby_dom::NodeId) -> Self {
        self.active_node_ids.push(node_id);
        self
    }

    fn normalized(mut self) -> Self {
        sort_dedup(&mut self.hovered_node_ids);
        sort_dedup(&mut self.focused_node_ids);
        sort_dedup(&mut self.active_node_ids);
        self
    }
}

impl Default for StyleContext {
    fn default() -> Self {
        Self::new(800.0, 600.0)
    }
}

fn sort_dedup(values: &mut Vec<webby_dom::NodeId>) {
    values.sort_unstable();
    values.dedup();
}

impl<'a> StyledNode<'a> {
    /// Element tag name for element nodes.
    pub fn tag_name(&self) -> Option<&'a str> {
        match &self.node.kind {
            NodeKind::Element(element) => Some(&element.tag_name),
            NodeKind::Document | NodeKind::Text(_) => None,
        }
    }

    /// Element attributes for element nodes.
    pub fn attributes(&self) -> Option<&'a BTreeMap<String, String>> {
        match &self.node.kind {
            NodeKind::Element(element) => Some(&element.attributes),
            NodeKind::Document | NodeKind::Text(_) => None,
        }
    }

    /// Text content for text nodes.
    pub fn text(&self) -> Option<&'a str> {
        match &self.node.kind {
            NodeKind::Text(text) => Some(text),
            NodeKind::Document | NodeKind::Element(_) => None,
        }
    }
}

/// Builds a default styled tree for a whole document.
pub fn style_document(document: &Document) -> StyledNode<'_> {
    let stylesheet = compose_document_stylesheet(document, &[]);
    style_document_with_css(document, &stylesheet)
}

/// Builds a styled tree using default/document CSS and a viewport context.
pub fn style_document_for_viewport(
    document: &Document,
    viewport_width: f32,
    viewport_height: f32,
) -> StyledNode<'_> {
    let stylesheet = compose_document_stylesheet(document, &[]);
    style_document_with_css_for_viewport(document, &stylesheet, viewport_width, viewport_height)
}

/// Combines external stylesheets followed by document `<style>` blocks.
///
/// Default element styles remain the base layer inside the style tree builder,
/// and inline `style=""` declarations are still applied after this sheet.
pub fn compose_document_stylesheet(
    document: &Document,
    external_stylesheets: &[Stylesheet],
) -> Stylesheet {
    let mut combined = Stylesheet::default();
    for stylesheet in external_stylesheets {
        combined.rules.extend(stylesheet.rules.iter().cloned());
        combined
            .diagnostics
            .extend(stylesheet.diagnostics.iter().cloned());
    }

    let css = collect_style_blocks(&document.root).join("\n");
    if let Ok(embedded) = webby_css::parse_stylesheet(&css) {
        combined.rules.extend(embedded.rules);
        combined.diagnostics.extend(embedded.diagnostics);
    }
    collect_inline_style_diagnostics(&document.root, &mut combined.diagnostics);

    combined
}

/// Builds a styled tree for a whole document using an explicit stylesheet.
pub fn style_document_with_css<'a>(
    document: &'a Document,
    stylesheet: &Stylesheet,
) -> StyledNode<'a> {
    style_document_with_css_for_context(document, stylesheet, StyleContext::default())
}

/// Builds a styled tree for a whole document using CSS and viewport context.
pub fn style_document_with_css_for_viewport<'a>(
    document: &'a Document,
    stylesheet: &Stylesheet,
    viewport_width: f32,
    viewport_height: f32,
) -> StyledNode<'a> {
    style_document_with_css_for_context(
        document,
        stylesheet,
        StyleContext::new(viewport_width, viewport_height),
    )
}

/// Builds a styled tree for a whole document using CSS and a full style context.
pub fn style_document_with_css_and_context<'a>(
    document: &'a Document,
    stylesheet: &Stylesheet,
    context: StyleContext,
) -> StyledNode<'a> {
    style_document_with_css_for_context(document, stylesheet, context)
}

fn style_document_with_css_for_context<'a>(
    document: &'a Document,
    stylesheet: &Stylesheet,
    context: StyleContext,
) -> StyledNode<'a> {
    let mut ancestors = Vec::new();
    style_visible_node(
        &document.root,
        &ComputedStyle::default(),
        stylesheet,
        &context,
        &mut ancestors,
    )
    .unwrap_or_else(|| StyledNode {
        node: &document.root,
        style: ComputedStyle {
            display: Display::Block,
            ..ComputedStyle::default()
        },
        children: Vec::new(),
    })
}

/// Builds a default styled tree from a DOM node.
pub fn style_tree(root: &Node) -> StyledNode<'_> {
    style_tree_with_css(root, &Stylesheet::default())
}

/// Builds a styled tree from a DOM node using an explicit stylesheet.
pub fn style_tree_with_css<'a>(root: &'a Node, stylesheet: &Stylesheet) -> StyledNode<'a> {
    let mut ancestors = Vec::new();
    style_visible_node(
        root,
        &ComputedStyle::default(),
        stylesheet,
        &StyleContext::default(),
        &mut ancestors,
    )
    .unwrap_or_else(|| StyledNode {
        node: root,
        style: ComputedStyle::default(),
        children: Vec::new(),
    })
}

/// Formats a styled tree for deterministic CLI/debug output.
pub fn dump_style_tree(root: &StyledNode<'_>) -> String {
    let mut output = String::new();
    dump_styled_node(root, 0, &mut output);
    output
}

fn style_visible_node<'a>(
    node: &'a Node,
    parent_style: &ComputedStyle,
    stylesheet: &Stylesheet,
    context: &StyleContext,
    ancestors: &mut Vec<&'a Node>,
) -> Option<StyledNode<'a>> {
    let style = match &node.kind {
        NodeKind::Document => ComputedStyle {
            display: Display::Block,
            ..ComputedStyle::default()
        },
        NodeKind::Text(_) => text_style(parent_style),
        NodeKind::Element(element) => {
            element_style(node, element, parent_style, stylesheet, context, ancestors)
        }
    };

    if style.display == Display::None {
        return None;
    }

    ancestors.push(node);
    let children = node
        .render_children()
        .iter()
        .filter_map(|child| style_visible_node(child, &style, stylesheet, context, ancestors))
        .collect();
    ancestors.pop();

    Some(StyledNode {
        node,
        style,
        children,
    })
}

fn text_style(parent_style: &ComputedStyle) -> ComputedStyle {
    ComputedStyle {
        display: Display::Inline,
        visibility: parent_style.visibility,
        overflow_x: Overflow::Visible,
        overflow_y: Overflow::Visible,
        position: Position::Static,
        top: None,
        right: None,
        bottom: None,
        left: None,
        color: parent_style.color,
        background_color: Color::TRANSPARENT,
        font_size: parent_style.font_size,
        line_height: parent_style.line_height,
        font_family: parent_style.font_family,
        font_weight: parent_style.font_weight,
        text_decoration: parent_style.text_decoration,
        text_align: parent_style.text_align,
        white_space: parent_style.white_space,
        margin: Edges::ZERO,
        padding: Edges::ZERO,
        border: Border::default(),
        width: None,
        height: None,
        min_width: None,
        max_width: None,
        min_height: None,
        max_height: None,
        box_sizing: BoxSizing::ContentBox,
        flex_direction: FlexDirection::Row,
        gap: 0.0,
        row_gap: 0.0,
        column_gap: 0.0,
        grid_template_columns: Vec::new(),
        grid_template_rows: Vec::new(),
        grid_column: GridPlacement::default(),
        grid_row: GridPlacement::default(),
        justify_content: JustifyContent::FlexStart,
        align_items: AlignItems::Stretch,
        flex_grow: 0.0,
        transition: Transition::default(),
        is_link: parent_style.is_link,
    }
}

fn element_style(
    node: &Node,
    element: &ElementData,
    parent_style: &ComputedStyle,
    stylesheet: &Stylesheet,
    context: &StyleContext,
    ancestors: &[&Node],
) -> ComputedStyle {
    let mut style = ComputedStyle {
        color: parent_style.color,
        visibility: parent_style.visibility,
        background_color: Color::TRANSPARENT,
        font_size: parent_style.font_size,
        line_height: parent_style.line_height,
        font_family: parent_style.font_family,
        font_weight: parent_style.font_weight,
        text_decoration: parent_style.text_decoration,
        text_align: parent_style.text_align,
        white_space: parent_style.white_space,
        is_link: parent_style.is_link,
        ..ComputedStyle::default()
    };

    match element.tag_name.as_str() {
        "head" | "title" | "meta" | "link" | "script" | "style" => {
            style.display = Display::None;
        }
        "html" => {
            style.display = Display::Block;
        }
        "body" => {
            style.display = Display::Block;
            style.background_color = Color::WHITE;
            style.margin = Edges::trbl(8.0, 8.0, 8.0, 8.0);
        }
        "h1" => {
            style.display = Display::Block;
            style.font_size = 32.0;
            style.font_weight = FontWeight::Bold;
            style.margin = Edges::trbl(10.0, 0.0, 10.0, 0.0);
        }
        "h2" => {
            style.display = Display::Block;
            style.font_size = 24.0;
            style.font_weight = FontWeight::Bold;
            style.margin = Edges::trbl(9.0, 0.0, 9.0, 0.0);
        }
        "h3" => {
            style.display = Display::Block;
            style.font_size = 19.0;
            style.font_weight = FontWeight::Bold;
            style.margin = Edges::trbl(8.0, 0.0, 8.0, 0.0);
        }
        "p" => {
            style.display = Display::Block;
            style.margin = Edges::trbl(8.0, 0.0, 8.0, 0.0);
        }
        "form" => {
            style.display = Display::Block;
            style.margin = Edges::trbl(8.0, 0.0, 8.0, 0.0);
        }
        "table" => {
            style.display = Display::Block;
            style.margin = Edges::trbl(8.0, 0.0, 8.0, 0.0);
        }
        "thead" | "tbody" | "tr" => {
            style.display = Display::Block;
        }
        "th" => {
            style.display = Display::Block;
            style.font_weight = FontWeight::Bold;
            style.background_color = Color {
                r: 238,
                g: 240,
                b: 244,
                a: 255,
            };
            style.padding = Edges::trbl(4.0, 6.0, 4.0, 6.0);
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 120,
                    g: 126,
                    b: 136,
                    a: 255,
                },
            };
        }
        "td" => {
            style.display = Display::Block;
            style.padding = Edges::trbl(4.0, 6.0, 4.0, 6.0);
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 160,
                    g: 166,
                    b: 176,
                    a: 255,
                },
            };
        }
        "div" | "header" | "nav" | "main" | "article" | "section" | "aside" | "footer" => {
            style.display = Display::Block;
        }
        "ul" | "ol" => {
            style.display = Display::Block;
            style.margin = Edges::trbl(8.0, 0.0, 8.0, 0.0);
            style.padding = Edges::trbl(0.0, 0.0, 0.0, 24.0);
        }
        "li" => {
            style.display = Display::Block;
        }
        "strong" => {
            style.display = Display::Inline;
            style.font_weight = FontWeight::Bold;
        }
        "em" => {
            style.display = Display::Inline;
        }
        "small" => {
            style.display = Display::Inline;
            style.font_size = 13.0;
        }
        "code" => {
            style.display = Display::Inline;
            style.font_size = 14.0;
            style.font_family = FontFamily::Monospace;
            style.background_color = Color {
                r: 244,
                g: 245,
                b: 247,
                a: 255,
            };
        }
        "pre" => {
            style.display = Display::Block;
            style.font_size = 14.0;
            style.font_family = FontFamily::Monospace;
            style.white_space = WhiteSpace::Pre;
            style.background_color = Color {
                r: 244,
                g: 245,
                b: 247,
                a: 255,
            };
            style.margin = Edges::trbl(8.0, 0.0, 8.0, 0.0);
            style.padding = Edges::trbl(8.0, 8.0, 8.0, 8.0);
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 210,
                    g: 214,
                    b: 220,
                    a: 255,
                },
            };
        }
        "blockquote" => {
            style.display = Display::Block;
            style.margin = Edges::trbl(8.0, 0.0, 8.0, 12.0);
            style.padding = Edges::trbl(4.0, 0.0, 4.0, 10.0);
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 180,
                    g: 186,
                    b: 196,
                    a: 255,
                },
            };
        }
        "hr" => {
            style.display = Display::Block;
            style.height = Some(CssSize::Px(0.0));
            style.margin = Edges::trbl(8.0, 0.0, 8.0, 0.0);
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 180,
                    g: 186,
                    b: 196,
                    a: 255,
                },
            };
        }
        "figure" | "figcaption" => {
            style.display = Display::Block;
            if element.tag_name == "figcaption" {
                style.font_size = 13.0;
            } else {
                style.margin = Edges::trbl(8.0, 0.0, 8.0, 0.0);
            }
        }
        "span" | "label" => {
            style.display = Display::Inline;
        }
        "a" => {
            style.display = Display::Inline;
            style.color = Color::LINK_BLUE;
            style.text_decoration = TextDecoration::Underline;
            style.is_link = true;
        }
        "br" => {
            style.display = Display::LineBreak;
        }
        "img" | "svg" | "canvas" | "audio" | "video" | "iframe" => {
            style.display = Display::Inline;
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 180,
                    g: 180,
                    b: 180,
                    a: 255,
                },
            };
        }
        "input" => {
            style.display = Display::Inline;
            style.background_color = Color::WHITE;
            style.padding = Edges::trbl(3.0, 6.0, 3.0, 6.0);
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 120,
                    g: 126,
                    b: 136,
                    a: 255,
                },
            };
            let input_type = element
                .attributes
                .get("type")
                .map(|value| value.to_ascii_lowercase())
                .unwrap_or_else(|| "text".to_string());
            if matches!(
                input_type.as_str(),
                "text" | "search" | "password" | "email"
            ) {
                style.width = Some(CssSize::Px(180.0));
            } else if matches!(input_type.as_str(), "checkbox" | "radio") {
                style.width = Some(CssSize::Px(14.0));
                style.padding = Edges::trbl(1.0, 1.0, 1.0, 1.0);
            } else {
                style.width = Some(CssSize::Px(74.0));
                style.background_color = Color {
                    r: 238,
                    g: 240,
                    b: 244,
                    a: 255,
                };
            }
            style.height = Some(CssSize::Px(20.0));
        }
        "select" | "textarea" => {
            style.display = Display::Inline;
            style.background_color = Color::WHITE;
            style.padding = Edges::trbl(3.0, 6.0, 3.0, 6.0);
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 120,
                    g: 126,
                    b: 136,
                    a: 255,
                },
            };
            style.width = Some(CssSize::Px(180.0));
            style.height = Some(CssSize::Px(if element.tag_name == "textarea" {
                56.0
            } else {
                20.0
            }));
        }
        "button" => {
            style.display = Display::Inline;
            style.background_color = Color {
                r: 238,
                g: 240,
                b: 244,
                a: 255,
            };
            style.padding = Edges::trbl(3.0, 8.0, 3.0, 8.0);
            style.border = Border {
                width: 1.0,
                color: Color {
                    r: 120,
                    g: 126,
                    b: 136,
                    a: 255,
                },
            };
            style.height = Some(CssSize::Px(20.0));
        }
        _ if element.tag_name.contains('-') => {
            style.display = Display::Block;
        }
        _ => {
            style.display = Display::Inline;
        }
    }

    let mut tracker = CascadeTracker::default();
    apply_stylesheet(
        node,
        element,
        ancestors,
        stylesheet,
        context,
        &mut CascadeApplication {
            parent_style,
            style: &mut style,
            tracker: &mut tracker,
        },
    );
    apply_inline_style(
        element,
        &mut CascadeApplication {
            parent_style,
            style: &mut style,
            tracker: &mut tracker,
        },
    );
    style
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CascadePriority {
    important: bool,
    specificity: Specificity,
    order: usize,
}

#[derive(Debug, Clone, Default)]
struct CascadeTracker {
    color: Option<CascadePriority>,
    background_color: Option<CascadePriority>,
    font_size: Option<CascadePriority>,
    line_height: Option<CascadePriority>,
    font_family: Option<CascadePriority>,
    font_weight: Option<CascadePriority>,
    text_decoration: Option<CascadePriority>,
    text_align: Option<CascadePriority>,
    white_space: Option<CascadePriority>,
    display: Option<CascadePriority>,
    visibility: Option<CascadePriority>,
    overflow_x: Option<CascadePriority>,
    overflow_y: Option<CascadePriority>,
    position: Option<CascadePriority>,
    top: Option<CascadePriority>,
    right: Option<CascadePriority>,
    bottom: Option<CascadePriority>,
    left: Option<CascadePriority>,
    margin: Option<CascadePriority>,
    padding: Option<CascadePriority>,
    border_width: Option<CascadePriority>,
    border_color: Option<CascadePriority>,
    width: Option<CascadePriority>,
    height: Option<CascadePriority>,
    min_width: Option<CascadePriority>,
    max_width: Option<CascadePriority>,
    min_height: Option<CascadePriority>,
    max_height: Option<CascadePriority>,
    box_sizing: Option<CascadePriority>,
    flex_direction: Option<CascadePriority>,
    gap: Option<CascadePriority>,
    row_gap: Option<CascadePriority>,
    column_gap: Option<CascadePriority>,
    grid_template_columns: Option<CascadePriority>,
    grid_template_rows: Option<CascadePriority>,
    grid_column: Option<CascadePriority>,
    grid_row: Option<CascadePriority>,
    justify_content: Option<CascadePriority>,
    align_items: Option<CascadePriority>,
    flex_grow: Option<CascadePriority>,
    transition_property: Option<CascadePriority>,
    transition_duration: Option<CascadePriority>,
    transition_delay: Option<CascadePriority>,
    transition_timing_function: Option<CascadePriority>,
}

struct CascadeApplication<'a> {
    parent_style: &'a ComputedStyle,
    style: &'a mut ComputedStyle,
    tracker: &'a mut CascadeTracker,
}

fn apply_stylesheet(
    node: &Node,
    element: &ElementData,
    ancestors: &[&Node],
    stylesheet: &Stylesheet,
    context: &StyleContext,
    application: &mut CascadeApplication<'_>,
) {
    for (order, rule) in stylesheet.rules.iter().enumerate() {
        if !media_matches(rule.media.as_ref(), context) {
            continue;
        }
        if selector_matches(&rule.selector, node, element, ancestors, context) {
            let priority = CascadePriority {
                important: false,
                specificity: rule.selector.specificity(),
                order,
            };
            for declaration in &rule.declarations {
                apply_declaration(
                    application.style,
                    application.tracker,
                    declaration,
                    priority,
                    application.parent_style,
                );
            }
        }
    }
}

fn media_matches(media: Option<&MediaQuery>, context: &StyleContext) -> bool {
    let Some(media) = media else {
        return true;
    };
    if !matches!(media.media_type, MediaType::All | MediaType::Screen) {
        return false;
    }
    media
        .features
        .iter()
        .all(|feature| media_feature_matches(*feature, context))
}

fn media_feature_matches(feature: MediaFeature, context: &StyleContext) -> bool {
    let width = context.viewport_width.round() as u32;
    match feature {
        MediaFeature::MinWidth(value) => width >= value,
        MediaFeature::MaxWidth(value) => width <= value,
        MediaFeature::Width(value) => width == value,
        MediaFeature::Orientation(Orientation::Portrait) => {
            context.viewport_height >= context.viewport_width
        }
        MediaFeature::Orientation(Orientation::Landscape) => {
            context.viewport_width > context.viewport_height
        }
    }
}

fn apply_inline_style(element: &ElementData, application: &mut CascadeApplication<'_>) {
    let Some(raw_style) = element.attributes.get("style") else {
        return;
    };
    let Ok((declarations, _diagnostics)) = webby_css::parse_declarations(raw_style) else {
        return;
    };
    let specificity = Specificity {
        ids: u16::MAX,
        classes: u16::MAX,
        tags: u16::MAX,
    };
    for (order, declaration) in declarations.iter().enumerate() {
        apply_declaration(
            application.style,
            application.tracker,
            declaration,
            CascadePriority {
                important: false,
                specificity,
                order: usize::MAX
                    .saturating_sub(declarations.len())
                    .saturating_add(order),
            },
            application.parent_style,
        );
    }
}

fn apply_declaration(
    style: &mut ComputedStyle,
    tracker: &mut CascadeTracker,
    declaration: &Declaration,
    mut priority: CascadePriority,
    parent_style: &ComputedStyle,
) {
    priority.important = declaration.important;
    if let Value::CssWideKeyword(keyword) = declaration.value {
        apply_css_wide_keyword(
            style,
            tracker,
            declaration.property,
            keyword,
            priority,
            parent_style,
        );
        return;
    }
    match (&declaration.property, &declaration.value) {
        (Property::Color, Value::Color(color)) if should_apply(&mut tracker.color, priority) => {
            style.color = Color::from(*color);
        }
        (Property::Background | Property::BackgroundColor, Value::Color(color))
            if should_apply(&mut tracker.background_color, priority) =>
        {
            style.background_color = Color::from(*color);
        }
        (Property::FontSize, Value::Length(length))
            if should_apply(&mut tracker.font_size, priority) =>
        {
            style.font_size = length.px;
        }
        (Property::FontFamily, Value::FontFamily(family))
            if should_apply(&mut tracker.font_family, priority) =>
        {
            style.font_family = FontFamily::from(*family);
        }
        (Property::LineHeight, Value::LineHeight(line_height))
            if should_apply(&mut tracker.line_height, priority) =>
        {
            style.line_height = match line_height {
                webby_css::LineHeight::Normal => None,
                webby_css::LineHeight::Px(px) => Some(*px),
            };
        }
        (Property::FontWeight, Value::FontWeight(weight))
            if should_apply(&mut tracker.font_weight, priority) =>
        {
            style.font_weight = FontWeight::from(*weight);
        }
        (Property::TextDecoration, Value::TextDecoration(decoration))
            if should_apply(&mut tracker.text_decoration, priority) =>
        {
            style.text_decoration = TextDecoration::from(*decoration);
        }
        (Property::TextAlign, Value::TextAlign(align))
            if should_apply(&mut tracker.text_align, priority) =>
        {
            style.text_align = TextAlign::from(*align);
        }
        (Property::WhiteSpace, Value::WhiteSpace(white_space))
            if should_apply(&mut tracker.white_space, priority) =>
        {
            style.white_space = WhiteSpace::from(*white_space);
        }
        (Property::Display, Value::Display(display))
            if should_apply(&mut tracker.display, priority) =>
        {
            style.display = Display::from(*display);
        }
        (Property::Visibility, Value::Visibility(visibility))
            if should_apply(&mut tracker.visibility, priority) =>
        {
            style.visibility = Visibility::from(*visibility);
        }
        (Property::Overflow, Value::Overflow(overflow)) => {
            if should_apply(&mut tracker.overflow_x, priority) {
                style.overflow_x = Overflow::from(*overflow);
            }
            if should_apply(&mut tracker.overflow_y, priority) {
                style.overflow_y = Overflow::from(*overflow);
            }
        }
        (Property::OverflowX, Value::Overflow(overflow))
            if should_apply(&mut tracker.overflow_x, priority) =>
        {
            style.overflow_x = Overflow::from(*overflow);
        }
        (Property::OverflowY, Value::Overflow(overflow))
            if should_apply(&mut tracker.overflow_y, priority) =>
        {
            style.overflow_y = Overflow::from(*overflow);
        }
        (Property::Position, Value::Position(position))
            if should_apply(&mut tracker.position, priority) =>
        {
            style.position = Position::from(*position);
        }
        (Property::Top, Value::Size(size)) if should_apply(&mut tracker.top, priority) => {
            style.top = offset_from_css_size(*size);
        }
        (Property::Right, Value::Size(size)) if should_apply(&mut tracker.right, priority) => {
            style.right = offset_from_css_size(*size);
        }
        (Property::Bottom, Value::Size(size)) if should_apply(&mut tracker.bottom, priority) => {
            style.bottom = offset_from_css_size(*size);
        }
        (Property::Left, Value::Size(size)) if should_apply(&mut tracker.left, priority) => {
            style.left = offset_from_css_size(*size);
        }
        (Property::Margin, Value::Edges(edges)) if should_apply(&mut tracker.margin, priority) => {
            style.margin = Edges::from(*edges);
        }
        (Property::Padding, Value::Edges(edges))
            if should_apply(&mut tracker.padding, priority) =>
        {
            style.padding = Edges::from(*edges);
        }
        (Property::Border, Value::Border(border)) => {
            if let Some(width) = border.width
                && should_apply(&mut tracker.border_width, priority)
            {
                style.border.width = width.px;
            }
            if let Some(color) = border.color
                && should_apply(&mut tracker.border_color, priority)
            {
                style.border.color = Color::from(color);
            }
        }
        (Property::BorderWidth, Value::Length(length))
            if should_apply(&mut tracker.border_width, priority) =>
        {
            style.border.width = length.px;
        }
        (Property::BorderColor, Value::Color(color))
            if should_apply(&mut tracker.border_color, priority) =>
        {
            style.border.color = Color::from(*color);
        }
        (Property::Width, Value::Size(size)) if should_apply(&mut tracker.width, priority) => {
            style.width = match CssSize::from(*size) {
                CssSize::Auto => None,
                size => Some(size),
            };
        }
        (Property::Height, Value::Size(size)) if should_apply(&mut tracker.height, priority) => {
            style.height = match CssSize::from(*size) {
                CssSize::Auto => None,
                size => Some(size),
            };
        }
        (Property::MinWidth, Value::Size(size))
            if should_apply(&mut tracker.min_width, priority) =>
        {
            style.min_width = size_if_not_auto(*size);
        }
        (Property::MaxWidth, Value::Size(size))
            if should_apply(&mut tracker.max_width, priority) =>
        {
            style.max_width = size_if_not_auto(*size);
        }
        (Property::MinHeight, Value::Size(size))
            if should_apply(&mut tracker.min_height, priority) =>
        {
            style.min_height = size_if_not_auto(*size);
        }
        (Property::MaxHeight, Value::Size(size))
            if should_apply(&mut tracker.max_height, priority) =>
        {
            style.max_height = size_if_not_auto(*size);
        }
        (Property::BoxSizing, Value::BoxSizing(box_sizing))
            if should_apply(&mut tracker.box_sizing, priority) =>
        {
            style.box_sizing = BoxSizing::from(*box_sizing);
        }
        (Property::FlexDirection, Value::FlexDirection(direction))
            if should_apply(&mut tracker.flex_direction, priority) =>
        {
            style.flex_direction = FlexDirection::from(*direction);
        }
        (Property::Gap, Value::Length(length)) => {
            if should_apply(&mut tracker.gap, priority) {
                style.gap = length.px;
            }
            if should_apply(&mut tracker.row_gap, priority) {
                style.row_gap = length.px;
            }
            if should_apply(&mut tracker.column_gap, priority) {
                style.column_gap = length.px;
            }
        }
        (Property::RowGap, Value::Length(length))
            if should_apply(&mut tracker.row_gap, priority) =>
        {
            style.row_gap = length.px;
        }
        (Property::ColumnGap, Value::Length(length))
            if should_apply(&mut tracker.column_gap, priority) =>
        {
            style.column_gap = length.px;
        }
        (Property::GridTemplateColumns, Value::GridTrackList(tracks))
            if should_apply(&mut tracker.grid_template_columns, priority) =>
        {
            style.grid_template_columns = tracks.iter().copied().map(GridTrack::from).collect();
        }
        (Property::GridTemplateRows, Value::GridTrackList(tracks))
            if should_apply(&mut tracker.grid_template_rows, priority) =>
        {
            style.grid_template_rows = tracks.iter().copied().map(GridTrack::from).collect();
        }
        (Property::GridColumn, Value::GridPlacement(placement))
            if should_apply(&mut tracker.grid_column, priority) =>
        {
            style.grid_column = GridPlacement::from(*placement);
        }
        (Property::GridRow, Value::GridPlacement(placement))
            if should_apply(&mut tracker.grid_row, priority) =>
        {
            style.grid_row = GridPlacement::from(*placement);
        }
        (Property::JustifyContent, Value::JustifyContent(justify))
            if should_apply(&mut tracker.justify_content, priority) =>
        {
            style.justify_content = JustifyContent::from(*justify);
        }
        (Property::AlignItems, Value::AlignItems(align))
            if should_apply(&mut tracker.align_items, priority) =>
        {
            style.align_items = AlignItems::from(*align);
        }
        (Property::Flex | Property::FlexGrow, Value::Number(value))
            if should_apply(&mut tracker.flex_grow, priority) =>
        {
            style.flex_grow = *value;
        }
        (Property::TransitionProperty, Value::TransitionProperties(properties))
            if should_apply(&mut tracker.transition_property, priority) =>
        {
            style.transition.properties = properties
                .iter()
                .copied()
                .map(TransitionProperty::from)
                .collect();
        }
        (Property::TransitionDuration, Value::TimeMs(duration_ms))
            if should_apply(&mut tracker.transition_duration, priority) =>
        {
            style.transition.duration_ms = *duration_ms;
        }
        (Property::TransitionDelay, Value::TimeMs(delay_ms))
            if should_apply(&mut tracker.transition_delay, priority) =>
        {
            style.transition.delay_ms = *delay_ms;
        }
        (Property::TransitionTimingFunction, Value::TransitionTimingFunction(timing))
            if should_apply(&mut tracker.transition_timing_function, priority) =>
        {
            style.transition.timing_function = TransitionTimingFunction::from(*timing);
        }
        (Property::Transition, Value::Transition(transition)) => {
            if should_apply(&mut tracker.transition_property, priority) {
                style.transition.properties = transition
                    .properties
                    .iter()
                    .copied()
                    .map(TransitionProperty::from)
                    .collect();
            }
            if should_apply(&mut tracker.transition_duration, priority) {
                style.transition.duration_ms = transition.duration_ms;
            }
            if should_apply(&mut tracker.transition_delay, priority) {
                style.transition.delay_ms = transition.delay_ms;
            }
            if should_apply(&mut tracker.transition_timing_function, priority) {
                style.transition.timing_function =
                    TransitionTimingFunction::from(transition.timing_function);
            }
        }
        _ => {}
    }
}

fn apply_css_wide_keyword(
    style: &mut ComputedStyle,
    tracker: &mut CascadeTracker,
    property: Property,
    keyword: CssWideKeyword,
    priority: CascadePriority,
    parent_style: &ComputedStyle,
) {
    let initial = ComputedStyle::default();
    let source = match keyword {
        CssWideKeyword::Inherit => parent_style,
        CssWideKeyword::Initial => &initial,
        CssWideKeyword::Unset if property_is_inherited(property) => parent_style,
        CssWideKeyword::Unset => &initial,
    };

    macro_rules! copy_value {
        ($tracker:ident, $field:ident) => {
            if should_apply(&mut tracker.$tracker, priority) {
                style.$field = source.$field;
            }
        };
    }

    match property {
        Property::Color => copy_value!(color, color),
        Property::Background | Property::BackgroundColor => {
            copy_value!(background_color, background_color)
        }
        Property::FontSize => copy_value!(font_size, font_size),
        Property::FontFamily => copy_value!(font_family, font_family),
        Property::FontWeight => copy_value!(font_weight, font_weight),
        Property::LineHeight => copy_value!(line_height, line_height),
        Property::TextAlign => copy_value!(text_align, text_align),
        Property::WhiteSpace => copy_value!(white_space, white_space),
        Property::TextDecoration => copy_value!(text_decoration, text_decoration),
        Property::Display => copy_value!(display, display),
        Property::Visibility => copy_value!(visibility, visibility),
        Property::Overflow => {
            copy_value!(overflow_x, overflow_x);
            copy_value!(overflow_y, overflow_y);
        }
        Property::OverflowX => copy_value!(overflow_x, overflow_x),
        Property::OverflowY => copy_value!(overflow_y, overflow_y),
        Property::Margin => copy_value!(margin, margin),
        Property::Padding => copy_value!(padding, padding),
        Property::Border => {
            if should_apply(&mut tracker.border_width, priority) {
                style.border.width = source.border.width;
            }
            if should_apply(&mut tracker.border_color, priority) {
                style.border.color = source.border.color;
            }
        }
        Property::BorderWidth => {
            if should_apply(&mut tracker.border_width, priority) {
                style.border.width = source.border.width;
            }
        }
        Property::BorderColor => {
            if should_apply(&mut tracker.border_color, priority) {
                style.border.color = source.border.color;
            }
        }
        Property::Width => copy_value!(width, width),
        Property::Height => copy_value!(height, height),
        Property::MinWidth => copy_value!(min_width, min_width),
        Property::MaxWidth => copy_value!(max_width, max_width),
        Property::MinHeight => copy_value!(min_height, min_height),
        Property::MaxHeight => copy_value!(max_height, max_height),
        Property::BoxSizing => copy_value!(box_sizing, box_sizing),
        Property::Position => copy_value!(position, position),
        Property::Top => copy_value!(top, top),
        Property::Right => copy_value!(right, right),
        Property::Bottom => copy_value!(bottom, bottom),
        Property::Left => copy_value!(left, left),
        Property::FlexDirection => copy_value!(flex_direction, flex_direction),
        Property::Gap => {
            copy_value!(gap, gap);
            copy_value!(row_gap, row_gap);
            copy_value!(column_gap, column_gap);
        }
        Property::RowGap => copy_value!(row_gap, row_gap),
        Property::ColumnGap => copy_value!(column_gap, column_gap),
        Property::GridTemplateColumns => {
            if should_apply(&mut tracker.grid_template_columns, priority) {
                style.grid_template_columns = source.grid_template_columns.clone();
            }
        }
        Property::GridTemplateRows => {
            if should_apply(&mut tracker.grid_template_rows, priority) {
                style.grid_template_rows = source.grid_template_rows.clone();
            }
        }
        Property::GridColumn => copy_value!(grid_column, grid_column),
        Property::GridRow => copy_value!(grid_row, grid_row),
        Property::JustifyContent => copy_value!(justify_content, justify_content),
        Property::AlignItems => copy_value!(align_items, align_items),
        Property::Flex | Property::FlexGrow => copy_value!(flex_grow, flex_grow),
        Property::TransitionProperty => {
            if should_apply(&mut tracker.transition_property, priority) {
                style.transition.properties = source.transition.properties.clone();
            }
        }
        Property::TransitionDuration => {
            if should_apply(&mut tracker.transition_duration, priority) {
                style.transition.duration_ms = source.transition.duration_ms;
            }
        }
        Property::TransitionDelay => {
            if should_apply(&mut tracker.transition_delay, priority) {
                style.transition.delay_ms = source.transition.delay_ms;
            }
        }
        Property::TransitionTimingFunction => {
            if should_apply(&mut tracker.transition_timing_function, priority) {
                style.transition.timing_function = source.transition.timing_function;
            }
        }
        Property::Transition => {
            if should_apply(&mut tracker.transition_property, priority) {
                style.transition.properties = source.transition.properties.clone();
            }
            if should_apply(&mut tracker.transition_duration, priority) {
                style.transition.duration_ms = source.transition.duration_ms;
            }
            if should_apply(&mut tracker.transition_delay, priority) {
                style.transition.delay_ms = source.transition.delay_ms;
            }
            if should_apply(&mut tracker.transition_timing_function, priority) {
                style.transition.timing_function = source.transition.timing_function;
            }
        }
    }
}

fn property_is_inherited(property: Property) -> bool {
    matches!(
        property,
        Property::Color
            | Property::FontFamily
            | Property::FontSize
            | Property::FontWeight
            | Property::LineHeight
            | Property::TextAlign
            | Property::Visibility
            | Property::WhiteSpace
    )
}

fn size_if_not_auto(size: Size) -> Option<CssSize> {
    match CssSize::from(size) {
        CssSize::Auto => None,
        size => Some(size),
    }
}

fn offset_from_css_size(size: Size) -> Option<CssSize> {
    size_if_not_auto(size)
}

fn should_apply(current: &mut Option<CascadePriority>, incoming: CascadePriority) -> bool {
    if current.is_some_and(|existing| existing > incoming) {
        return false;
    }
    *current = Some(incoming);
    true
}

fn selector_matches(
    selector: &Selector,
    node: &Node,
    element: &ElementData,
    ancestors: &[&Node],
    context: &StyleContext,
) -> bool {
    let Some(last) = selector.parts.last() else {
        return false;
    };
    if !selector_part_matches(last, node, element, ancestors, context) {
        return false;
    }

    let mut ancestor_limit = ancestors.len();
    for part_index in (0..selector.parts.len().saturating_sub(1)).rev() {
        let combinator = selector.combinators[part_index];
        let part = &selector.parts[part_index];
        match combinator {
            Combinator::Child => {
                if ancestor_limit == 0 {
                    return false;
                }
                let parent_index = ancestor_limit - 1;
                let parent_node = ancestors[parent_index];
                let Some(parent) = element_from_node(parent_node) else {
                    return false;
                };
                if !selector_part_matches(
                    part,
                    parent_node,
                    parent,
                    &ancestors[..parent_index],
                    context,
                ) {
                    return false;
                }
                ancestor_limit = parent_index;
            }
            Combinator::Descendant => {
                let mut found = None;
                for candidate_index in (0..ancestor_limit).rev() {
                    let candidate_node = ancestors[candidate_index];
                    if let Some(candidate) = element_from_node(candidate_node)
                        && selector_part_matches(
                            part,
                            candidate_node,
                            candidate,
                            &ancestors[..candidate_index],
                            context,
                        )
                    {
                        found = Some(candidate_index);
                        break;
                    }
                }
                let Some(found_index) = found else {
                    return false;
                };
                ancestor_limit = found_index;
            }
        }
    }

    true
}

fn element_from_node(node: &Node) -> Option<&ElementData> {
    match &node.kind {
        NodeKind::Element(element) => Some(element),
        NodeKind::Document | NodeKind::Text(_) => None,
    }
}

fn selector_part_matches(
    part: &SelectorPart,
    node: &Node,
    element: &ElementData,
    ancestors: &[&Node],
    context: &StyleContext,
) -> bool {
    if let Some(tag_name) = &part.tag_name
        && tag_name != &element.tag_name
    {
        return false;
    }

    if let Some(id) = &part.id
        && element.attributes.get("id") != Some(id)
    {
        return false;
    }

    if !part.classes.is_empty() {
        let Some(classes) = element.attributes.get("class") else {
            return false;
        };
        for required in &part.classes {
            if !classes.split_whitespace().any(|class| class == required) {
                return false;
            }
        }
    }

    for attribute in &part.attributes {
        let Some(value) = element.attributes.get(&attribute.name) else {
            return false;
        };
        if let Some(required) = &attribute.value
            && value != required
        {
            return false;
        }
    }

    for pseudo_class in &part.pseudo_classes {
        if !pseudo_class_matches(*pseudo_class, node, element, ancestors, context) {
            return false;
        }
    }

    part.universal
        || part.tag_name.is_some()
        || !part.classes.is_empty()
        || part.id.is_some()
        || !part.attributes.is_empty()
        || !part.pseudo_classes.is_empty()
}

fn pseudo_class_matches(
    pseudo_class: PseudoClass,
    node: &Node,
    element: &ElementData,
    ancestors: &[&Node],
    context: &StyleContext,
) -> bool {
    match pseudo_class {
        PseudoClass::Hover => context
            .interaction
            .hovered_node_ids
            .binary_search(&node.id)
            .is_ok(),
        PseudoClass::Focus => context
            .interaction
            .focused_node_ids
            .binary_search(&node.id)
            .is_ok(),
        PseudoClass::Active => context
            .interaction
            .active_node_ids
            .binary_search(&node.id)
            .is_ok(),
        PseudoClass::Checked => element.attributes.contains_key("checked"),
        PseudoClass::Disabled => element.attributes.contains_key("disabled"),
        PseudoClass::FirstChild => element_child_position(node, ancestors) == Some(1),
        PseudoClass::LastChild => is_last_element_child(node, ancestors),
        PseudoClass::NthChild(index) => element_child_position(node, ancestors) == Some(index),
    }
}

fn element_child_position(node: &Node, ancestors: &[&Node]) -> Option<u32> {
    let parent = ancestors.last()?;
    let mut position = 0u32;
    for child in parent.render_children() {
        if matches!(child.kind, NodeKind::Element(_)) {
            position = position.saturating_add(1);
        }
        if child.id == node.id {
            return Some(position);
        }
    }
    None
}

fn is_last_element_child(node: &Node, ancestors: &[&Node]) -> bool {
    let Some(parent) = ancestors.last() else {
        return false;
    };
    parent
        .render_children()
        .iter()
        .rev()
        .find(|child| matches!(child.kind, NodeKind::Element(_)))
        .is_some_and(|child| child.id == node.id)
}

fn collect_style_blocks(node: &Node) -> Vec<String> {
    let mut blocks = Vec::new();
    collect_style_blocks_from_node(node, false, &mut blocks);
    blocks
}

fn collect_style_blocks_from_node(node: &Node, inside_style: bool, blocks: &mut Vec<String>) {
    match &node.kind {
        NodeKind::Text(text) if inside_style => blocks.push(text.clone()),
        NodeKind::Text(_) => {}
        NodeKind::Element(element) => {
            let style_context = inside_style || element.tag_name == "style";
            for child in node.tree_children() {
                collect_style_blocks_from_node(child, style_context, blocks);
            }
        }
        NodeKind::Document => {
            for child in node.tree_children() {
                collect_style_blocks_from_node(child, inside_style, blocks);
            }
        }
    }
}

fn collect_inline_style_diagnostics(node: &Node, diagnostics: &mut Vec<webby_css::CssDiagnostic>) {
    if let NodeKind::Element(element) = &node.kind
        && let Some(style) = element.attributes.get("style")
        && let Ok((_, inline_diagnostics)) = webby_css::parse_declarations(style)
    {
        diagnostics.extend(inline_diagnostics.into_iter().map(|diagnostic| {
            webby_css::CssDiagnostic {
                offset: diagnostic.offset,
                message: format!(
                    "inline style on <{}>: {}",
                    element.tag_name, diagnostic.message
                ),
            }
        }));
    }
    for child in node.tree_children() {
        collect_inline_style_diagnostics(child, diagnostics);
    }
}

fn dump_styled_node(node: &StyledNode<'_>, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    output.push_str(&indent);

    match &node.node.kind {
        NodeKind::Document => output.push_str("#document"),
        NodeKind::Element(element) => {
            output.push('<');
            output.push_str(&element.tag_name);
            for (name, value) in &element.attributes {
                output.push(' ');
                output.push_str(name);
                output.push_str("=\"");
                output.push_str(&escape_dump_string(value));
                output.push('"');
            }
            output.push('>');
        }
        NodeKind::Text(text) => {
            output.push('"');
            output.push_str(&escape_dump_string(text));
            output.push('"');
        }
    }

    output.push_str(" display=");
    output.push_str(node.style.display.as_str());
    output.push_str(" color=");
    output.push_str(&node.style.color.as_rgba());
    output.push_str(" visibility=");
    output.push_str(node.style.visibility.as_str());
    output.push_str(" overflow-x=");
    output.push_str(node.style.overflow_x.as_str());
    output.push_str(" overflow-y=");
    output.push_str(node.style.overflow_y.as_str());
    output.push_str(" background=");
    output.push_str(&node.style.background_color.as_rgba());
    output.push_str(" font-size=");
    output.push_str(&format!("{:.1}", node.style.font_size));
    output.push_str(" line-height=");
    match node.style.line_height {
        Some(line_height) => output.push_str(&format!("{line_height:.1}")),
        None => output.push_str("normal"),
    }
    output.push_str(" font-family=");
    output.push_str(node.style.font_family.as_str());
    output.push_str(" font-weight=");
    output.push_str(node.style.font_weight.as_str());
    output.push_str(" text-decoration=");
    output.push_str(node.style.text_decoration.as_str());
    output.push_str(" text-align=");
    output.push_str(node.style.text_align.as_str());
    output.push_str(" white-space=");
    output.push_str(node.style.white_space.as_str());
    output.push_str(" margin=");
    output.push_str(&node.style.margin.as_debug_value());
    output.push_str(" padding=");
    output.push_str(&node.style.padding.as_debug_value());
    output.push_str(" border-width=");
    output.push_str(&format!("{:.1}", node.style.border.width));
    output.push_str(" border-color=");
    output.push_str(&node.style.border.color.as_rgba());
    output.push_str(" width=");
    output.push_str(&format_optional_size(node.style.width));
    output.push_str(" height=");
    output.push_str(&format_optional_size(node.style.height));
    output.push_str(" min-width=");
    output.push_str(&format_optional_size(node.style.min_width));
    output.push_str(" max-width=");
    output.push_str(&format_optional_size(node.style.max_width));
    output.push_str(" min-height=");
    output.push_str(&format_optional_size(node.style.min_height));
    output.push_str(" max-height=");
    output.push_str(&format_optional_size(node.style.max_height));
    output.push_str(" box-sizing=");
    output.push_str(node.style.box_sizing.as_str());
    output.push_str(" position=");
    output.push_str(node.style.position.as_str());
    output.push_str(" top=");
    output.push_str(&format_optional_size(node.style.top));
    output.push_str(" right=");
    output.push_str(&format_optional_size(node.style.right));
    output.push_str(" bottom=");
    output.push_str(&format_optional_size(node.style.bottom));
    output.push_str(" left=");
    output.push_str(&format_optional_size(node.style.left));
    output.push_str(" flex-direction=");
    output.push_str(node.style.flex_direction.as_str());
    output.push_str(" gap=");
    output.push_str(&format!("{:.1}", node.style.gap));
    output.push_str(" row-gap=");
    output.push_str(&format!("{:.1}", node.style.row_gap));
    output.push_str(" column-gap=");
    output.push_str(&format!("{:.1}", node.style.column_gap));
    output.push_str(" grid-template-columns=");
    output.push_str(&format_grid_tracks(&node.style.grid_template_columns));
    output.push_str(" grid-template-rows=");
    output.push_str(&format_grid_tracks(&node.style.grid_template_rows));
    output.push_str(" grid-column=");
    output.push_str(&node.style.grid_column.as_debug_value());
    output.push_str(" grid-row=");
    output.push_str(&node.style.grid_row.as_debug_value());
    output.push_str(" justify-content=");
    output.push_str(node.style.justify_content.as_str());
    output.push_str(" align-items=");
    output.push_str(node.style.align_items.as_str());
    output.push_str(" flex-grow=");
    output.push_str(&format!("{:.1}", node.style.flex_grow));
    output.push_str(" transition=");
    output.push_str(&format_transition(&node.style.transition));
    output.push_str(" link=");
    output.push_str(if node.style.is_link { "true" } else { "false" });
    output.push('\n');

    for child in &node.children {
        dump_styled_node(child, depth + 1, output);
    }
}

fn format_optional_size(value: Option<CssSize>) -> String {
    match value {
        Some(value) => value.as_debug_value(),
        None => "auto".to_string(),
    }
}

fn format_transition(transition: &Transition) -> String {
    if transition.duration_ms == 0 || transition.properties.is_empty() {
        return "none".to_string();
    }
    let properties = transition
        .properties
        .iter()
        .map(|property| match property {
            TransitionProperty::All => "all",
            TransitionProperty::Color => "color",
            TransitionProperty::BackgroundColor => "background-color",
            TransitionProperty::Left => "left",
            TransitionProperty::Top => "top",
        })
        .collect::<Vec<_>>()
        .join(",");
    let timing = match transition.timing_function {
        TransitionTimingFunction::Linear => "linear",
        TransitionTimingFunction::Ease => "ease",
    };
    format!(
        "{} {}ms delay={}ms timing={}",
        properties, transition.duration_ms, transition.delay_ms, timing
    )
}

fn format_grid_tracks(tracks: &[GridTrack]) -> String {
    if tracks.is_empty() {
        return "none".to_string();
    }
    tracks
        .iter()
        .map(|track| track.as_debug_value())
        .collect::<Vec<_>>()
        .join("/")
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
        AlignItems, BoxSizing, Color, CssSize, Display, FlexDirection, FontFamily, FontWeight,
        GridPlacement, GridTrack, JustifyContent, Overflow, Position, StyleContext,
        StyleInteraction, TextAlign, TextDecoration, TransitionProperty, TransitionTimingFunction,
        Visibility, WhiteSpace, compose_document_stylesheet, dump_style_tree, style_document,
        style_document_with_css, style_document_with_css_and_context,
        style_document_with_css_for_viewport, style_tree,
    };
    use webby_html::parse_document;

    #[test]
    fn default_body_style_is_block_with_margin_and_background() -> webby_core::WebbyResult<()> {
        let document = parse_document("<body>Hello</body>")?;
        let styled = style_document(&document);
        let body = find_first_tag(&styled, "body")?;

        assert_eq!(body.style.display, Display::Block);
        assert_eq!(body.style.background_color, Color::WHITE);
        assert_eq!(body.style.margin.top, 8.0);
        Ok(())
    }

    #[test]
    fn heading_styles_are_bold_and_larger_than_body_text() -> webby_core::WebbyResult<()> {
        let document = parse_document("<body><h1>One</h1><h2>Two</h2><h3>Three</h3></body>")?;
        let styled = style_document(&document);
        let h1 = find_first_tag(&styled, "h1")?;
        let h2 = find_first_tag(&styled, "h2")?;
        let h3 = find_first_tag(&styled, "h3")?;

        assert_eq!(h1.style.font_weight, FontWeight::Bold);
        assert!(h1.style.font_size > h2.style.font_size);
        assert!(h2.style.font_size > h3.style.font_size);
        assert!(h3.style.font_size > 16.0);
        Ok(())
    }

    #[test]
    fn paragraph_div_and_span_display_behavior() -> webby_core::WebbyResult<()> {
        let document = parse_document("<div><p>Para <span>span</span></p></div>")?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "div")?.style.display,
            Display::Block
        );
        assert_eq!(find_first_tag(&styled, "p")?.style.display, Display::Block);
        assert_eq!(
            find_first_tag(&styled, "span")?.style.display,
            Display::Inline
        );
        Ok(())
    }

    #[test]
    fn custom_elements_default_to_block_for_shadow_hosts() -> webby_core::WebbyResult<()> {
        let document = parse_document("<x-card>Light</x-card>")?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "x-card")?.style.display,
            Display::Block
        );
        Ok(())
    }

    #[test]
    fn link_style_is_blue_underlined_and_marked_link() -> webby_core::WebbyResult<()> {
        let document = parse_document("<a href=\"/docs\">Docs</a>")?;
        let styled = style_document(&document);
        let link = find_first_tag(&styled, "a")?;

        assert_eq!(link.style.display, Display::Inline);
        assert_eq!(link.style.color, Color::LINK_BLUE);
        assert_eq!(link.style.text_decoration, TextDecoration::Underline);
        assert!(link.style.is_link);
        Ok(())
    }

    #[test]
    fn br_is_represented_as_line_break() -> webby_core::WebbyResult<()> {
        let document = parse_document("<p>Hello<br>Webby</p>")?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "br")?.style.display,
            Display::LineBreak
        );
        Ok(())
    }

    #[test]
    fn default_form_control_styles_exist() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<form><label for=\"q\">Query</label><input type=\"search\" name=\"q\"><button>Go</button></form>",
        )?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "form")?.style.display,
            Display::Block
        );
        assert_eq!(
            find_first_tag(&styled, "label")?.style.display,
            Display::Inline
        );
        let input = find_first_tag(&styled, "input")?;
        assert_eq!(input.style.display, Display::Inline);
        assert_eq!(input.style.width, Some(CssSize::Px(180.0)));
        assert_eq!(input.style.height, Some(CssSize::Px(20.0)));
        assert!(input.style.border.width > 0.0);
        let button = find_first_tag(&styled, "button")?;
        assert_eq!(button.style.display, Display::Inline);
        assert_eq!(button.style.height, Some(CssSize::Px(20.0)));
        assert!(button.style.border.width > 0.0);
        Ok(())
    }

    #[test]
    fn default_richer_form_control_styles_exist() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<form><input type=\"password\"><input type=\"email\"><input type=\"checkbox\"><input type=\"radio\"><select><option>One</option></select><textarea>Hi</textarea></form>",
        )?;
        let styled = style_document(&document);
        let inputs = collect_tags(&styled, "input");

        assert_eq!(inputs[0].style.width, Some(CssSize::Px(180.0)));
        assert_eq!(inputs[1].style.width, Some(CssSize::Px(180.0)));
        assert_eq!(inputs[2].style.width, Some(CssSize::Px(14.0)));
        assert_eq!(inputs[3].style.width, Some(CssSize::Px(14.0)));

        let select = find_first_tag(&styled, "select")?;
        assert_eq!(select.style.display, Display::Inline);
        assert_eq!(select.style.width, Some(CssSize::Px(180.0)));
        let textarea = find_first_tag(&styled, "textarea")?;
        assert_eq!(textarea.style.display, Display::Inline);
        assert_eq!(textarea.style.height, Some(CssSize::Px(56.0)));
        Ok(())
    }

    #[test]
    fn hidden_metadata_and_script_content_are_excluded() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<html><head><title>T</title><meta name=\"x\"><link href=\"x\"><style>p{}</style><script>bad()</script></head><body>Visible</body></html>",
        )?;
        let styled = style_document(&document);
        let dump = dump_style_tree(&styled);

        assert!(!dump.contains("<head"));
        assert!(!dump.contains("<title"));
        assert!(!dump.contains("<meta"));
        assert!(!dump.contains("<link"));
        assert!(!dump.contains("<script"));
        assert!(!dump.contains("<style"));
        assert!(dump.contains("\"Visible\""));
        Ok(())
    }

    #[test]
    fn text_nodes_preserve_text_and_inherit_link_style() -> webby_core::WebbyResult<()> {
        let document = parse_document("<a href=\"/docs\">Docs</a>")?;
        let styled = style_document(&document);
        let text = find_first_text(&styled, "Docs")?;

        assert_eq!(text.text(), Some("Docs"));
        assert_eq!(text.style.color, Color::LINK_BLUE);
        assert!(text.style.is_link);
        Ok(())
    }

    #[test]
    fn style_dump_is_deterministic_and_contains_core_properties() -> webby_core::WebbyResult<()> {
        let document = parse_document("<body><p>Hello <a href=\"/docs\">docs</a></p></body>")?;
        let styled = style_document(&document);
        let dump = dump_style_tree(&styled);

        assert_eq!(dump, dump_style_tree(&styled));
        assert!(dump.contains("<body> display=block"));
        assert!(dump.contains("<p> display=block"));
        assert!(dump.contains("<a href=\"/docs\"> display=inline"));
        assert!(dump.contains("text-decoration=underline"));
        Ok(())
    }

    #[test]
    fn style_tree_keeps_dom_reference_information() -> webby_core::WebbyResult<()> {
        let document = parse_document("<img src=\"logo.png\">")?;
        let styled = style_document(&document);
        let image = find_first_tag(&styled, "img")?;

        assert_eq!(
            image
                .attributes()
                .and_then(|attributes| attributes.get("src"))
                .map(String::as_str),
            Some("logo.png")
        );
        assert_eq!(image.style.display, Display::Inline);
        assert_eq!(image.style.border.width, 1.0);
        Ok(())
    }

    #[test]
    fn default_media_placeholder_styles_exist() -> webby_core::WebbyResult<()> {
        let document = parse_document("<audio controls></audio><video controls></video>")?;
        let styled = style_document(&document);
        let audio = find_first_tag(&styled, "audio")?;
        let video = find_first_tag(&styled, "video")?;

        assert_eq!(audio.style.display, Display::Inline);
        assert_eq!(video.style.display, Display::Inline);
        assert_eq!(audio.style.border.width, 1.0);
        assert_eq!(video.style.border.width, 1.0);
        Ok(())
    }

    #[test]
    fn style_tree_can_start_from_any_node() -> webby_core::WebbyResult<()> {
        let document = parse_document("<span>Text</span>")?;
        let styled = style_tree(&document.root);

        assert_eq!(styled.children.len(), 1);
        Ok(())
    }

    #[test]
    fn style_block_changes_heading_body_and_link_styles() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                body { background-color: #eee; color: red; }
                h1 { font-size: 40px; }
                a { color: green; text-decoration: none; }
            </style><body><h1>Title</h1><a href=\"/x\">Link</a></body>",
        )?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "body")?.style.background_color,
            Color {
                r: 238,
                g: 238,
                b: 238,
                a: 255
            }
        );
        assert_eq!(find_first_tag(&styled, "body")?.style.color.r, 255);
        assert_eq!(find_first_tag(&styled, "h1")?.style.font_size, 40.0);
        assert_eq!(
            find_first_tag(&styled, "a")?.style.color,
            Color {
                r: 0,
                g: 128,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            find_first_tag(&styled, "a")?.style.text_decoration,
            TextDecoration::None
        );
        Ok(())
    }

    #[test]
    fn css_box_model_sizing_properties_are_computed() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                div {
                    width: 50%;
                    min-width: 120px;
                    max-width: 320px;
                    height: auto;
                    min-height: 20px;
                    max-height: 40px;
                    box-sizing: border-box;
                }
            </style><body><div>Box</div></body>",
        )?;
        let styled = style_document(&document);
        let div = find_first_tag(&styled, "div")?;

        assert_eq!(div.style.width, Some(CssSize::Percent(0.5)));
        assert_eq!(div.style.height, None);
        assert_eq!(div.style.min_width, Some(CssSize::Px(120.0)));
        assert_eq!(div.style.max_width, Some(CssSize::Px(320.0)));
        assert_eq!(div.style.min_height, Some(CssSize::Px(20.0)));
        assert_eq!(div.style.max_height, Some(CssSize::Px(40.0)));
        assert_eq!(div.style.box_sizing, BoxSizing::BorderBox);
        Ok(())
    }

    #[test]
    fn css_display_and_visibility_are_computed() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                .hidden { display: none; }
                .ghost { visibility: hidden; }
                span { display: block; }
                a { display: inline-block; }
            </style><body><p class=\"hidden\">Gone</p><div class=\"ghost\">Ghost</div><span>Block span</span><a href=\"/\">Box link</a></body>",
        )?;
        let styled = style_document(&document);

        assert!(find_text(&styled, "Gone").is_err());
        assert_eq!(
            find_first_tag(&styled, "div")?.style.visibility,
            Visibility::Hidden
        );
        assert_eq!(
            find_first_tag(&styled, "span")?.style.display,
            Display::Block
        );
        assert_eq!(
            find_first_tag(&styled, "a")?.style.display,
            Display::InlineBlock
        );
        Ok(())
    }

    #[test]
    fn css_overflow_shorthand_and_axis_overrides_are_computed() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>.clip { overflow: hidden; overflow-y: auto; }</style><div class=\"clip\">Text</div>",
        )?;
        let stylesheet = compose_document_stylesheet(&document, &[]);
        let styled = style_document_with_css(&document, &stylesheet);
        let div = find_first_tag(&styled, "div")?;

        assert_eq!(div.style.overflow_x, Overflow::Hidden);
        assert_eq!(div.style.overflow_y, Overflow::Auto);
        assert!(dump_style_tree(&styled).contains("overflow-x=hidden overflow-y=auto"));
        Ok(())
    }

    #[test]
    fn css_position_and_offsets_are_computed() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                section { position: relative; top: 5px; left: 10%; }
                aside { position: absolute; right: 7px; bottom: auto; }
            </style><body><section>Relative</section><aside>Absolute</aside></body>",
        )?;
        let styled = style_document(&document);

        let relative = find_first_tag(&styled, "section")?;
        let absolute = find_first_tag(&styled, "aside")?;
        assert_eq!(relative.style.position, Position::Relative);
        assert_eq!(relative.style.top, Some(CssSize::Px(5.0)));
        assert_eq!(relative.style.left, Some(CssSize::Percent(0.1)));
        assert_eq!(absolute.style.position, Position::Absolute);
        assert_eq!(absolute.style.right, Some(CssSize::Px(7.0)));
        assert_eq!(absolute.style.bottom, None);
        Ok(())
    }

    #[test]
    fn css_flex_properties_are_computed() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                nav { display: flex; flex-direction: column; gap: 6px; justify-content: center; align-items: flex-start; }
                a { flex-grow: 2; }
                span { flex: 1; }
            </style><body><nav><a href=\"/\">Home</a><span>More</span></nav></body>",
        )?;
        let styled = style_document(&document);
        let nav = find_first_tag(&styled, "nav")?;
        let link = find_first_tag(&styled, "a")?;
        let span = find_first_tag(&styled, "span")?;

        assert_eq!(nav.style.display, Display::Flex);
        assert_eq!(nav.style.flex_direction, FlexDirection::Column);
        assert_eq!(nav.style.gap, 6.0);
        assert_eq!(nav.style.justify_content, JustifyContent::Center);
        assert_eq!(nav.style.align_items, AlignItems::FlexStart);
        assert_eq!(link.style.flex_grow, 2.0);
        assert_eq!(span.style.flex_grow, 1.0);
        Ok(())
    }

    #[test]
    fn css_grid_properties_are_computed() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                main { display: grid; grid-template-columns: 100px 1fr auto; grid-template-rows: 40px auto; gap: 6px; row-gap: 8px; column-gap: 10px; }
                aside { grid-column: 2 / 4; grid-row: 1; }
            </style><body><main><section>One</section><aside>Two</aside></main></body>",
        )?;
        let styled = style_document(&document);
        let main = find_first_tag(&styled, "main")?;
        let aside = find_first_tag(&styled, "aside")?;

        assert_eq!(main.style.display, Display::Grid);
        assert_eq!(
            main.style.grid_template_columns,
            vec![GridTrack::Px(100.0), GridTrack::Fr(1.0), GridTrack::Auto]
        );
        assert_eq!(
            main.style.grid_template_rows,
            vec![GridTrack::Px(40.0), GridTrack::Auto]
        );
        assert_eq!(main.style.gap, 6.0);
        assert_eq!(main.style.row_gap, 8.0);
        assert_eq!(main.style.column_gap, 10.0);
        assert_eq!(
            aside.style.grid_column,
            GridPlacement {
                start: Some(2),
                end: Some(4)
            }
        );
        assert_eq!(
            aside.style.grid_row,
            GridPlacement {
                start: Some(1),
                end: None
            }
        );
        Ok(())
    }

    #[test]
    fn transition_properties_are_computed_and_dumped() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>a { transition: color 200ms linear 50ms; }</style><a href=\"/\">Link</a>",
        )?;
        let styled = style_document(&document);
        let link = find_first_tag(&styled, "a")?;

        assert_eq!(
            link.style.transition.properties,
            vec![TransitionProperty::Color]
        );
        assert_eq!(link.style.transition.duration_ms, 200);
        assert_eq!(link.style.transition.delay_ms, 50);
        assert_eq!(
            link.style.transition.timing_function,
            TransitionTimingFunction::Linear
        );
        assert!(
            dump_style_tree(&styled).contains("transition=color 200ms delay=50ms timing=linear")
        );
        Ok(())
    }

    #[test]
    fn media_queries_match_viewport_width_and_order() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                p { color: red; }
                @media screen and (min-width: 600px) { p { color: blue; } }
                @media screen and (max-width: 500px) { p { font-size: 20px; } }
            </style><body><p>Responsive</p></body>",
        )?;
        let stylesheet = compose_document_stylesheet(&document, &[]);
        let wide = style_document_with_css_for_viewport(&document, &stylesheet, 800.0, 600.0);
        let narrow = style_document_with_css_for_viewport(&document, &stylesheet, 400.0, 600.0);

        assert_eq!(
            find_first_tag(&wide, "p")?.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        assert_eq!(find_first_tag(&wide, "p")?.style.font_size, 16.0);
        assert_eq!(find_first_tag(&narrow, "p")?.style.color.r, 255);
        assert_eq!(find_first_tag(&narrow, "p")?.style.font_size, 20.0);
        Ok(())
    }

    #[test]
    fn media_queries_match_orientation() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                @media (orientation: landscape) { p { font-weight: bold; } }
                @media (orientation: portrait) { p { text-decoration: underline; } }
            </style><body><p>Responsive</p></body>",
        )?;
        let stylesheet = compose_document_stylesheet(&document, &[]);
        let landscape = style_document_with_css_for_viewport(&document, &stylesheet, 800.0, 600.0);
        let portrait = style_document_with_css_for_viewport(&document, &stylesheet, 320.0, 600.0);

        assert_eq!(
            find_first_tag(&landscape, "p")?.style.font_weight,
            FontWeight::Bold
        );
        assert_eq!(
            find_first_tag(&portrait, "p")?.style.text_decoration,
            TextDecoration::Underline
        );
        Ok(())
    }

    #[test]
    fn default_display_values_cover_common_document_structure() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<body><header>Head</header><nav>Nav</nav><main><article><section>Section</section></article></main><aside>Aside</aside><footer>Foot</footer><figure><figcaption>Caption</figcaption></figure></body>",
        )?;
        let styled = style_document(&document);

        for tag in [
            "header",
            "nav",
            "main",
            "article",
            "section",
            "aside",
            "footer",
            "figure",
            "figcaption",
        ] {
            assert_eq!(find_first_tag(&styled, tag)?.style.display, Display::Block);
        }
        Ok(())
    }

    #[test]
    fn default_richer_text_element_styles_exist() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<body><ul><li>One</li></ul><ol><li>Two</li></ol><p><strong>Bold</strong><em>Em</em><small>Small</small><code>code</code></p><pre>  let x = 1;</pre><blockquote>Quote</blockquote><hr></body>",
        )?;
        let styled = style_document(&document);

        assert_eq!(find_first_tag(&styled, "ul")?.style.display, Display::Block);
        assert!(find_first_tag(&styled, "ul")?.style.padding.left > 0.0);
        assert_eq!(
            find_first_tag(&styled, "strong")?.style.font_weight,
            FontWeight::Bold
        );
        assert!(find_first_tag(&styled, "small")?.style.font_size < 16.0);
        assert_eq!(
            find_first_tag(&styled, "pre")?.style.display,
            Display::Block
        );
        assert_eq!(
            find_first_tag(&styled, "code")?.style.font_family,
            FontFamily::Monospace
        );
        assert_eq!(
            find_first_tag(&styled, "pre")?.style.font_family,
            FontFamily::Monospace
        );
        assert_eq!(
            find_first_tag(&styled, "pre")?.style.white_space,
            WhiteSpace::Pre
        );
        assert!(find_first_tag(&styled, "pre")?.style.padding.left > 0.0);
        assert!(find_first_tag(&styled, "blockquote")?.style.border.width > 0.0);
        assert!(find_first_tag(&styled, "hr")?.style.border.width > 0.0);
        Ok(())
    }

    #[test]
    fn css_text_properties_are_computed() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                p {
                    font-family: monospace;
                    font-size: 20px;
                    line-height: 28px;
                    text-align: right;
                    white-space: nowrap;
                }
            </style><body><p>Text</p></body>",
        )?;
        let styled = style_document(&document);
        let paragraph = find_first_tag(&styled, "p")?;

        assert_eq!(paragraph.style.font_family, FontFamily::Monospace);
        assert_eq!(paragraph.style.font_size, 20.0);
        assert_eq!(paragraph.style.line_height, Some(28.0));
        assert_eq!(paragraph.style.text_align, TextAlign::Right);
        assert_eq!(paragraph.style.white_space, WhiteSpace::NoWrap);
        Ok(())
    }

    #[test]
    fn default_table_styles_distinguish_headers_and_cells() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<body><table><thead><tr><th>Name</th></tr></thead><tbody><tr><td>Webby</td></tr></tbody></table></body>",
        )?;
        let styled = style_document(&document);
        let table = find_first_tag(&styled, "table")?;
        let thead = find_first_tag(&styled, "thead")?;
        let row = find_first_tag(&styled, "tr")?;
        let header = find_first_tag(&styled, "th")?;
        let cell = find_first_tag(&styled, "td")?;

        assert_eq!(table.style.display, Display::Block);
        assert_eq!(thead.style.display, Display::Block);
        assert_eq!(row.style.display, Display::Block);
        assert_eq!(header.style.display, Display::Block);
        assert_eq!(cell.style.display, Display::Block);
        assert_eq!(header.style.font_weight, FontWeight::Bold);
        assert!(header.style.border.width > 0.0);
        assert!(cell.style.border.width > 0.0);
        assert_ne!(header.style.background_color, cell.style.background_color);
        Ok(())
    }

    #[test]
    fn specificity_ordering_overrides_lower_specificity() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                p { color: red; }
                .note { color: blue; }
                #intro { color: green; }
            </style><p id=\"intro\" class=\"note\">Text</p>",
        )?;
        let styled = style_document(&document);
        let paragraph = find_first_tag(&styled, "p")?;

        assert_eq!(
            paragraph.style.color,
            Color {
                r: 0,
                g: 128,
                b: 0,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn tag_class_specificity_beats_plain_class() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                .note { color: red; }
                p.note { color: blue; }
            </style><p class=\"note\">Text</p>",
        )?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "p")?.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn tag_id_specificity_beats_plain_id() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                #intro { color: red; }
                p#intro { color: blue; }
            </style><p id=\"intro\">Text</p>",
        )?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "p")?.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn descendant_selector_matches_nested_element() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>.card p { color: red; }</style><div class=\"card\"><section><p>Hit</p></section></div><p>Miss</p>",
        )?;
        let styled = style_document(&document);

        assert_eq!(find_text(&styled, "Hit")?.style.color, red());
        assert_eq!(find_text(&styled, "Miss")?.style.color, Color::BLACK);
        Ok(())
    }

    #[test]
    fn child_selector_matches_only_direct_child() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>.card > p { color: red; }</style><div class=\"card\"><p>Hit</p><section><p>Miss</p></section></div>",
        )?;
        let styled = style_document(&document);

        assert_eq!(find_text(&styled, "Hit")?.style.color, red());
        assert_eq!(find_text(&styled, "Miss")?.style.color, Color::BLACK);
        Ok(())
    }

    #[test]
    fn grouped_and_universal_selectors_apply() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>* { color: red; } h1, h2, h3 { font-size: 22px; }</style><body><h1>One</h1><h2>Two</h2><p>Para</p></body>",
        )?;
        let styled = style_document(&document);

        assert_eq!(find_text(&styled, "One")?.style.color, red());
        assert_eq!(find_first_tag(&styled, "h1")?.style.font_size, 22.0);
        assert_eq!(find_first_tag(&styled, "h2")?.style.font_size, 22.0);
        assert_eq!(find_text(&styled, "Para")?.style.color, red());
        Ok(())
    }

    #[test]
    fn multiple_classes_require_all_classes() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>.card.highlighted { color: red; }</style><p class=\"card highlighted\">Hit</p><p class=\"card\">Miss</p>",
        )?;
        let styled = style_document(&document);

        assert_eq!(find_text(&styled, "Hit")?.style.color, red());
        assert_eq!(find_text(&styled, "Miss")?.style.color, Color::BLACK);
        Ok(())
    }

    #[test]
    fn attribute_selectors_match_existence_and_equality() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>a[href] { color: red; } input[type=\"text\"] { background-color: blue; }</style><a href=\"/\">Link</a><input type=\"text\"><input type=\"submit\">",
        )?;
        let styled = style_document(&document);

        assert_eq!(find_first_tag(&styled, "a")?.style.color, red());
        let inputs = collect_tags(&styled, "input");
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].style.background_color, blue());
        assert_ne!(inputs[1].style.background_color, blue());
        Ok(())
    }

    #[test]
    fn hover_and_focus_pseudo_classes_use_interaction_context() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>a:hover { color: red; } input:focus { background-color: blue; }</style><a href=\"/\">Link</a><input type=\"text\">",
        )?;
        let stylesheet = compose_document_stylesheet(&document, &[]);
        let base = style_document_with_css(&document, &stylesheet);
        let link_id = find_first_tag(&base, "a")?.node.id;
        let input_id = find_first_tag(&base, "input")?.node.id;
        let context = StyleContext::default().with_interaction(
            StyleInteraction::new()
                .with_hovered_node(link_id)
                .with_focused_node(input_id),
        );

        let styled = style_document_with_css_and_context(&document, &stylesheet, context);

        assert_eq!(find_first_tag(&styled, "a")?.style.color, red());
        assert_eq!(
            find_first_tag(&styled, "input")?.style.background_color,
            blue()
        );
        Ok(())
    }

    #[test]
    fn checked_disabled_and_child_pseudo_classes_match_dom_state() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                input:checked { background-color: red; }
                button:disabled { color: blue; }
                li:first-child { color: red; }
                li:last-child { background-color: blue; }
                li:nth-child(2) { font-weight: bold; }
            </style>
            <input type=\"checkbox\" checked>
            <button disabled>Off</button>
            <ul><li>One</li><li>Two</li><li>Three</li></ul>",
        )?;
        let styled = style_document(&document);
        let items = collect_tags(&styled, "li");

        assert_eq!(
            find_first_tag(&styled, "input")?.style.background_color,
            red()
        );
        assert_eq!(find_first_tag(&styled, "button")?.style.color, blue());
        assert_eq!(items[0].style.color, red());
        assert_eq!(items[1].style.font_weight, FontWeight::Bold);
        assert_eq!(items[2].style.background_color, blue());
        Ok(())
    }

    #[test]
    fn specificity_for_combinators_attributes_and_ids_is_correct() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                .card p[type=\"text\"] { color: red; }
                section > p.note { color: blue; }
                #intro { color: green; }
            </style><section class=\"card\"><p id=\"intro\" class=\"note\" type=\"text\">Text</p></section>",
        )?;
        let styled = style_document(&document);

        assert_eq!(find_first_tag(&styled, "p")?.style.color, green());
        Ok(())
    }

    #[test]
    fn later_equal_specificity_rule_overrides_earlier_rule() -> webby_core::WebbyResult<()> {
        let document =
            parse_document("<style>p { color: red; } p { color: blue; }</style><p>Text</p>")?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "p")?.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn later_equal_specificity_complex_rule_wins() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>.card p.note { color: red; } div.card .note { color: blue; }</style><div class=\"card\"><p class=\"note\">Text</p></div>",
        )?;
        let styled = style_document(&document);

        assert_eq!(find_first_tag(&styled, "p")?.style.color, blue());
        Ok(())
    }

    #[test]
    fn inline_style_overrides_stylesheet_rules() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>#intro { color: red; font-size: 30px; }</style><p id=\"intro\" style=\"color: blue; font-size: 12px;\">Text</p>",
        )?;
        let styled = style_document(&document);
        let paragraph = find_first_tag(&styled, "p")?;

        assert_eq!(
            paragraph.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        assert_eq!(paragraph.style.font_size, 12.0);
        Ok(())
    }

    #[test]
    fn external_stylesheets_feed_computed_style_in_document_order() -> webby_core::WebbyResult<()> {
        let document = parse_document("<p class=\"note\">Text</p>")?;
        let first = webby_css::parse_stylesheet(".note { color: red; }")?;
        let second = webby_css::parse_stylesheet(".note { color: blue; }")?;
        let stylesheet = compose_document_stylesheet(&document, &[first, second]);
        let styled = style_document_with_css(&document, &stylesheet);

        assert_eq!(
            find_first_tag(&styled, "p")?.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn style_blocks_override_earlier_external_rules_with_equal_specificity()
    -> webby_core::WebbyResult<()> {
        let document = parse_document("<style>p { color: blue; }</style><p>Text</p>")?;
        let external = webby_css::parse_stylesheet("p { color: red; }")?;
        let stylesheet = compose_document_stylesheet(&document, &[external]);
        let styled = style_document_with_css(&document, &stylesheet);

        assert_eq!(
            find_first_tag(&styled, "p")?.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn inline_styles_override_external_stylesheets() -> webby_core::WebbyResult<()> {
        let document = parse_document("<p style=\"color: blue;\">Text</p>")?;
        let external = webby_css::parse_stylesheet("p { color: red; }")?;
        let stylesheet = compose_document_stylesheet(&document, &[external]);
        let styled = style_document_with_css(&document, &stylesheet);

        assert_eq!(
            find_first_tag(&styled, "p")?.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn background_shorthand_applies_background_color() -> webby_core::WebbyResult<()> {
        let document =
            parse_document("<style>body { background: blue; }</style><body>Text</body>")?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "body")?.style.background_color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn style_closing_lookalikes_do_not_truncate_css() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>
                p.note { color: red; }
                p::before { content: \"</stylesheet>\"; }
                p::after { content: \"</stylefoo>\"; }
                p.note { color: blue; }
            </style><p class=\"note\">Text</p>",
        )?;
        let styled = style_document(&document);

        assert_eq!(
            find_first_tag(&styled, "p")?.style.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
        Ok(())
    }

    #[test]
    fn style_block_content_is_not_visible_in_style_tree() -> webby_core::WebbyResult<()> {
        let document = parse_document("<style>p { color: red; }</style><p>Visible</p>")?;
        let styled = style_document(&document);
        let dump = dump_style_tree(&styled);

        assert!(dump.contains("\"Visible\""));
        assert!(!dump.contains("color: red"));
        assert!(!dump.contains("<style>"));
        Ok(())
    }

    #[test]
    fn inherited_text_properties_flow_to_nested_elements() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>.parent { color: red; font-family: monospace; font-size: 22px; font-weight: bold; line-height: 28px; text-align: right; visibility: hidden; }</style><div class=\"parent\"><span>Text</span></div>",
        )?;
        let styled = style_document(&document);
        let span = find_first_tag(&styled, "span")?;

        assert_eq!(span.style.color, red());
        assert_eq!(span.style.font_family, FontFamily::Monospace);
        assert_eq!(span.style.font_size, 22.0);
        assert_eq!(span.style.font_weight, FontWeight::Bold);
        assert_eq!(span.style.line_height, Some(28.0));
        assert_eq!(span.style.text_align, TextAlign::Right);
        assert_eq!(span.style.visibility, Visibility::Hidden);
        Ok(())
    }

    #[test]
    fn css_wide_keywords_resolve_against_parent_or_initial_values() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>.parent { color: red; font-size: 22px; visibility: hidden; padding: 9px; } .child { color: inherit; font-size: unset; visibility: unset; padding: inherit; margin: initial; display: initial; }</style><div class=\"parent\"><div class=\"child\">Text</div></div>",
        )?;
        let styled = style_document(&document);
        let divs = collect_tags(&styled, "div");
        let child = divs
            .get(1)
            .copied()
            .ok_or_else(|| webby_core::WebbyError::invalid_input("missing child div"))?;

        assert_eq!(child.style.color, red());
        assert_eq!(child.style.font_size, 22.0);
        assert_eq!(child.style.visibility, Visibility::Hidden);
        assert_eq!(child.style.padding, super::Edges::trbl(9.0, 9.0, 9.0, 9.0));
        assert_eq!(child.style.margin, super::Edges::ZERO);
        assert_eq!(child.style.display, Display::Inline);
        Ok(())
    }

    #[test]
    fn important_priority_beats_normal_inline_and_inline_important_wins_ties()
    -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>p { color: red !important; } #normal { color: green; }</style><p id=\"normal\" style=\"color: blue\">One</p><p id=\"important\" style=\"color: blue !important\">Two</p>",
        )?;
        let styled = style_document(&document);
        let paragraphs = collect_tags(&styled, "p");

        assert_eq!(paragraphs[0].style.color, red());
        assert_eq!(paragraphs[1].style.color, blue());
        Ok(())
    }

    #[test]
    fn malformed_inline_style_diagnostic_is_retained_by_composition() -> webby_core::WebbyResult<()>
    {
        let document = parse_document("<p style=\"color: invalid\">Text</p>")?;
        let stylesheet = compose_document_stylesheet(&document, &[]);

        assert_eq!(stylesheet.diagnostics.len(), 1);
        assert!(
            stylesheet.diagnostics[0]
                .message
                .contains("inline style on <p>: unsupported or invalid declaration")
        );
        Ok(())
    }

    fn find_first_tag<'a>(
        node: &'a super::StyledNode<'a>,
        tag_name: &str,
    ) -> webby_core::WebbyResult<&'a super::StyledNode<'a>> {
        if node.tag_name() == Some(tag_name) {
            return Ok(node);
        }

        for child in &node.children {
            if let Ok(found) = find_first_tag(child, tag_name) {
                return Ok(found);
            }
        }

        Err(webby_core::WebbyError::Parse {
            message: format!("expected styled tag {tag_name}"),
        })
    }

    fn find_first_text<'a>(
        node: &'a super::StyledNode<'a>,
        text: &str,
    ) -> webby_core::WebbyResult<&'a super::StyledNode<'a>> {
        if node.text() == Some(text) {
            return Ok(node);
        }

        for child in &node.children {
            if let Ok(found) = find_first_text(child, text) {
                return Ok(found);
            }
        }

        Err(webby_core::WebbyError::Parse {
            message: format!("expected styled text {text}"),
        })
    }

    fn find_text<'a>(
        node: &'a super::StyledNode<'a>,
        text: &str,
    ) -> webby_core::WebbyResult<&'a super::StyledNode<'a>> {
        find_first_text(node, text)
    }

    fn collect_tags<'a>(
        node: &'a super::StyledNode<'a>,
        tag_name: &str,
    ) -> Vec<&'a super::StyledNode<'a>> {
        let mut matches = Vec::new();
        collect_tags_from_node(node, tag_name, &mut matches);
        matches
    }

    fn collect_tags_from_node<'a>(
        node: &'a super::StyledNode<'a>,
        tag_name: &str,
        matches: &mut Vec<&'a super::StyledNode<'a>>,
    ) {
        if node.tag_name() == Some(tag_name) {
            matches.push(node);
        }
        for child in &node.children {
            collect_tags_from_node(child, tag_name, matches);
        }
    }

    fn red() -> Color {
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    fn blue() -> Color {
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        }
    }

    fn green() -> Color {
        Color {
            r: 0,
            g: 128,
            b: 0,
            a: 255,
        }
    }
}
