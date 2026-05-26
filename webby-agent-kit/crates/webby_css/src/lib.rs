//! Small forgiving CSS parser for Webby's early style cascade.
//!
//! Webby intentionally supports a compact CSS subset. Invalid rules,
//! unsupported selectors, and unsupported declarations are skipped with
//! diagnostics so malformed CSS cannot stop page rendering.

use webby_core::WebbyResult;

/// Parsed stylesheet with source-order-preserving rules.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stylesheet {
    /// Parsed rules in source order.
    pub rules: Vec<CssRule>,
    /// Non-fatal parse diagnostics for skipped CSS.
    pub diagnostics: Vec<CssDiagnostic>,
}

/// Non-fatal CSS parser diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssDiagnostic {
    /// Byte offset where the parser noticed the issue.
    pub offset: usize,
    /// Human-readable diagnostic.
    pub message: String,
}

/// Structured CSS rule.
#[derive(Debug, Clone, PartialEq)]
pub struct CssRule {
    /// Selector.
    pub selector: Selector,
    /// Optional media query gating this rule.
    pub media: Option<MediaQuery>,
    /// Supported declarations in source order.
    pub declarations: Vec<Declaration>,
}

/// Supported `@media` query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaQuery {
    /// Media type.
    pub media_type: MediaType,
    /// Required media features.
    pub features: Vec<MediaFeature>,
}

/// Supported media types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    /// `all`
    All,
    /// `screen`
    Screen,
}

/// Supported media features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFeature {
    /// `(min-width: Npx)`
    MinWidth(u32),
    /// `(max-width: Npx)`
    MaxWidth(u32),
    /// `(width: Npx)`
    Width(u32),
    /// `(orientation: portrait|landscape)`
    Orientation(Orientation),
}

/// Supported media orientation values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// Height is greater than or equal to width.
    Portrait,
    /// Width is greater than height.
    Landscape,
}

/// Supported selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// Compound selector parts from left to right.
    pub parts: Vec<SelectorPart>,
    /// Combinators between adjacent parts.
    pub combinators: Vec<Combinator>,
}

impl Selector {
    /// Specificity tuple: ids, classes/attributes, tags.
    pub fn specificity(&self) -> Specificity {
        let mut specificity = Specificity::default();
        for part in &self.parts {
            specificity.ids = specificity.ids.saturating_add(u16::from(part.id.is_some()));
            specificity.classes = specificity.classes.saturating_add(
                u16::try_from(
                    part.classes
                        .len()
                        .saturating_add(part.attributes.len())
                        .saturating_add(part.pseudo_classes.len()),
                )
                .unwrap_or(u16::MAX),
            );
            specificity.tags = specificity
                .tags
                .saturating_add(u16::from(part.tag_name.is_some()));
        }
        specificity
    }
}

/// Supported combinators between compound selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// Whitespace descendant combinator.
    Descendant,
    /// `>` child combinator.
    Child,
}

/// One compound selector component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorPart {
    /// Universal selector marker.
    pub universal: bool,
    /// Optional tag selector, normalized to lowercase ASCII.
    pub tag_name: Option<String>,
    /// Required class selectors.
    pub classes: Vec<String>,
    /// Optional id selector.
    pub id: Option<String>,
    /// Required attribute selectors.
    pub attributes: Vec<AttributeSelector>,
    /// Required pseudo-class selectors.
    pub pseudo_classes: Vec<PseudoClass>,
}

/// Supported attribute selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeSelector {
    /// Attribute name.
    pub name: String,
    /// Exact value required when present.
    pub value: Option<String>,
}

/// Supported pseudo-class selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoClass {
    /// `:hover`
    Hover,
    /// `:focus`
    Focus,
    /// `:active`
    Active,
    /// `:checked`
    Checked,
    /// `:disabled`
    Disabled,
    /// `:first-child`
    FirstChild,
    /// `:last-child`
    LastChild,
    /// `:nth-child(n)` with a positive integer.
    NthChild(u32),
}

/// CSS specificity used by the cascade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Specificity {
    /// Number of id selectors.
    pub ids: u16,
    /// Number of class selectors.
    pub classes: u16,
    /// Number of tag selectors.
    pub tags: u16,
}

/// Supported declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// Declaration property.
    pub property: Property,
    /// Parsed declaration value.
    pub value: Value,
}

/// Supported CSS properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    /// `color`
    Color,
    /// `background` shorthand when it contains a supported color.
    Background,
    /// `background-color`
    BackgroundColor,
    /// `font-size`
    FontSize,
    /// `font-family`
    FontFamily,
    /// `font-weight`
    FontWeight,
    /// `line-height`
    LineHeight,
    /// `text-align`
    TextAlign,
    /// `white-space`
    WhiteSpace,
    /// `text-decoration`
    TextDecoration,
    /// `display`
    Display,
    /// `visibility`
    Visibility,
    /// `margin`
    Margin,
    /// `padding`
    Padding,
    /// `border` shorthand with supported width/color values.
    Border,
    /// `border-width`
    BorderWidth,
    /// `border-color`
    BorderColor,
    /// `width`
    Width,
    /// `height`
    Height,
    /// `min-width`
    MinWidth,
    /// `max-width`
    MaxWidth,
    /// `min-height`
    MinHeight,
    /// `max-height`
    MaxHeight,
    /// `box-sizing`
    BoxSizing,
    /// `position`
    Position,
    /// `top`
    Top,
    /// `right`
    Right,
    /// `bottom`
    Bottom,
    /// `left`
    Left,
    /// `flex-direction`
    FlexDirection,
    /// `gap`
    Gap,
    /// `row-gap`
    RowGap,
    /// `column-gap`
    ColumnGap,
    /// `grid-template-columns`
    GridTemplateColumns,
    /// `grid-template-rows`
    GridTemplateRows,
    /// `grid-column`
    GridColumn,
    /// `grid-row`
    GridRow,
    /// `justify-content`
    JustifyContent,
    /// `align-items`
    AlignItems,
    /// `flex-grow`
    FlexGrow,
    /// `flex`
    Flex,
    /// `transition-property`
    TransitionProperty,
    /// `transition-duration`
    TransitionDuration,
    /// `transition-delay`
    TransitionDelay,
    /// `transition-timing-function`
    TransitionTimingFunction,
    /// `transition` shorthand.
    Transition,
}

/// Parsed declaration value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// RGBA color.
    Color(Color),
    /// CSS pixel length.
    Length(Length),
    /// CSS sizing value.
    Size(Size),
    /// Four-sided edge value.
    Edges(Edges),
    /// Font weight.
    FontWeight(FontWeight),
    /// Font family category.
    FontFamily(FontFamily),
    /// Line height value.
    LineHeight(LineHeight),
    /// Text alignment.
    TextAlign(TextAlign),
    /// White-space behavior.
    WhiteSpace(WhiteSpace),
    /// Text decoration.
    TextDecoration(TextDecoration),
    /// Display value.
    Display(Display),
    /// Visibility value.
    Visibility(Visibility),
    /// Border shorthand result.
    Border(BorderValue),
    /// Box sizing mode.
    BoxSizing(BoxSizing),
    /// Positioning mode.
    Position(Position),
    /// Flex direction.
    FlexDirection(FlexDirection),
    /// Justify content.
    JustifyContent(JustifyContent),
    /// Align items.
    AlignItems(AlignItems),
    /// Grid track list.
    GridTrackList(Vec<GridTrack>),
    /// Grid placement.
    GridPlacement(GridPlacement),
    /// Numeric value.
    Number(f32),
    /// Transition property list.
    TransitionProperties(Vec<TransitionProperty>),
    /// Transition duration/delay in milliseconds.
    TimeMs(u32),
    /// Transition timing function.
    TransitionTimingFunction(TransitionTimingFunction),
    /// Transition shorthand.
    Transition(TransitionValue),
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

/// CSS pixel length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Length {
    /// Pixel value.
    pub px: f32,
}

/// CSS font family category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    /// Default sans/proportional family.
    Sans,
    /// Deterministic monospace family.
    Monospace,
}

/// CSS line-height value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    /// Browser normal line-height.
    Normal,
    /// Pixel line-height.
    Px(f32),
}

/// CSS text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    /// Start/left alignment.
    Left,
    /// Center alignment.
    Center,
    /// Right alignment.
    Right,
}

/// CSS white-space behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    /// Collapse whitespace and wrap normally.
    Normal,
    /// Preserve spaces/newlines.
    Pre,
    /// Collapse whitespace but avoid wrapping.
    NoWrap,
}

/// CSS positioning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Normal flow.
    Static,
    /// Normal flow with visual offset.
    Relative,
    /// Removed from normal flow and placed against a containing block.
    Absolute,
}

/// Flex main-axis direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    /// Left-to-right row.
    Row,
    /// Top-to-bottom column.
    Column,
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

/// CSS size value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    /// `auto`
    Auto,
    /// CSS pixel value.
    Px(f32),
    /// Percentage value, stored as a 0.0-1.0 ratio.
    Percent(f32),
}

/// Four-sided edge value.
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

/// Font weight value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// `normal`
    Normal,
    /// `bold`
    Bold,
}

/// Text decoration value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecoration {
    /// `none`
    None,
    /// `underline`
    Underline,
}

/// Display value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// `block`
    Block,
    /// `inline`
    Inline,
    /// `inline-block`
    InlineBlock,
    /// `flex`
    Flex,
    /// `grid`
    Grid,
    /// `none`
    None,
}

/// Supported CSS grid track sizing.
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

/// Supported grid line placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridPlacement {
    /// 1-based start grid line.
    pub start: Option<usize>,
    /// 1-based end grid line.
    pub end: Option<usize>,
}

/// Supported transition target property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionProperty {
    /// `all`
    All,
    /// `color`
    Color,
    /// `background-color`/`background`
    BackgroundColor,
    /// `left`
    Left,
    /// `top`
    Top,
}

/// Supported transition timing functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionTimingFunction {
    /// Linear interpolation.
    Linear,
    /// Deterministic smoothstep-like easing.
    Ease,
}

/// Parsed single-transition shorthand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionValue {
    /// Property list from the shorthand.
    pub properties: Vec<TransitionProperty>,
    /// Duration in milliseconds.
    pub duration_ms: u32,
    /// Delay in milliseconds.
    pub delay_ms: u32,
    /// Timing function.
    pub timing_function: TransitionTimingFunction,
}

/// Visibility value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// `visible`
    Visible,
    /// `hidden`
    Hidden,
}

/// Box sizing value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSizing {
    /// Width/height describe content box.
    ContentBox,
    /// Width/height describe border box.
    BorderBox,
}

/// Parsed border shorthand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderValue {
    /// Optional border width.
    pub width: Option<Length>,
    /// Optional border color.
    pub color: Option<Color>,
}

/// Parses a stylesheet.
pub fn parse_stylesheet(input: &str) -> WebbyResult<Stylesheet> {
    let mut parser = StylesheetParser::new(input);
    Ok(parser.parse())
}

/// Parses a declaration list, useful for inline `style=""` attributes.
pub fn parse_declarations(input: &str) -> WebbyResult<(Vec<Declaration>, Vec<CssDiagnostic>)> {
    let mut diagnostics = Vec::new();
    let declarations = parse_declaration_block(input, 0, &mut diagnostics);
    Ok((declarations, diagnostics))
}

struct StylesheetParser<'a> {
    input: &'a str,
    cursor: usize,
    offset_base: usize,
    media: Option<MediaQuery>,
    stylesheet: Stylesheet,
}

impl<'a> StylesheetParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            cursor: 0,
            offset_base: 0,
            media: None,
            stylesheet: Stylesheet::default(),
        }
    }

    fn nested(input: &'a str, offset_base: usize, media: Option<MediaQuery>) -> Self {
        Self {
            input,
            cursor: 0,
            offset_base,
            media,
            stylesheet: Stylesheet::default(),
        }
    }

    fn parse(&mut self) -> Stylesheet {
        while self.cursor < self.input.len() {
            self.skip_ignored();
            if self.cursor >= self.input.len() {
                break;
            }

            let selector_start = self.offset_base + self.cursor;
            let Some(open_brace) = self.input[self.cursor..].find('{') else {
                self.diagnostic(selector_start, "skipped CSS without declaration block");
                break;
            };
            let selector_text = &self.input[self.cursor..self.cursor + open_brace];
            self.cursor += open_brace + 1;

            let block_start = self.offset_base + self.cursor;
            let block_text = self.take_block(block_start);

            if selector_text.trim_start().starts_with("@media") {
                self.parse_media_block(selector_text, selector_start, block_text, block_start);
                continue;
            }

            let selectors =
                parse_selector_list(selector_text, selector_start, &mut self.stylesheet);
            let declarations =
                parse_declaration_block(block_text, block_start, &mut self.stylesheet.diagnostics);
            if declarations.is_empty() {
                continue;
            }
            for selector in selectors {
                self.stylesheet.rules.push(CssRule {
                    selector,
                    media: self.media.clone(),
                    declarations: declarations.clone(),
                });
            }
        }

        std::mem::take(&mut self.stylesheet)
    }

    fn take_block(&mut self, block_start: usize) -> &'a str {
        let start = self.cursor;
        let mut depth = 0usize;
        for (relative, character) in self.input[start..].char_indices() {
            match character {
                '{' => depth = depth.saturating_add(1),
                '}' if depth == 0 => {
                    self.cursor = start + relative + 1;
                    return &self.input[start..start + relative];
                }
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        self.diagnostic(block_start, "unterminated CSS declaration block");
        self.cursor = self.input.len();
        &self.input[start..]
    }

    fn parse_media_block(
        &mut self,
        prelude: &str,
        prelude_offset: usize,
        block_text: &'a str,
        block_offset: usize,
    ) {
        let media_text = prelude.trim_start().trim_start_matches("@media").trim();
        let Some(media) = parse_media_query(media_text) else {
            self.stylesheet.diagnostics.push(CssDiagnostic {
                offset: prelude_offset,
                message: format!("unsupported media query {media_text:?}"),
            });
            return;
        };
        let mut nested = Self::nested(block_text, block_offset, Some(media));
        let parsed = nested.parse();
        self.stylesheet.rules.extend(parsed.rules);
        self.stylesheet.diagnostics.extend(parsed.diagnostics);
    }

    fn skip_ignored(&mut self) {
        loop {
            let before = self.cursor;
            self.cursor += leading_whitespace_len(&self.input[self.cursor..]);

            if self.input[self.cursor..].starts_with("/*") {
                let comment_start = self.cursor;
                match self.input[self.cursor + 2..].find("*/") {
                    Some(end) => self.cursor += end + 4,
                    None => {
                        self.diagnostic(comment_start, "unterminated CSS comment");
                        self.cursor = self.input.len();
                    }
                }
            }

            if self.cursor == before {
                break;
            }
        }
    }

    fn diagnostic(&mut self, offset: usize, message: &str) {
        self.stylesheet.diagnostics.push(CssDiagnostic {
            offset,
            message: message.to_string(),
        });
    }
}

fn parse_media_query(input: &str) -> Option<MediaQuery> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut media_type = MediaType::All;
    let mut features = Vec::new();

    for (index, raw_part) in split_media_and(trimmed).into_iter().enumerate() {
        let part = raw_part.trim();
        if part.is_empty() {
            return None;
        }
        if index == 0 && !part.starts_with('(') {
            media_type = match part.to_ascii_lowercase().as_str() {
                "all" => MediaType::All,
                "screen" => MediaType::Screen,
                _ => return None,
            };
            continue;
        }
        features.push(parse_media_feature(part)?);
    }

    Some(MediaQuery {
        media_type,
        features,
    })
}

fn split_media_and(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (index, character) in input.char_indices() {
        match character {
            '(' => depth = depth.saturating_add(1),
            ')' => depth = depth.saturating_sub(1),
            'a' | 'A' if depth == 0 && is_media_and_at(input, index) => {
                parts.push(&input[start..index]);
                start = index + 3;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

fn is_media_and_at(input: &str, index: usize) -> bool {
    let tail = &input[index..];
    if tail.len() < 3 || !tail[..3].eq_ignore_ascii_case("and") {
        return false;
    }
    let before_ok = input[..index]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    let after_ok = input[index + 3..]
        .chars()
        .next()
        .is_none_or(char::is_whitespace);
    before_ok && after_ok
}

fn parse_media_feature(input: &str) -> Option<MediaFeature> {
    let inner = input.trim().strip_prefix('(')?.strip_suffix(')')?.trim();
    let colon = inner.find(':')?;
    let name = inner[..colon].trim().to_ascii_lowercase();
    let value = inner[colon + 1..].trim();
    match name.as_str() {
        "min-width" => parse_media_width(value).map(MediaFeature::MinWidth),
        "max-width" => parse_media_width(value).map(MediaFeature::MaxWidth),
        "width" => parse_media_width(value).map(MediaFeature::Width),
        "orientation" => match value.to_ascii_lowercase().as_str() {
            "portrait" => Some(MediaFeature::Orientation(Orientation::Portrait)),
            "landscape" => Some(MediaFeature::Orientation(Orientation::Landscape)),
            _ => None,
        },
        _ => None,
    }
}

fn parse_media_width(value: &str) -> Option<u32> {
    let length = parse_length(value)?;
    if length.px > u32::MAX as f32 {
        return None;
    }
    Some(length.px.round() as u32)
}

fn parse_selector_list(input: &str, offset: usize, stylesheet: &mut Stylesheet) -> Vec<Selector> {
    let mut selectors = Vec::new();
    let mut relative_offset = 0;
    for raw_selector in input.split(',') {
        let leading = leading_whitespace_len(raw_selector);
        let selector_offset = offset + relative_offset + leading;
        match parse_selector(raw_selector) {
            Some(selector) => selectors.push(selector),
            None => stylesheet.diagnostics.push(CssDiagnostic {
                offset: selector_offset,
                message: format!("unsupported selector {:?}", raw_selector.trim()),
            }),
        }
        relative_offset += raw_selector.len().saturating_add(1);
    }
    selectors
}

fn parse_selector(input: &str) -> Option<Selector> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut cursor = 0;
    let mut parts = Vec::new();
    let mut combinators = Vec::new();
    let mut pending = None;

    while cursor < trimmed.len() {
        cursor += leading_whitespace_len(&trimmed[cursor..]);
        if cursor >= trimmed.len() {
            break;
        }

        if trimmed[cursor..].starts_with('>') {
            if parts.is_empty() || pending.is_some() {
                return None;
            }
            pending = Some(Combinator::Child);
            cursor += 1;
            continue;
        }

        let start = cursor;
        let token_len = selector_token_len(&trimmed[start..])?;
        if token_len == 0 {
            return None;
        }
        let token = &trimmed[start..start + token_len];
        let part = parse_selector_part(token)?;
        if !parts.is_empty() {
            combinators.push(pending.take().unwrap_or(Combinator::Descendant));
        } else if pending.is_some() {
            return None;
        }
        parts.push(part);
        cursor = start + token_len;

        let whitespace = leading_whitespace_len(&trimmed[cursor..]);
        if whitespace > 0
            && cursor + whitespace < trimmed.len()
            && pending.is_none()
            && !trimmed[cursor + whitespace..].starts_with('>')
        {
            pending = Some(Combinator::Descendant);
        }
    }

    if parts.is_empty() || pending.is_some() {
        return None;
    }

    Some(Selector { parts, combinators })
}

fn selector_token_len(input: &str) -> Option<usize> {
    let mut in_attribute = false;
    let mut quote = None;
    for (index, character) in input.char_indices() {
        match (character, in_attribute, quote) {
            ('"' | '\'', true, None) => quote = Some(character),
            (current, true, Some(open)) if current == open => quote = None,
            ('[', false, None) => in_attribute = true,
            (']', true, None) => in_attribute = false,
            ('>' | ' ' | '\n' | '\r' | '\t' | '\x0c', false, None) => return Some(index),
            _ => {}
        }
    }
    (!in_attribute && quote.is_none()).then_some(input.len())
}

fn parse_selector_part(input: &str) -> Option<SelectorPart> {
    let mut cursor = 0;
    let mut part = SelectorPart {
        universal: false,
        tag_name: None,
        classes: Vec::new(),
        id: None,
        attributes: Vec::new(),
        pseudo_classes: Vec::new(),
    };

    if input[cursor..].starts_with('*') {
        part.universal = true;
        cursor += 1;
    } else if let Some(length) = identifier_len(&input[cursor..])
        && length > 0
    {
        part.tag_name = Some(input[cursor..cursor + length].to_ascii_lowercase());
        cursor += length;
    }

    while cursor < input.len() {
        if let Some(class_tail) = input[cursor..].strip_prefix('.') {
            let length = identifier_len(class_tail)?;
            if length == 0 {
                return None;
            }
            part.classes.push(class_tail[..length].to_string());
            cursor += 1 + length;
            continue;
        }

        if let Some(id_tail) = input[cursor..].strip_prefix('#') {
            if part.id.is_some() {
                return None;
            }
            let length = identifier_len(id_tail)?;
            if length == 0 {
                return None;
            }
            part.id = Some(id_tail[..length].to_string());
            cursor += 1 + length;
            continue;
        }

        if input[cursor..].starts_with('[') {
            let close = attribute_close_index(&input[cursor..])?;
            let attribute_text = &input[cursor + 1..cursor + close];
            part.attributes
                .push(parse_attribute_selector(attribute_text)?);
            cursor += close + 1;
            continue;
        }

        if let Some(pseudo_tail) = input[cursor..].strip_prefix(':') {
            let (pseudo_class, consumed) = parse_pseudo_class(pseudo_tail)?;
            part.pseudo_classes.push(pseudo_class);
            cursor += 1 + consumed;
            continue;
        }

        return None;
    }

    if part.universal
        || part.tag_name.is_some()
        || !part.classes.is_empty()
        || part.id.is_some()
        || !part.attributes.is_empty()
        || !part.pseudo_classes.is_empty()
    {
        Some(part)
    } else {
        None
    }
}

fn parse_pseudo_class(input: &str) -> Option<(PseudoClass, usize)> {
    let length = identifier_len(input)?;
    if length == 0 {
        return None;
    }
    let name = input[..length].to_ascii_lowercase();
    match name.as_str() {
        "hover" => Some((PseudoClass::Hover, length)),
        "focus" => Some((PseudoClass::Focus, length)),
        "active" => Some((PseudoClass::Active, length)),
        "checked" => Some((PseudoClass::Checked, length)),
        "disabled" => Some((PseudoClass::Disabled, length)),
        "first-child" => Some((PseudoClass::FirstChild, length)),
        "last-child" => Some((PseudoClass::LastChild, length)),
        "nth-child" => {
            let remaining = &input[length..];
            let inner = remaining.strip_prefix('(')?;
            let close = inner.find(')')?;
            let value = inner[..close].trim().parse::<u32>().ok()?;
            if value == 0 {
                return None;
            }
            Some((PseudoClass::NthChild(value), length + close + 2))
        }
        _ => None,
    }
}

fn attribute_close_index(input: &str) -> Option<usize> {
    let mut quote = None;
    for (index, character) in input.char_indices().skip(1) {
        match (character, quote) {
            ('"' | '\'', None) => quote = Some(character),
            (current, Some(open)) if current == open => quote = None,
            (']', None) => return Some(index),
            _ => {}
        }
    }
    None
}

fn parse_attribute_selector(input: &str) -> Option<AttributeSelector> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let Some(equals) = trimmed.find('=') else {
        return valid_identifier(trimmed).then(|| AttributeSelector {
            name: trimmed.to_ascii_lowercase(),
            value: None,
        });
    };
    let name = trimmed[..equals].trim();
    let value = trimmed[equals + 1..].trim();
    if !valid_identifier(name) || value.is_empty() {
        return None;
    }
    Some(AttributeSelector {
        name: name.to_ascii_lowercase(),
        value: Some(parse_attribute_value(value)?),
    })
}

fn parse_attribute_value(input: &str) -> Option<String> {
    if input.len() >= 2
        && ((input.starts_with('"') && input.ends_with('"'))
            || (input.starts_with('\'') && input.ends_with('\'')))
    {
        return Some(input[1..input.len().saturating_sub(1)].to_string());
    }
    valid_identifier(input).then(|| input.to_string())
}

fn identifier_len(input: &str) -> Option<usize> {
    let mut seen = false;
    for (index, character) in input.char_indices() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            seen = true;
            continue;
        }
        return Some(index);
    }
    Some(if seen { input.len() } else { 0 })
}

fn parse_declaration_block(
    input: &str,
    offset: usize,
    diagnostics: &mut Vec<CssDiagnostic>,
) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    let mut cursor = 0;

    while cursor < input.len() {
        cursor += leading_whitespace_len(&input[cursor..]);
        if cursor >= input.len() {
            break;
        }

        let declaration_start = cursor;
        let Some(colon) = input[cursor..].find(':') else {
            diagnostics.push(CssDiagnostic {
                offset: offset + declaration_start,
                message: "skipped declaration without ':'".to_string(),
            });
            break;
        };
        let property = input[cursor..cursor + colon].trim();
        cursor += colon + 1;

        let value_start = cursor;
        let semicolon = input[cursor..].find(';');
        let value = match semicolon {
            Some(index) => {
                let value = &input[cursor..cursor + index];
                cursor += index + 1;
                value
            }
            None => {
                cursor = input.len();
                &input[value_start..]
            }
        };

        match parse_declaration(property, value) {
            Some(declaration) => declarations.push(declaration),
            None => diagnostics.push(CssDiagnostic {
                offset: offset + declaration_start,
                message: format!("unsupported or invalid declaration {property:?}"),
            }),
        }
    }

    declarations
}

fn parse_declaration(property: &str, value: &str) -> Option<Declaration> {
    let normalized = property.trim().to_ascii_lowercase();
    let trimmed_value = value.trim();
    let (property, value) = match normalized.as_str() {
        "color" => (Property::Color, Value::Color(parse_color(trimmed_value)?)),
        "background" => (
            Property::Background,
            Value::Color(parse_color(trimmed_value)?),
        ),
        "background-color" => (
            Property::BackgroundColor,
            Value::Color(parse_color(trimmed_value)?),
        ),
        "font-size" => (
            Property::FontSize,
            Value::Length(parse_length(trimmed_value)?),
        ),
        "font-family" => (
            Property::FontFamily,
            Value::FontFamily(parse_font_family(trimmed_value)?),
        ),
        "font-weight" => (
            Property::FontWeight,
            Value::FontWeight(parse_font_weight(trimmed_value)?),
        ),
        "line-height" => (
            Property::LineHeight,
            Value::LineHeight(parse_line_height(trimmed_value)?),
        ),
        "text-align" => (
            Property::TextAlign,
            Value::TextAlign(parse_text_align(trimmed_value)?),
        ),
        "white-space" => (
            Property::WhiteSpace,
            Value::WhiteSpace(parse_white_space(trimmed_value)?),
        ),
        "text-decoration" => (
            Property::TextDecoration,
            Value::TextDecoration(parse_text_decoration(trimmed_value)?),
        ),
        "display" => (
            Property::Display,
            Value::Display(parse_display(trimmed_value)?),
        ),
        "visibility" => (
            Property::Visibility,
            Value::Visibility(parse_visibility(trimmed_value)?),
        ),
        "margin" => (Property::Margin, Value::Edges(parse_edges(trimmed_value)?)),
        "padding" => (Property::Padding, Value::Edges(parse_edges(trimmed_value)?)),
        "border" => (
            Property::Border,
            Value::Border(parse_border(trimmed_value)?),
        ),
        "border-width" => (
            Property::BorderWidth,
            Value::Length(parse_length(trimmed_value)?),
        ),
        "border-color" => (
            Property::BorderColor,
            Value::Color(parse_color(trimmed_value)?),
        ),
        "width" => (Property::Width, Value::Size(parse_size(trimmed_value)?)),
        "height" => (Property::Height, Value::Size(parse_size(trimmed_value)?)),
        "min-width" => (Property::MinWidth, Value::Size(parse_size(trimmed_value)?)),
        "max-width" => (Property::MaxWidth, Value::Size(parse_size(trimmed_value)?)),
        "min-height" => (Property::MinHeight, Value::Size(parse_size(trimmed_value)?)),
        "max-height" => (Property::MaxHeight, Value::Size(parse_size(trimmed_value)?)),
        "box-sizing" => (
            Property::BoxSizing,
            Value::BoxSizing(parse_box_sizing(trimmed_value)?),
        ),
        "position" => (
            Property::Position,
            Value::Position(parse_position(trimmed_value)?),
        ),
        "top" => (Property::Top, Value::Size(parse_size(trimmed_value)?)),
        "right" => (Property::Right, Value::Size(parse_size(trimmed_value)?)),
        "bottom" => (Property::Bottom, Value::Size(parse_size(trimmed_value)?)),
        "left" => (Property::Left, Value::Size(parse_size(trimmed_value)?)),
        "flex-direction" => (
            Property::FlexDirection,
            Value::FlexDirection(parse_flex_direction(trimmed_value)?),
        ),
        "gap" => (Property::Gap, Value::Length(parse_length(trimmed_value)?)),
        "row-gap" => (
            Property::RowGap,
            Value::Length(parse_length(trimmed_value)?),
        ),
        "column-gap" => (
            Property::ColumnGap,
            Value::Length(parse_length(trimmed_value)?),
        ),
        "grid-template-columns" => (
            Property::GridTemplateColumns,
            Value::GridTrackList(parse_grid_tracks(trimmed_value)?),
        ),
        "grid-template-rows" => (
            Property::GridTemplateRows,
            Value::GridTrackList(parse_grid_tracks(trimmed_value)?),
        ),
        "grid-column" => (
            Property::GridColumn,
            Value::GridPlacement(parse_grid_placement(trimmed_value)?),
        ),
        "grid-row" => (
            Property::GridRow,
            Value::GridPlacement(parse_grid_placement(trimmed_value)?),
        ),
        "justify-content" => (
            Property::JustifyContent,
            Value::JustifyContent(parse_justify_content(trimmed_value)?),
        ),
        "align-items" => (
            Property::AlignItems,
            Value::AlignItems(parse_align_items(trimmed_value)?),
        ),
        "flex-grow" => (
            Property::FlexGrow,
            Value::Number(parse_non_negative_number(trimmed_value)?),
        ),
        "flex" => (
            Property::Flex,
            Value::Number(parse_non_negative_number(trimmed_value)?),
        ),
        "transition-property" => (
            Property::TransitionProperty,
            Value::TransitionProperties(parse_transition_properties(trimmed_value)?),
        ),
        "transition-duration" => (
            Property::TransitionDuration,
            Value::TimeMs(parse_time_ms(trimmed_value)?),
        ),
        "transition-delay" => (
            Property::TransitionDelay,
            Value::TimeMs(parse_time_ms(trimmed_value)?),
        ),
        "transition-timing-function" => (
            Property::TransitionTimingFunction,
            Value::TransitionTimingFunction(parse_transition_timing_function(trimmed_value)?),
        ),
        "transition" => (
            Property::Transition,
            Value::Transition(parse_transition_shorthand(trimmed_value)?),
        ),
        _ => return None,
    };

    Some(Declaration { property, value })
}

fn parse_color(value: &str) -> Option<Color> {
    let lower = value.trim().to_ascii_lowercase();
    match lower.as_str() {
        "transparent" => Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }),
        "black" => Some(opaque(0, 0, 0)),
        "white" => Some(opaque(255, 255, 255)),
        "red" => Some(opaque(255, 0, 0)),
        "green" => Some(opaque(0, 128, 0)),
        "blue" => Some(opaque(0, 0, 255)),
        "yellow" => Some(opaque(255, 255, 0)),
        "gray" | "grey" => Some(opaque(128, 128, 128)),
        "silver" => Some(opaque(192, 192, 192)),
        "navy" => Some(opaque(0, 0, 128)),
        _ => parse_hex_color(&lower),
    }
}

fn parse_hex_color(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    match hex.len() {
        3 => {
            let mut chars = hex.chars();
            let r = expand_hex_digit(chars.next()?)?;
            let g = expand_hex_digit(chars.next()?)?;
            let b = expand_hex_digit(chars.next()?)?;
            Some(opaque(r, g, b))
        }
        6 => {
            if !hex.is_ascii() {
                return None;
            }
            let bytes = hex.as_bytes();
            let r = parse_hex_pair(bytes.first().copied()?, bytes.get(1).copied()?)?;
            let g = parse_hex_pair(bytes.get(2).copied()?, bytes.get(3).copied()?)?;
            let b = parse_hex_pair(bytes.get(4).copied()?, bytes.get(5).copied()?)?;
            Some(opaque(r, g, b))
        }
        _ => None,
    }
}

fn parse_hex_pair(high: u8, low: u8) -> Option<u8> {
    Some(hex_value(high)? * 16 + hex_value(low)?)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn expand_hex_digit(character: char) -> Option<u8> {
    let value = character.to_digit(16)? as u8;
    Some(value * 17)
}

fn opaque(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

fn parse_length(value: &str) -> Option<Length> {
    let trimmed = value.trim();
    let number = trimmed.strip_suffix("px").unwrap_or(trimmed).trim();
    let px = number.parse::<f32>().ok()?;
    (px.is_finite() && px >= 0.0).then_some(Length { px })
}

fn parse_size(value: &str) -> Option<Size> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("auto") {
        return Some(Size::Auto);
    }
    if let Some(percent) = trimmed.strip_suffix('%') {
        let value = percent.trim().parse::<f32>().ok()?;
        return (value.is_finite() && value >= 0.0).then_some(Size::Percent(value / 100.0));
    }
    parse_length(trimmed).map(|length| Size::Px(length.px))
}

fn parse_edges(value: &str) -> Option<Edges> {
    let lengths = value
        .split_whitespace()
        .map(parse_length)
        .collect::<Option<Vec<_>>>()?;
    match lengths.as_slice() {
        [one] => Some(Edges {
            top: one.px,
            right: one.px,
            bottom: one.px,
            left: one.px,
        }),
        [vertical, horizontal] => Some(Edges {
            top: vertical.px,
            right: horizontal.px,
            bottom: vertical.px,
            left: horizontal.px,
        }),
        [top, horizontal, bottom] => Some(Edges {
            top: top.px,
            right: horizontal.px,
            bottom: bottom.px,
            left: horizontal.px,
        }),
        [top, right, bottom, left] => Some(Edges {
            top: top.px,
            right: right.px,
            bottom: bottom.px,
            left: left.px,
        }),
        _ => None,
    }
}

fn parse_border(value: &str) -> Option<BorderValue> {
    let mut border = BorderValue {
        width: None,
        color: None,
    };

    for part in value.split_whitespace() {
        if border.width.is_none() {
            border.width = parse_length(part);
            if border.width.is_some() {
                continue;
            }
        }

        if border.color.is_none() {
            border.color = parse_color(part);
            if border.color.is_some() {
                continue;
            }
        }

        if matches!(part.to_ascii_lowercase().as_str(), "solid" | "none") {
            continue;
        }

        return None;
    }

    (border.width.is_some() || border.color.is_some()).then_some(border)
}

fn parse_box_sizing(value: &str) -> Option<BoxSizing> {
    match value.trim().to_ascii_lowercase().as_str() {
        "content-box" => Some(BoxSizing::ContentBox),
        "border-box" => Some(BoxSizing::BorderBox),
        _ => None,
    }
}

fn parse_position(value: &str) -> Option<Position> {
    match value.trim().to_ascii_lowercase().as_str() {
        "static" => Some(Position::Static),
        "relative" => Some(Position::Relative),
        "absolute" => Some(Position::Absolute),
        _ => None,
    }
}

fn parse_flex_direction(value: &str) -> Option<FlexDirection> {
    match value.trim().to_ascii_lowercase().as_str() {
        "row" => Some(FlexDirection::Row),
        "column" => Some(FlexDirection::Column),
        _ => None,
    }
}

fn parse_justify_content(value: &str) -> Option<JustifyContent> {
    match value.trim().to_ascii_lowercase().as_str() {
        "flex-start" => Some(JustifyContent::FlexStart),
        "center" => Some(JustifyContent::Center),
        "space-between" => Some(JustifyContent::SpaceBetween),
        _ => None,
    }
}

fn parse_align_items(value: &str) -> Option<AlignItems> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stretch" => Some(AlignItems::Stretch),
        "center" => Some(AlignItems::Center),
        "flex-start" => Some(AlignItems::FlexStart),
        _ => None,
    }
}

fn parse_non_negative_number(value: &str) -> Option<f32> {
    let parsed = value.trim().parse::<f32>().ok()?;
    (parsed.is_finite() && parsed >= 0.0).then_some(parsed)
}

fn parse_transition_properties(value: &str) -> Option<Vec<TransitionProperty>> {
    let properties = value
        .split(',')
        .map(|part| parse_transition_property(part.trim()))
        .collect::<Option<Vec<_>>>()?;
    (!properties.is_empty()).then_some(properties)
}

fn parse_transition_property(value: &str) -> Option<TransitionProperty> {
    match value.to_ascii_lowercase().as_str() {
        "all" => Some(TransitionProperty::All),
        "color" => Some(TransitionProperty::Color),
        "background" | "background-color" => Some(TransitionProperty::BackgroundColor),
        "left" => Some(TransitionProperty::Left),
        "top" => Some(TransitionProperty::Top),
        _ => None,
    }
}

fn parse_time_ms(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    let multiplier = if let Some(raw) = trimmed.strip_suffix("ms") {
        return parse_time_number_ms(raw, 1.0);
    } else if let Some(raw) = trimmed.strip_suffix('s') {
        (raw, 1000.0)
    } else {
        (trimmed, 1000.0)
    };
    parse_time_number_ms(multiplier.0, multiplier.1)
}

fn parse_time_number_ms(raw: &str, multiplier: f32) -> Option<u32> {
    let value = raw.trim().parse::<f32>().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let ms = value * multiplier;
    (ms <= u32::MAX as f32).then_some(ms.round() as u32)
}

fn parse_transition_timing_function(value: &str) -> Option<TransitionTimingFunction> {
    match value.trim().to_ascii_lowercase().as_str() {
        "linear" => Some(TransitionTimingFunction::Linear),
        "ease" => Some(TransitionTimingFunction::Ease),
        _ => None,
    }
}

fn parse_transition_shorthand(value: &str) -> Option<TransitionValue> {
    let mut properties = Vec::new();
    let mut duration_ms = None;
    let mut delay_ms = None;
    let mut timing_function = None;

    for token in value.split_whitespace() {
        if let Some(time_ms) = parse_time_ms(token) {
            if duration_ms.is_none() {
                duration_ms = Some(time_ms);
            } else if delay_ms.is_none() {
                delay_ms = Some(time_ms);
            } else {
                return None;
            }
            continue;
        }
        if timing_function.is_none()
            && let Some(parsed) = parse_transition_timing_function(token)
        {
            timing_function = Some(parsed);
            continue;
        }
        if properties.is_empty()
            && let Some(property) = parse_transition_property(token)
        {
            properties.push(property);
            continue;
        }
        return None;
    }

    Some(TransitionValue {
        properties: if properties.is_empty() {
            vec![TransitionProperty::All]
        } else {
            properties
        },
        duration_ms: duration_ms?,
        delay_ms: delay_ms.unwrap_or(0),
        timing_function: timing_function.unwrap_or(TransitionTimingFunction::Ease),
    })
}

fn parse_font_weight(value: &str) -> Option<FontWeight> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" | "400" => Some(FontWeight::Normal),
        "bold" | "700" => Some(FontWeight::Bold),
        _ => None,
    }
}

fn parse_font_family(value: &str) -> Option<FontFamily> {
    let first = value.split(',').next().map(|part| {
        part.trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_ascii_lowercase()
    })?;
    if first.contains("mono") || first == "code" {
        Some(FontFamily::Monospace)
    } else if !first.is_empty() {
        Some(FontFamily::Sans)
    } else {
        None
    }
}

fn parse_line_height(value: &str) -> Option<LineHeight> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("normal") {
        return Some(LineHeight::Normal);
    }
    if let Some(length) = parse_length(trimmed) {
        return Some(LineHeight::Px(length.px));
    }
    None
}

fn parse_text_align(value: &str) -> Option<TextAlign> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" | "start" => Some(TextAlign::Left),
        "center" => Some(TextAlign::Center),
        "right" | "end" => Some(TextAlign::Right),
        _ => None,
    }
}

fn parse_white_space(value: &str) -> Option<WhiteSpace> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(WhiteSpace::Normal),
        "pre" => Some(WhiteSpace::Pre),
        "nowrap" => Some(WhiteSpace::NoWrap),
        _ => None,
    }
}

fn parse_text_decoration(value: &str) -> Option<TextDecoration> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some(TextDecoration::None),
        "underline" => Some(TextDecoration::Underline),
        _ => None,
    }
}

fn parse_display(value: &str) -> Option<Display> {
    match value.trim().to_ascii_lowercase().as_str() {
        "block" => Some(Display::Block),
        "inline" => Some(Display::Inline),
        "inline-block" => Some(Display::InlineBlock),
        "flex" => Some(Display::Flex),
        "grid" => Some(Display::Grid),
        "none" => Some(Display::None),
        _ => None,
    }
}

fn parse_grid_tracks(value: &str) -> Option<Vec<GridTrack>> {
    let tracks = value
        .split_whitespace()
        .map(parse_grid_track)
        .collect::<Option<Vec<_>>>()?;
    (!tracks.is_empty()).then_some(tracks)
}

fn parse_grid_track(value: &str) -> Option<GridTrack> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("auto") {
        return Some(GridTrack::Auto);
    }
    if let Some(percent) = trimmed.strip_suffix('%') {
        let value = percent.trim().parse::<f32>().ok()?;
        return (value.is_finite() && value >= 0.0).then_some(GridTrack::Percent(value / 100.0));
    }
    if let Some(fr) = trimmed.strip_suffix("fr") {
        let value = fr.trim().parse::<f32>().ok()?;
        return (value.is_finite() && value >= 0.0).then_some(GridTrack::Fr(value));
    }
    parse_length(trimmed).map(|length| GridTrack::Px(length.px))
}

fn parse_grid_placement(value: &str) -> Option<GridPlacement> {
    let mut parts = value.split('/').map(str::trim);
    let start = parse_grid_line(parts.next()?)?;
    let end = match parts.next() {
        Some(raw) if !raw.is_empty() => parse_grid_line(raw)?,
        Some(_) => return None,
        None => None,
    };
    if parts.next().is_some() {
        return None;
    }
    if let (Some(start), Some(end)) = (start, end)
        && end <= start
    {
        return None;
    }
    Some(GridPlacement { start, end })
}

fn parse_grid_line(value: &str) -> Option<Option<usize>> {
    if value.eq_ignore_ascii_case("auto") {
        return Some(None);
    }
    let parsed = value.parse::<usize>().ok()?;
    (parsed > 0).then_some(Some(parsed))
}

fn parse_visibility(value: &str) -> Option<Visibility> {
    match value.trim().to_ascii_lowercase().as_str() {
        "visible" => Some(Visibility::Visible),
        "hidden" => Some(Visibility::Hidden),
        _ => None,
    }
}

fn valid_identifier(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || matches!(first, '_' | '-')) {
        return false;
    }
    chars.all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn leading_whitespace_len(input: &str) -> usize {
    input
        .chars()
        .take_while(|character| character.is_whitespace())
        .map(char::len_utf8)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{
        AlignItems, BoxSizing, Color, Combinator, Display, FlexDirection, FontFamily, FontWeight,
        GridPlacement, GridTrack, JustifyContent, Length, LineHeight, MediaFeature, MediaType,
        Orientation, Position, Property, PseudoClass, Selector, Size, TextAlign, TextDecoration,
        TransitionProperty, TransitionTimingFunction, Value, Visibility, WhiteSpace,
        parse_declarations, parse_stylesheet,
    };

    #[test]
    fn empty_stylesheet_is_valid() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("")?;

        assert!(stylesheet.rules.is_empty());
        assert!(stylesheet.diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn parses_supported_selectors() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet(
            "p{color:red}.card{color:blue}#main{color:black}a.nav{color:white}h1#hero{color:#123}",
        )?;

        let selectors = stylesheet
            .rules
            .iter()
            .map(|rule| rule.selector.clone())
            .collect::<Vec<_>>();
        assert_eq!(selectors.len(), 5);
        assert_eq!(selectors[0].parts[0].tag_name.as_deref(), Some("p"));
        assert_eq!(selectors[1].parts[0].classes, vec!["card"]);
        assert_eq!(selectors[2].parts[0].id.as_deref(), Some("main"));
        assert_eq!(selectors[3].parts[0].tag_name.as_deref(), Some("a"));
        assert_eq!(selectors[3].parts[0].classes, vec!["nav"]);
        assert_eq!(selectors[4].parts[0].tag_name.as_deref(), Some("h1"));
        assert_eq!(selectors[4].parts[0].id.as_deref(), Some("hero"));
        Ok(())
    }

    #[test]
    fn parses_descendant_selector() -> webby_core::WebbyResult<()> {
        let selector = first_selector(".card p { color: red; }")?;

        assert_eq!(selector.parts.len(), 2);
        assert_eq!(selector.combinators, vec![Combinator::Descendant]);
        assert_eq!(selector.parts[0].classes, vec!["card"]);
        assert_eq!(selector.parts[1].tag_name.as_deref(), Some("p"));
        Ok(())
    }

    #[test]
    fn parses_child_selector() -> webby_core::WebbyResult<()> {
        let selector = first_selector(".card > p { color: red; }")?;

        assert_eq!(selector.parts.len(), 2);
        assert_eq!(selector.combinators, vec![Combinator::Child]);
        Ok(())
    }

    #[test]
    fn parses_grouped_selectors() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("h1, h2, h3 { color: red; }")?;

        assert_eq!(stylesheet.rules.len(), 3);
        assert_eq!(
            stylesheet.rules[0].selector.parts[0].tag_name.as_deref(),
            Some("h1")
        );
        assert_eq!(
            stylesheet.rules[1].selector.parts[0].tag_name.as_deref(),
            Some("h2")
        );
        assert_eq!(
            stylesheet.rules[2].selector.parts[0].tag_name.as_deref(),
            Some("h3")
        );
        Ok(())
    }

    #[test]
    fn parses_universal_selector() -> webby_core::WebbyResult<()> {
        let selector = first_selector("* { color: red; }")?;

        assert!(selector.parts[0].universal);
        assert_eq!(selector.specificity().ids, 0);
        assert_eq!(selector.specificity().classes, 0);
        assert_eq!(selector.specificity().tags, 0);
        Ok(())
    }

    #[test]
    fn parses_multiple_classes() -> webby_core::WebbyResult<()> {
        let selector = first_selector(".card.highlighted { color: red; }")?;

        assert_eq!(selector.parts[0].classes, vec!["card", "highlighted"]);
        assert_eq!(selector.specificity().classes, 2);
        Ok(())
    }

    #[test]
    fn parses_attribute_selectors() -> webby_core::WebbyResult<()> {
        let exists = first_selector("[href] { color: red; }")?;
        let equals = first_selector("input[type=\"text\"] { color: blue; }")?;

        assert_eq!(exists.parts[0].attributes[0].name, "href");
        assert_eq!(exists.parts[0].attributes[0].value, None);
        assert_eq!(equals.parts[0].tag_name.as_deref(), Some("input"));
        assert_eq!(equals.parts[0].attributes[0].name, "type");
        assert_eq!(equals.parts[0].attributes[0].value.as_deref(), Some("text"));
        assert_eq!(equals.specificity().classes, 1);
        assert_eq!(equals.specificity().tags, 1);
        Ok(())
    }

    #[test]
    fn parses_supported_pseudo_classes() -> webby_core::WebbyResult<()> {
        let selector = first_selector(
            "input:hover:focus:active:checked:disabled:first-child:last-child:nth-child(2) { color: red; }",
        )?;

        assert_eq!(
            selector.parts[0].pseudo_classes,
            vec![
                PseudoClass::Hover,
                PseudoClass::Focus,
                PseudoClass::Active,
                PseudoClass::Checked,
                PseudoClass::Disabled,
                PseudoClass::FirstChild,
                PseudoClass::LastChild,
                PseudoClass::NthChild(2),
            ]
        );
        assert_eq!(selector.specificity().classes, 8);
        assert_eq!(selector.specificity().tags, 1);
        Ok(())
    }

    #[test]
    fn unsupported_pseudo_class_is_skipped_with_diagnostic() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("a:visited { color: red; } a:hover { color: blue; }")?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(
            stylesheet.rules[0].selector.parts[0].pseudo_classes,
            vec![PseudoClass::Hover]
        );
        assert!(stylesheet.diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("unsupported selector")
                && diagnostic.message.contains(":visited")
        }));
        Ok(())
    }

    #[test]
    fn malformed_pseudo_class_selector_makes_progress() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("a:nth-child(bad) { color: red; } p { color: blue; }")?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(
            stylesheet.rules[0].selector.parts[0].tag_name.as_deref(),
            Some("p")
        );
        assert!(stylesheet.diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("unsupported selector")
                && diagnostic.message.contains("nth-child")
        }));
        Ok(())
    }

    #[test]
    fn parses_supported_declarations() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet(
            "p {
                color: #0f0;
                background: white;
                background-color: #112233;
                font-size: 18px;
                font-family: monospace;
                line-height: 24px;
                font-weight: bold;
                text-decoration: underline;
                text-align: right;
                white-space: pre;
                display: flex;
                visibility: hidden;
                margin: 1px 2px 3px 4px;
                padding: 5;
                border: 2px solid red;
                border-width: 3px;
                border-color: blue;
                width: 120px;
                min-width: 50%;
                max-width: 320px;
                height: 44px;
                min-height: 12px;
                max-height: 90%;
                box-sizing: border-box;
                position: relative;
                top: 2px;
                right: auto;
                bottom: 4px;
                left: 10%;
                flex-direction: column;
                gap: 8px;
                row-gap: 9px;
                column-gap: 10px;
                grid-template-columns: 120px 1fr 25%;
                grid-template-rows: auto 40px;
                grid-column: 2 / 4;
                grid-row: 1;
                justify-content: space-between;
                align-items: center;
                flex-grow: 2;
                flex: 1;
                transition-property: color, background-color;
                transition-duration: 150ms;
                transition-delay: 25ms;
                transition-timing-function: linear;
                transition: color 200ms ease 50ms;
            }",
        )?;

        let rule = stylesheet
            .rules
            .first()
            .ok_or_else(|| webby_core::WebbyError::invalid_input("missing CSS rule"))?;
        assert_eq!(rule.declarations.len(), 46);
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Color
                && declaration.value
                    == Value::Color(Color {
                        r: 0,
                        g: 255,
                        b: 0,
                        a: 255,
                    })
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::FontWeight
                && declaration.value == Value::FontWeight(FontWeight::Bold)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::FontFamily
                && declaration.value == Value::FontFamily(FontFamily::Monospace)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::LineHeight
                && declaration.value == Value::LineHeight(LineHeight::Px(24.0))
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::TextDecoration
                && declaration.value == Value::TextDecoration(TextDecoration::Underline)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::TextAlign
                && declaration.value == Value::TextAlign(TextAlign::Right)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::WhiteSpace
                && declaration.value == Value::WhiteSpace(WhiteSpace::Pre)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Display
                && declaration.value == Value::Display(Display::Flex)
        }));
        let grid_display = parse_stylesheet("main { display: grid; }")?;
        assert!(
            grid_display.rules[0]
                .declarations
                .iter()
                .any(|declaration| {
                    declaration.property == Property::Display
                        && declaration.value == Value::Display(Display::Grid)
                })
        );
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Visibility
                && declaration.value == Value::Visibility(Visibility::Hidden)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Width
                && declaration.value == Value::Size(Size::Px(120.0))
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::MinWidth
                && declaration.value == Value::Size(Size::Percent(0.5))
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::BoxSizing
                && declaration.value == Value::BoxSizing(BoxSizing::BorderBox)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Position
                && declaration.value == Value::Position(Position::Relative)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Top && declaration.value == Value::Size(Size::Px(2.0))
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Right && declaration.value == Value::Size(Size::Auto)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Left
                && declaration.value == Value::Size(Size::Percent(0.1))
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::FlexDirection
                && declaration.value == Value::FlexDirection(FlexDirection::Column)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::RowGap
                && declaration.value == Value::Length(Length { px: 9.0 })
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::ColumnGap
                && declaration.value == Value::Length(Length { px: 10.0 })
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::GridTemplateColumns
                && declaration.value
                    == Value::GridTrackList(vec![
                        GridTrack::Px(120.0),
                        GridTrack::Fr(1.0),
                        GridTrack::Percent(0.25),
                    ])
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::GridTemplateRows
                && declaration.value
                    == Value::GridTrackList(vec![GridTrack::Auto, GridTrack::Px(40.0)])
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::GridColumn
                && declaration.value
                    == Value::GridPlacement(GridPlacement {
                        start: Some(2),
                        end: Some(4),
                    })
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::GridRow
                && declaration.value
                    == Value::GridPlacement(GridPlacement {
                        start: Some(1),
                        end: None,
                    })
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::JustifyContent
                && declaration.value == Value::JustifyContent(JustifyContent::SpaceBetween)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::AlignItems
                && declaration.value == Value::AlignItems(AlignItems::Center)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::Flex && declaration.value == Value::Number(1.0)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::TransitionProperty
                && declaration.value
                    == Value::TransitionProperties(vec![
                        TransitionProperty::Color,
                        TransitionProperty::BackgroundColor,
                    ])
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::TransitionDuration
                && declaration.value == Value::TimeMs(150)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::TransitionDelay
                && declaration.value == Value::TimeMs(25)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            declaration.property == Property::TransitionTimingFunction
                && declaration.value
                    == Value::TransitionTimingFunction(TransitionTimingFunction::Linear)
        }));
        assert!(rule.declarations.iter().any(|declaration| {
            matches!(
                &declaration.value,
                Value::Transition(value)
                    if declaration.property == Property::Transition
                        && value.properties == vec![TransitionProperty::Color]
                        && value.duration_ms == 200
                        && value.delay_ms == 50
                        && value.timing_function == TransitionTimingFunction::Ease
            )
        }));
        Ok(())
    }

    #[test]
    fn unsupported_transition_syntax_is_diagnostic() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet(
            "p { transition: transform 1s; color: red; transition-duration: -1s; }",
        )?;

        assert_eq!(stylesheet.rules[0].declarations.len(), 1);
        assert_eq!(
            stylesheet.rules[0].declarations[0].property,
            Property::Color
        );
        assert_eq!(stylesheet.diagnostics.len(), 2);
        Ok(())
    }

    #[test]
    fn parses_media_rules_with_supported_features() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet(
            "@media screen and (min-width: 600px) and (max-width: 900px) {
                main { display: grid; }
            }
            @media all and (width: 320px) {
                body { color: red; }
            }
            @media (orientation: landscape) {
                p { font-size: 18px; }
            }",
        )?;

        assert_eq!(stylesheet.rules.len(), 3);
        let first = stylesheet
            .rules
            .first()
            .ok_or_else(|| webby_core::WebbyError::invalid_input("expected first media rule"))?;
        assert_eq!(
            first.media.as_ref().map(|media| media.media_type),
            Some(MediaType::Screen)
        );
        assert_eq!(
            first.media.as_ref().map(|media| media.features.as_slice()),
            Some([MediaFeature::MinWidth(600), MediaFeature::MaxWidth(900)].as_slice())
        );
        assert_eq!(
            stylesheet
                .rules
                .get(1)
                .and_then(|rule| rule.media.as_ref())
                .map(|media| media.features.as_slice()),
            Some([MediaFeature::Width(320)].as_slice())
        );
        assert_eq!(
            stylesheet
                .rules
                .get(2)
                .and_then(|rule| rule.media.as_ref())
                .map(|media| media.features.as_slice()),
            Some([MediaFeature::Orientation(Orientation::Landscape)].as_slice())
        );
        Ok(())
    }

    #[test]
    fn unsupported_media_query_is_skipped_with_diagnostic() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet(
            "@media print and (min-width: 1px) { p { color: red; } } p { color: blue; }",
        )?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert!(!stylesheet.diagnostics.is_empty());
        assert!(
            stylesheet.diagnostics[0]
                .message
                .contains("unsupported media query")
        );
        Ok(())
    }

    #[test]
    fn malformed_media_rule_makes_progress() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet(
            "@media screen and (min-width: wide) { p { color: red; } } a { color: blue; }",
        )?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(
            stylesheet.rules[0].selector.parts[0].tag_name.as_deref(),
            Some("a")
        );
        assert!(!stylesheet.diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn invalid_css_is_skipped_with_diagnostics() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("p { color: red; invalid } p + a { color: blue }")?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert!(!stylesheet.diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn malformed_and_unsupported_selectors_are_skipped_with_diagnostics()
    -> webby_core::WebbyResult<()> {
        let stylesheet =
            parse_stylesheet(".card > { color: red; } p + a { color: blue; } p { color: green; }")?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(
            stylesheet.rules[0].selector.parts[0].tag_name.as_deref(),
            Some("p")
        );
        assert_eq!(stylesheet.diagnostics.len(), 2);
        Ok(())
    }

    #[test]
    fn grouped_selector_keeps_valid_members_when_one_member_is_unsupported()
    -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("p + a, h1 { color: red; }")?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(
            stylesheet.rules[0].selector.parts[0].tag_name.as_deref(),
            Some("h1")
        );
        assert_eq!(stylesheet.diagnostics.len(), 1);
        assert!(
            stylesheet.diagnostics[0]
                .message
                .contains("unsupported selector")
        );
        Ok(())
    }

    #[test]
    fn unclosed_attribute_selector_reports_diagnostic_and_keeps_later_rule()
    -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("[href { color: red; } p { color: blue; }")?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(
            stylesheet.rules[0].selector.parts[0].tag_name.as_deref(),
            Some("p")
        );
        assert_eq!(stylesheet.diagnostics.len(), 1);
        assert!(
            stylesheet.diagnostics[0]
                .message
                .contains("unsupported selector")
        );
        Ok(())
    }

    #[test]
    fn unsupported_declarations_are_ignored() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("p { transform: rotate(10deg); color: red; }")?;
        let rule = stylesheet
            .rules
            .first()
            .ok_or_else(|| webby_core::WebbyError::invalid_input("missing CSS rule"))?;

        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations[0].property, Property::Color);
        assert_eq!(stylesheet.diagnostics.len(), 1);
        Ok(())
    }

    #[test]
    fn inline_declaration_parser_accepts_declaration_lists() -> webby_core::WebbyResult<()> {
        let (declarations, diagnostics) =
            parse_declarations("color: red; font-weight: bold; broken")?;

        assert_eq!(declarations.len(), 2);
        assert_eq!(diagnostics.len(), 1);
        Ok(())
    }

    #[test]
    fn malformed_css_makes_progress() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("/* unterminated comment p { color: red; } trailing")?;

        assert!(stylesheet.rules.is_empty());
        assert_eq!(stylesheet.diagnostics.len(), 1);
        Ok(())
    }

    #[test]
    fn generated_malformed_css_inputs_do_not_panic_or_loop() -> webby_core::WebbyResult<()> {
        let fragments = [
            "{",
            "}",
            "p { color:",
            ".card > {",
            "[href { color: red; }",
            "a + b { color: red; }",
            "/* unterminated",
            "#id.class[attr=\"x\" {",
            "span { margin: one two; }",
            ",,,",
        ];

        for repeat in 1..=10 {
            let mut css = String::new();
            for index in 0..repeat {
                css.push_str(fragments[index % fragments.len()]);
                css.push_str(" p { color: blue; }");
            }
            let stylesheet = parse_stylesheet(&css)?;
            assert!(stylesheet.rules.len() <= repeat);
        }
        Ok(())
    }

    #[test]
    fn unterminated_rule_keeps_valid_declarations() -> webby_core::WebbyResult<()> {
        let stylesheet = parse_stylesheet("p { color: red; font-size: 20px")?;

        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(stylesheet.rules[0].declarations.len(), 2);
        assert_eq!(stylesheet.diagnostics.len(), 1);
        Ok(())
    }

    fn first_selector(css: &str) -> webby_core::WebbyResult<Selector> {
        let stylesheet = parse_stylesheet(css)?;
        stylesheet
            .rules
            .first()
            .map(|rule| rule.selector.clone())
            .ok_or_else(|| webby_core::WebbyError::invalid_input("missing selector"))
    }
}
