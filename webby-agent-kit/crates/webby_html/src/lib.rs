//! HTML tokenization, DOM parsing, and visible-text extraction.

use std::{collections::BTreeMap, fmt};

use webby_core::{WebbyError, WebbyResult};
use webby_dom::{Document, Node, NodeKind};

/// Maximum HTML source bytes accepted by the parser.
pub const MAX_HTML_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum DOM nodes produced by one parsed document, including the document root.
pub const MAX_DOM_NODES: usize = 16_384;

/// Stylesheet link metadata collected from the DOM in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StylesheetLink {
    /// Link target exactly as preserved in the DOM.
    pub href: String,
    /// Original `rel` attribute value.
    pub rel: String,
}

/// Favicon/icon link metadata collected from the DOM in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconLink {
    /// Link target exactly as preserved in the DOM.
    pub href: String,
    /// Original `rel` attribute value.
    pub rel: String,
    /// Optional `sizes` attribute.
    pub sizes: Option<String>,
    /// Optional `type` attribute.
    pub media_type: Option<String>,
}

/// Resource hint metadata for no-op diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceHint {
    /// Original `rel` attribute value.
    pub rel: String,
    /// Optional `href` attribute.
    pub href: Option<String>,
}

/// Iframe metadata collected from the DOM in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IframeElement {
    /// Source URL exactly as preserved in the DOM.
    pub src: Option<String>,
    /// Optional browsing-context name.
    pub name: Option<String>,
    /// Optional width attribute.
    pub width: Option<String>,
    /// Optional height attribute.
    pub height: Option<String>,
    /// Optional sandbox attribute.
    pub sandbox: Option<String>,
}

/// Inline script metadata collected from the DOM in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptBlock {
    /// Deterministic script label for diagnostics.
    pub label: String,
    /// Raw script text exactly as preserved in the DOM after entity decoding.
    pub text: String,
}

/// External script metadata collected from the DOM in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptReference {
    /// Deterministic script label for diagnostics.
    pub label: String,
    /// Script source exactly as preserved in the DOM.
    pub src: String,
}

/// Script payload collected from the DOM in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptContent {
    /// Inline script text.
    Inline(String),
    /// External script source URL exactly as preserved in the DOM.
    External(String),
}

/// Script metadata collected from the DOM in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptElement {
    /// Deterministic script label for diagnostics.
    pub label: String,
    /// Script payload.
    pub content: ScriptContent,
    /// Optional `type` attribute exactly as preserved in the DOM.
    pub script_type: Option<String>,
    /// Whether the `defer` attribute is present.
    pub defer: bool,
    /// Whether the `async` attribute is present.
    pub async_attr: bool,
}

/// A deterministic forgiving-parser diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlDiagnostic {
    /// Byte offset where recovery started.
    pub offset: usize,
    /// Human-readable recovery description.
    pub message: String,
}

impl fmt::Display for HtmlDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "HTML diagnostic at byte {}: {}",
            self.offset, self.message
        )
    }
}

/// DOM output plus diagnostics from forgiving HTML recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDocument {
    /// Parsed DOM document.
    pub document: Document,
    /// Recoverable parser diagnostics in input order.
    pub diagnostics: Vec<HtmlDiagnostic>,
}

/// Parses HTML into Webby's DOM.
pub fn parse_document(input: &str) -> WebbyResult<Document> {
    Ok(parse_document_with_diagnostics(input)?.document)
}

/// Parses HTML into Webby's DOM and retains deterministic recovery diagnostics.
pub fn parse_document_with_diagnostics(input: &str) -> WebbyResult<ParsedDocument> {
    if input.len() > MAX_HTML_INPUT_BYTES {
        return Err(WebbyError::Parse {
            message: format!(
                "HTML input is {} bytes, limit is {} bytes",
                input.len(),
                MAX_HTML_INPUT_BYTES
            ),
        });
    }
    let (tokens, diagnostics) = tokenize(input);
    let mut stack = vec![Node::document()];
    let mut node_count = 1usize;

    for token in tokens {
        match token {
            Token::Text(text) => {
                note_dom_node(&mut node_count)?;
                append_child(&mut stack, Node::text(text));
            }
            Token::StartTag {
                tag_name,
                attributes,
                self_closing,
            } => {
                close_optional_elements_for_start_tag(&mut stack, &tag_name);
                note_dom_node(&mut node_count)?;
                let node = Node::element(tag_name, attributes);
                if self_closing {
                    append_child(&mut stack, node);
                } else {
                    stack.push(node);
                }
            }
            Token::EndTag(tag_name) => close_element(&mut stack, &tag_name),
        }
    }

    while stack.len() > 1 {
        if let Some(node) = stack.pop() {
            append_child(&mut stack, node);
        }
    }

    let mut document = Document {
        root: stack.pop().unwrap_or_else(Node::document),
    };
    document.assign_stable_ids();
    Ok(ParsedDocument {
        document,
        diagnostics,
    })
}

fn note_dom_node(node_count: &mut usize) -> WebbyResult<()> {
    *node_count = node_count.saturating_add(1);
    if *node_count > MAX_DOM_NODES {
        return Err(WebbyError::Parse {
            message: format!("DOM node limit {MAX_DOM_NODES} exceeded"),
        });
    }
    Ok(())
}

/// Extracts visible text from an existing DOM tree.
pub fn extract_visible_text(document: &Document) -> String {
    let mut output = VisibleText::default();
    collect_visible_text(&document.root, &mut output);
    output.finish()
}

/// Formats a DOM tree for deterministic CLI/debug output.
pub fn dump_dom(document: &Document) -> String {
    let mut output = String::new();
    dump_node(&document.root, 0, &mut output);
    output
}

/// Collects stylesheet links in document order.
pub fn collect_stylesheet_links(document: &Document) -> Vec<StylesheetLink> {
    let mut links = Vec::new();
    collect_stylesheet_links_from_node(&document.root, &mut links);
    links
}

/// Collects favicon/icon links in document order.
pub fn collect_icon_links(document: &Document) -> Vec<IconLink> {
    let mut links = Vec::new();
    collect_icon_links_from_node(&document.root, &mut links);
    links
}

/// Extracts the first document title as trimmed text.
pub fn extract_document_title(document: &Document) -> String {
    find_first_tag(&document.root, "title")
        .map(node_text_content)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Collects preload/preconnect hints in document order.
pub fn collect_resource_hints(document: &Document) -> Vec<ResourceHint> {
    let mut hints = Vec::new();
    collect_resource_hints_from_node(&document.root, &mut hints);
    hints
}

/// Collects iframe elements in document order.
pub fn collect_iframes(document: &Document) -> Vec<IframeElement> {
    let mut iframes = Vec::new();
    collect_iframes_from_node(&document.root, &mut iframes);
    iframes
}

/// Collects inline script blocks in document order.
pub fn collect_inline_scripts(document: &Document) -> Vec<ScriptBlock> {
    collect_scripts(document)
        .into_iter()
        .filter_map(|script| match script.content {
            ScriptContent::Inline(text) => Some(ScriptBlock {
                label: script.label,
                text,
            }),
            ScriptContent::External(_) => None,
        })
        .collect()
}

/// Collects external script references in document order.
pub fn collect_external_scripts(document: &Document) -> Vec<ScriptReference> {
    collect_scripts(document)
        .into_iter()
        .filter_map(|script| match script.content {
            ScriptContent::External(src) => Some(ScriptReference {
                label: script.label,
                src,
            }),
            ScriptContent::Inline(_) => None,
        })
        .collect()
}

/// Collects all script elements in document order.
pub fn collect_scripts(document: &Document) -> Vec<ScriptElement> {
    let mut scripts = Vec::new();
    let mut counts = ScriptCounts::default();
    collect_scripts_from_node(&document.root, &mut counts, &mut scripts);
    scripts
}

#[derive(Default)]
struct ScriptCounts {
    inline: usize,
    external: usize,
}

fn collect_scripts_from_node(
    node: &Node,
    counts: &mut ScriptCounts,
    scripts: &mut Vec<ScriptElement>,
) {
    if let NodeKind::Element(element) = &node.kind
        && element.tag_name == "script"
    {
        let content = if let Some(src) = element.attributes.get("src") {
            counts.external += 1;
            ScriptContent::External(src.clone())
        } else {
            counts.inline += 1;
            let mut text = String::new();
            collect_text_descendants(node, &mut text);
            ScriptContent::Inline(text)
        };
        let label = match &content {
            ScriptContent::Inline(_) => format!("inline script {}", counts.inline),
            ScriptContent::External(_) => format!("external script {}", counts.external),
        };
        scripts.push(ScriptElement {
            label,
            content,
            script_type: element.attributes.get("type").cloned(),
            defer: element.attributes.contains_key("defer"),
            async_attr: element.attributes.contains_key("async"),
        });
        return;
    }

    for child in node.tree_children() {
        collect_scripts_from_node(child, counts, scripts);
    }
}

fn collect_text_descendants(node: &Node, output: &mut String) {
    match &node.kind {
        NodeKind::Text(text) => output.push_str(text),
        NodeKind::Document | NodeKind::Element(_) => {
            for child in node.tree_children() {
                collect_text_descendants(child, output);
            }
        }
    }
}

fn collect_stylesheet_links_from_node(node: &Node, links: &mut Vec<StylesheetLink>) {
    if let NodeKind::Element(element) = &node.kind
        && element.tag_name == "link"
    {
        let rel = element.attributes.get("rel");
        let href = element.attributes.get("href");
        if let (Some(rel), Some(href)) = (rel, href)
            && rel
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("stylesheet"))
        {
            links.push(StylesheetLink {
                href: href.clone(),
                rel: rel.clone(),
            });
        }
    }

    for child in node.tree_children() {
        collect_stylesheet_links_from_node(child, links);
    }
}

fn collect_icon_links_from_node(node: &Node, links: &mut Vec<IconLink>) {
    if let NodeKind::Element(element) = &node.kind
        && element.tag_name == "link"
    {
        let rel = element.attributes.get("rel");
        let href = element.attributes.get("href");
        if let (Some(rel), Some(href)) = (rel, href)
            && rel.split_ascii_whitespace().any(is_icon_rel_token)
        {
            links.push(IconLink {
                href: href.clone(),
                rel: rel.clone(),
                sizes: element.attributes.get("sizes").cloned(),
                media_type: element.attributes.get("type").cloned(),
            });
        }
    }

    for child in node.tree_children() {
        collect_icon_links_from_node(child, links);
    }
}

fn find_first_tag<'a>(node: &'a Node, tag_name: &str) -> Option<&'a Node> {
    if let NodeKind::Element(element) = &node.kind
        && element.tag_name == tag_name
    {
        return Some(node);
    }
    node.tree_children()
        .find_map(|child| find_first_tag(child, tag_name))
}

fn node_text_content(node: &Node) -> String {
    match &node.kind {
        NodeKind::Text(text) => text.clone(),
        NodeKind::Document | NodeKind::Element(_) => {
            let mut text = String::new();
            for child in node.tree_children() {
                text.push_str(&node_text_content(child));
            }
            text
        }
    }
}

fn is_icon_rel_token(token: &str) -> bool {
    token.eq_ignore_ascii_case("icon") || token.to_ascii_lowercase().ends_with("-icon")
}

fn collect_resource_hints_from_node(node: &Node, hints: &mut Vec<ResourceHint>) {
    if let NodeKind::Element(element) = &node.kind
        && element.tag_name == "link"
        && let Some(rel) = element.attributes.get("rel")
        && rel.split_ascii_whitespace().any(|token| {
            token.eq_ignore_ascii_case("preload") || token.eq_ignore_ascii_case("preconnect")
        })
    {
        hints.push(ResourceHint {
            rel: rel.clone(),
            href: element.attributes.get("href").cloned(),
        });
    }

    for child in node.tree_children() {
        collect_resource_hints_from_node(child, hints);
    }
}

fn collect_iframes_from_node(node: &Node, iframes: &mut Vec<IframeElement>) {
    if let NodeKind::Element(element) = &node.kind
        && element.tag_name == "iframe"
    {
        iframes.push(IframeElement {
            src: element.attributes.get("src").cloned(),
            name: element.attributes.get("name").cloned(),
            width: element.attributes.get("width").cloned(),
            height: element.attributes.get("height").cloned(),
            sandbox: element.attributes.get("sandbox").cloned(),
        });
    }

    for child in node.tree_children() {
        collect_iframes_from_node(child, iframes);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    StartTag {
        tag_name: String,
        attributes: BTreeMap<String, String>,
        self_closing: bool,
    },
    EndTag(String),
    Text(String),
}

fn tokenize(input: &str) -> (Vec<Token>, Vec<HtmlDiagnostic>) {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut cursor = 0;

    while cursor < input.len() {
        let remaining = &input[cursor..];
        let Some(relative_tag_start) = remaining.find('<') else {
            push_text(&mut tokens, remaining);
            break;
        };

        if relative_tag_start > 0 {
            push_text(&mut tokens, &remaining[..relative_tag_start]);
        }

        cursor += relative_tag_start;
        let markup = &input[cursor..];

        if markup.starts_with("<!--") {
            if let Some(after) = find_after(markup, "-->") {
                cursor += after;
            } else {
                diagnostics.push(HtmlDiagnostic {
                    offset: cursor,
                    message: "unclosed comment ignored through end of input".to_string(),
                });
                cursor = input.len();
            }
            continue;
        }

        if markup
            .get(..9)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("<!doctype"))
        {
            if let Some(after) = find_after(markup, ">") {
                cursor += after;
            } else {
                diagnostics.push(HtmlDiagnostic {
                    offset: cursor,
                    message: "unclosed doctype ignored through end of input".to_string(),
                });
                cursor = input.len();
            }
            continue;
        }

        if markup.starts_with("</") {
            let Some(tag_end) = find_after(markup, ">") else {
                diagnostics.push(HtmlDiagnostic {
                    offset: cursor,
                    message: "unclosed end tag preserved as text".to_string(),
                });
                push_text(&mut tokens, markup);
                break;
            };
            let raw_name = markup[2..tag_end - 1].trim();
            if let Some(name) = parse_tag_name(raw_name) {
                tokens.push(Token::EndTag(name));
            }
            cursor += tag_end;
            continue;
        }

        if markup.starts_with('<') {
            let Some(tag_end) = find_tag_end(markup) else {
                diagnostics.push(HtmlDiagnostic {
                    offset: cursor,
                    message: "unclosed start tag preserved as text".to_string(),
                });
                push_text(&mut tokens, markup);
                break;
            };
            let raw_tag = &markup[1..tag_end - 1];
            if let Some((tag_name, attributes, explicit_self_closing)) = parse_start_tag(raw_tag) {
                let self_closing = explicit_self_closing || is_void_element(&tag_name);
                tokens.push(Token::StartTag {
                    tag_name: tag_name.clone(),
                    attributes,
                    self_closing,
                });
                cursor += tag_end;

                if is_raw_text_element(&tag_name) || is_escapable_raw_text_element(&tag_name) {
                    let raw_text_start = cursor;
                    match find_raw_text_close(&input[cursor..], &tag_name) {
                        Some(close) => {
                            if preserves_raw_text_in_dom(&tag_name) {
                                let text = &input[raw_text_start..raw_text_start + close.start];
                                push_raw_text(
                                    &mut tokens,
                                    text,
                                    is_escapable_raw_text_element(&tag_name),
                                );
                            }
                            cursor += close.after;
                            tokens.push(Token::EndTag(tag_name));
                        }
                        None => {
                            diagnostics.push(HtmlDiagnostic {
                                offset: raw_text_start,
                                message: format!(
                                    "unclosed {tag_name} element preserved through end of input"
                                ),
                            });
                            if preserves_raw_text_in_dom(&tag_name) {
                                push_raw_text(
                                    &mut tokens,
                                    &input[cursor..],
                                    is_escapable_raw_text_element(&tag_name),
                                );
                            }
                            cursor = input.len();
                            tokens.push(Token::EndTag(tag_name));
                        }
                    }
                }
                continue;
            }
        }

        push_text(&mut tokens, "<");
        cursor += 1;
    }

    (tokens, diagnostics)
}

fn push_text(tokens: &mut Vec<Token>, text: &str) {
    if !text.is_empty() {
        tokens.push(Token::Text(decode_entities(text)));
    }
}

fn push_raw_text(tokens: &mut Vec<Token>, text: &str, decode: bool) {
    if text.is_empty() {
        return;
    }
    let text = if decode {
        decode_entities(text)
    } else {
        text.to_string()
    };
    tokens.push(Token::Text(text));
}

fn find_after(input: &str, pattern: &str) -> Option<usize> {
    input.find(pattern).map(|index| index + pattern.len())
}

fn find_tag_end(input: &str) -> Option<usize> {
    let mut quote = None;

    for (index, character) in input.char_indices() {
        match (character, quote) {
            ('"' | '\'', None) => quote = Some(character),
            (current, Some(open)) if current == open => quote = None,
            ('>', None) => return Some(index + 1),
            _ => {}
        }
    }

    None
}

fn parse_start_tag(raw_tag: &str) -> Option<(String, BTreeMap<String, String>, bool)> {
    let trimmed = raw_tag.trim();
    if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('?') {
        return None;
    }

    let explicit_self_closing = trimmed.ends_with('/');
    let without_slash = if explicit_self_closing {
        trimmed[..trimmed.len().saturating_sub(1)].trim_end()
    } else {
        trimmed
    };
    let name_end = without_slash
        .find(|character: char| character.is_whitespace())
        .unwrap_or(without_slash.len());
    let tag_name = parse_tag_name(&without_slash[..name_end])?;
    let attributes = parse_attributes(&without_slash[name_end..]);

    Some((tag_name, attributes, explicit_self_closing))
}

fn parse_tag_name(raw_name: &str) -> Option<String> {
    let name = raw_name
        .trim()
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':')
        })
        .collect::<String>()
        .to_ascii_lowercase();

    if name.is_empty() { None } else { Some(name) }
}

fn parse_attributes(input: &str) -> BTreeMap<String, String> {
    let mut attributes = BTreeMap::new();
    let mut cursor = 0;

    while cursor < input.len() {
        cursor += leading_whitespace_len(&input[cursor..]);
        if cursor >= input.len() {
            break;
        }

        let name_start = cursor;
        while cursor < input.len() {
            let rest = &input[cursor..];
            let Some(character) = rest.chars().next() else {
                break;
            };
            if character.is_whitespace() || matches!(character, '=' | '/' | '>') {
                break;
            }
            cursor += character.len_utf8();
        }

        if name_start == cursor {
            cursor += input[cursor..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
            continue;
        }

        let name = input[name_start..cursor].to_ascii_lowercase();
        cursor += leading_whitespace_len(&input[cursor..]);

        let value = if input[cursor..].starts_with('=') {
            cursor += 1;
            cursor += leading_whitespace_len(&input[cursor..]);
            parse_attribute_value(input, &mut cursor)
        } else {
            String::new()
        };

        attributes.insert(name, decode_entities(&value));
    }

    attributes
}

fn parse_attribute_value(input: &str, cursor: &mut usize) -> String {
    let Some(first) = input[*cursor..].chars().next() else {
        return String::new();
    };

    if matches!(first, '"' | '\'') {
        *cursor += first.len_utf8();
        let value_start = *cursor;
        while *cursor < input.len() {
            let Some(character) = input[*cursor..].chars().next() else {
                break;
            };
            if character == first {
                let value = input[value_start..*cursor].to_string();
                *cursor += character.len_utf8();
                return value;
            }
            *cursor += character.len_utf8();
        }
        return input[value_start..].to_string();
    }

    let value_start = *cursor;
    while *cursor < input.len() {
        let Some(character) = input[*cursor..].chars().next() else {
            break;
        };
        if character.is_whitespace() || matches!(character, '>') {
            break;
        }
        *cursor += character.len_utf8();
    }

    input[value_start..*cursor].to_string()
}

fn leading_whitespace_len(input: &str) -> usize {
    input
        .chars()
        .take_while(|character| character.is_whitespace())
        .map(char::len_utf8)
        .sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawTextClose {
    start: usize,
    after: usize,
}

fn find_raw_text_close(input: &str, tag_name: &str) -> Option<RawTextClose> {
    let lower = input.to_ascii_lowercase();
    let needle = format!("</{tag_name}");
    let mut search_from = 0;

    while let Some(relative_start) = lower[search_from..].find(&needle) {
        let start = search_from + relative_start;
        let after_name = start + needle.len();
        let boundary = lower[after_name..].chars().next();

        if boundary.is_some_and(|character| character == '>' || character.is_whitespace()) {
            let end = find_after(&input[start..], ">")?;
            return Some(RawTextClose {
                start,
                after: start + end,
            });
        }

        search_from = after_name;
    }

    None
}

fn is_void_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "source"
            | "track"
            | "wbr"
    )
}

fn is_raw_text_element(tag_name: &str) -> bool {
    matches!(tag_name, "script" | "style")
}

fn is_escapable_raw_text_element(tag_name: &str) -> bool {
    matches!(tag_name, "textarea" | "title")
}

fn preserves_raw_text_in_dom(tag_name: &str) -> bool {
    matches!(tag_name, "script" | "style" | "textarea" | "title")
}

fn append_child(stack: &mut [Node], node: Node) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    }
}

fn close_optional_elements_for_start_tag(stack: &mut Vec<Node>, tag_name: &str) {
    if closes_open_paragraph(tag_name) {
        close_open_optional_element(stack, &["p"], &[]);
    }

    match tag_name {
        "li" => close_open_optional_element(stack, &["li"], &["ol", "ul"]),
        "thead" | "tbody" | "tfoot" => {
            close_open_optional_element(stack, &["td", "th"], &["table"]);
            close_open_optional_element(stack, &["tr"], &["table"]);
            close_open_optional_element(stack, &["thead", "tbody", "tfoot"], &["table"]);
        }
        "tr" => {
            close_open_optional_element(stack, &["td", "th"], &["table"]);
            close_open_optional_element(stack, &["tr"], &["table"]);
        }
        "td" | "th" => close_open_optional_element(stack, &["td", "th"], &["table", "tr"]),
        _ => {}
    }
}

fn close_open_optional_element(stack: &mut Vec<Node>, tag_names: &[&str], boundaries: &[&str]) {
    for tag_name in stack.iter().rev().filter_map(element_name) {
        if tag_names.contains(&tag_name) {
            let tag_name = tag_name.to_string();
            close_element(stack, &tag_name);
            return;
        }
        if boundaries.contains(&tag_name) {
            return;
        }
    }
}

fn closes_open_paragraph(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "div"
            | "dl"
            | "fieldset"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "ul"
    )
}

fn close_element(stack: &mut Vec<Node>, tag_name: &str) {
    let Some(position) = stack
        .iter()
        .rposition(|node| element_name(node) == Some(tag_name))
    else {
        return;
    };

    while stack.len() > position + 1 {
        if let Some(node) = stack.pop() {
            append_child(stack, node);
        }
    }

    if stack.len() > 1
        && let Some(node) = stack.pop()
    {
        append_child(stack, node);
    }
}

fn element_name(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Element(element) => Some(&element.tag_name),
        NodeKind::Document | NodeKind::Text(_) => None,
    }
}

#[derive(Debug, Default)]
struct VisibleText {
    output: String,
    pending_space: bool,
    pending_newlines: usize,
}

impl VisibleText {
    fn push_text(&mut self, text: &str) {
        for word in text.split_whitespace() {
            self.flush_pending_newlines();
            if self.pending_space && needs_space_before(&self.output) && !attaches_to_previous(word)
            {
                self.output.push(' ');
            }
            self.pending_space = false;
            if needs_space_before(&self.output) && !attaches_to_previous(word) {
                self.output.push(' ');
            }
            self.output.push_str(word);
            self.pending_space = true;
        }
    }

    fn push_space(&mut self) {
        if !self.output.is_empty() {
            self.pending_space = true;
        }
    }

    fn push_newlines(&mut self, count: usize) {
        if !self.output.is_empty() {
            self.pending_space = false;
            self.pending_newlines = self.pending_newlines.max(count);
        }
    }

    fn flush_pending_newlines(&mut self) {
        if self.pending_newlines > 0 {
            let trimmed_len = self.output.trim_end_matches(' ').len();
            self.output.truncate(trimmed_len);
            let existing_newlines = self
                .output
                .chars()
                .rev()
                .take_while(|character| *character == '\n')
                .count();
            for _ in existing_newlines..self.pending_newlines {
                self.output.push('\n');
            }
            self.pending_newlines = 0;
            self.pending_space = false;
        }
    }

    fn finish(mut self) -> String {
        self.output = self.output.trim().to_string();
        self.output
    }
}

fn needs_space_before(output: &str) -> bool {
    output
        .chars()
        .last()
        .is_some_and(|character| !character.is_whitespace())
}

fn attaches_to_previous(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|character| matches!(character, '.' | ',' | '!' | '?' | ';' | ':' | ')' | ']'))
}

fn collect_visible_text(node: &Node, output: &mut VisibleText) {
    match &node.kind {
        NodeKind::Text(text) => output.push_text(text),
        NodeKind::Element(element) if is_hidden_from_visible_text(&element.tag_name) => {}
        NodeKind::Element(element) if element.tag_name == "br" => output.push_newlines(1),
        NodeKind::Element(element) => {
            let block = is_block_element(&element.tag_name);
            if block {
                output.push_newlines(2);
            }
            for child in node.render_children() {
                collect_visible_text(child, output);
            }
            if block {
                output.push_newlines(2);
            } else {
                output.push_space();
            }
        }
        NodeKind::Document => {
            for child in node.render_children() {
                collect_visible_text(child, output);
            }
        }
    }
}

fn is_hidden_from_visible_text(tag_name: &str) -> bool {
    matches!(tag_name, "head" | "title" | "script" | "style" | "noscript")
}

fn is_block_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "html" | "body" | "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    )
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn dump_node(node: &Node, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    match &node.kind {
        NodeKind::Document => output.push_str("#document\n"),
        NodeKind::Element(element) => {
            output.push_str(&indent);
            output.push('<');
            output.push_str(&element.tag_name);
            for (name, value) in &element.attributes {
                output.push(' ');
                output.push_str(name);
                output.push_str("=\"");
                output.push_str(&escape_dump_string(value));
                output.push('"');
            }
            output.push_str(">\n");
        }
        NodeKind::Text(text) => {
            output.push_str(&indent);
            output.push('"');
            output.push_str(&escape_dump_string(text));
            output.push_str("\"\n");
        }
    }

    for child in &node.children {
        dump_node(child, depth + 1, output);
    }
    if !node.shadow_children.is_empty() {
        output.push_str(&indent);
        output.push_str("#shadow-root\n");
        for child in &node.shadow_children {
            dump_node(child, depth + 1, output);
        }
    }
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
        ScriptContent, collect_external_scripts, collect_icon_links, collect_iframes,
        collect_inline_scripts, collect_resource_hints, collect_scripts, collect_stylesheet_links,
        dump_dom, extract_document_title, extract_visible_text, parse_document,
        parse_document_with_diagnostics,
    };
    use webby_dom::{Document, Node, NodeKind};

    #[test]
    fn empty_input_produces_empty_document() -> webby_core::WebbyResult<()> {
        let result = parse_document("")?;

        assert_eq!(result, Document::empty());
        Ok(())
    }

    #[test]
    fn oversized_html_input_returns_structured_error() {
        let html = "x".repeat(super::MAX_HTML_INPUT_BYTES + 1);
        let result = parse_document_with_diagnostics(&html);

        assert!(matches!(
            result,
            Err(webby_core::WebbyError::Parse { message })
                if message.contains("HTML input") && message.contains("limit")
        ));
    }

    #[test]
    fn dom_node_limit_returns_structured_error() {
        let mut html = String::new();
        for _ in 0..super::MAX_DOM_NODES {
            html.push_str("<span></span>");
        }

        let result = parse_document_with_diagnostics(&html);

        assert!(matches!(
            result,
            Err(webby_core::WebbyError::Parse { message })
                if message.contains("DOM node limit")
        ));
    }

    #[test]
    fn parses_nested_normal_tags() -> webby_core::WebbyResult<()> {
        let document = parse_document("<html><body><h1>Hello</h1><p>World</p></body></html>")?;

        assert_eq!(extract_visible_text(&document), "Hello\n\nWorld");
        assert!(dump_dom(&document).contains("<h1>"));
        Ok(())
    }

    #[test]
    fn document_title_extraction_uses_first_trimmed_title() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<head><title>  First title  </title><title>Second title</title></head><body>Visible</body>",
        )?;

        assert_eq!(extract_document_title(&document), "First title");
        Ok(())
    }

    #[test]
    fn visible_text_walks_dom_order() {
        let mut document = Document::empty();
        document.root.children.push(Node::text("Hello "));
        document.root.children.push(Node::text("Webby"));

        assert_eq!(extract_visible_text(&document), "Hello Webby");
    }

    #[test]
    fn parses_quoted_and_unquoted_attributes() -> webby_core::WebbyResult<()> {
        let document = parse_document("<a href=/docs class=\"nav\" data-id='42'>Docs</a>")?;
        let Some(link) = document.root.children.first() else {
            return Err(webby_core::WebbyError::Parse {
                message: "expected link element".to_string(),
            });
        };

        match &link.kind {
            NodeKind::Element(element) => {
                assert_eq!(element.tag_name, "a");
                assert_eq!(
                    element.attributes.get("href").map(String::as_str),
                    Some("/docs")
                );
                assert_eq!(
                    element.attributes.get("class").map(String::as_str),
                    Some("nav")
                );
                assert_eq!(
                    element.attributes.get("data-id").map(String::as_str),
                    Some("42")
                );
            }
            NodeKind::Document | NodeKind::Text(_) => {
                return Err(webby_core::WebbyError::Parse {
                    message: "expected link element".to_string(),
                });
            }
        }

        Ok(())
    }

    #[test]
    fn comments_and_doctype_are_ignored() -> webby_core::WebbyResult<()> {
        let document = parse_document("<!doctype html><p>Hello<!-- hidden --> world</p>")?;

        assert_eq!(extract_visible_text(&document), "Hello world");
        assert!(!dump_dom(&document).contains("hidden"));
        Ok(())
    }

    #[test]
    fn script_and_style_are_not_visible() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>p { color: red; }</style><script>alert(1)</script><p>Visible</p>",
        )?;

        assert_eq!(extract_visible_text(&document), "Visible");
        assert!(dump_dom(&document).contains("alert(1)"));
        assert!(dump_dom(&document).contains("p { color: red; }"));
        Ok(())
    }

    #[test]
    fn unknown_tags_keep_visible_child_text() -> webby_core::WebbyResult<()> {
        let document = parse_document("<custom-card><span>Kept</span></custom-card>")?;

        assert_eq!(extract_visible_text(&document), "Kept");
        assert!(dump_dom(&document).contains("<custom-card>"));
        Ok(())
    }

    #[test]
    fn br_creates_line_break() -> webby_core::WebbyResult<()> {
        let document = parse_document("<p>Hello<br>Webby</p>")?;

        assert_eq!(extract_visible_text(&document), "Hello\nWebby");
        Ok(())
    }

    #[test]
    fn malformed_nesting_does_not_panic() -> webby_core::WebbyResult<()> {
        let document = parse_document("<div><p>Hello</div> tail</p>")?;

        assert_eq!(extract_visible_text(&document), "Hello\n\ntail");
        Ok(())
    }

    #[test]
    fn optional_end_tags_recover_common_list_table_and_paragraph_markup()
    -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<p>one<span> inline<div>block</div><ul><li>first<li>second</ul><table><thead><tr><th>A<th>B<tbody><tr><td>1<td>2<tr><td>3<td>4</table>",
        )?;
        let dump = dump_dom(&document);

        assert!(dump.contains("<p>\n    \"one\"\n    <span>\n      \" inline\"\n  <div>"));
        assert!(dump.contains("<li>\n      \"first\"\n    <li>\n      \"second\""));
        assert!(dump.contains("<th>\n          \"A\"\n        <th>\n          \"B\""));
        assert!(dump.contains("<td>\n          \"1\"\n        <td>\n          \"2\""));
        assert!(dump.contains("<td>\n          \"3\"\n        <td>\n          \"4\""));
        Ok(())
    }

    #[test]
    fn optional_end_tag_recovery_respects_nested_list_and_table_scopes()
    -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<ul><li>outer<ul><li>inner<li>second</ul><li>tail</ul><table><tr><td>outer<table><tr><td>inner</table><td>tail</table>",
        )?;
        let dump = dump_dom(&document);

        assert!(
            dump.contains("<li>\n      \"outer\"\n      <ul>\n        <li>\n          \"inner\"")
        );
        assert!(dump.contains("<li>\n          \"second\"\n    <li>\n      \"tail\""));
        assert!(dump.contains(
            "<td>\n        \"outer\"\n        <table>\n          <tr>\n            <td>\n              \"inner\""
        ));
        assert!(dump.contains("<td>\n        \"tail\""));
        Ok(())
    }

    #[test]
    fn escapable_raw_text_elements_preserve_markup_like_text_and_decode_entities()
    -> webby_core::WebbyResult<()> {
        let document =
            parse_document("<title>A &amp; <b>literal</title><textarea>x &lt; <i>y</textarea>")?;
        let dump = dump_dom(&document);

        assert!(dump.contains("<title>\n    \"A & <b>literal\""));
        assert!(dump.contains("<textarea>\n    \"x < <i>y\""));
        Ok(())
    }

    #[test]
    fn title_is_hidden_from_visible_text_even_outside_head() -> webby_core::WebbyResult<()> {
        let document = parse_document("<title>Metadata</title><p>Visible</p>")?;

        assert_eq!(extract_visible_text(&document), "Visible");
        Ok(())
    }

    #[test]
    fn unclosed_escapable_raw_text_is_preserved_with_diagnostic() -> webby_core::WebbyResult<()> {
        let parsed = parse_document_with_diagnostics("<textarea>x &amp; <b>literal")?;

        assert!(dump_dom(&parsed.document).contains("\"x & <b>literal\""));
        assert_eq!(
            parsed.diagnostics[0].message,
            "unclosed textarea element preserved through end of input"
        );
        Ok(())
    }

    #[test]
    fn raw_text_elements_do_not_decode_entities() -> webby_core::WebbyResult<()> {
        let document = parse_document("<script>const x = '&amp;';</script>")?;

        assert!(dump_dom(&document).contains("\"const x = '&amp;';\""));
        Ok(())
    }

    #[test]
    fn parser_recovery_diagnostics_are_deterministic() -> webby_core::WebbyResult<()> {
        let parsed = parse_document_with_diagnostics("<p>kept<!-- missing")?;

        assert_eq!(extract_visible_text(&parsed.document), "kept");
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].offset, 7);
        assert_eq!(
            parsed.diagnostics[0].message,
            "unclosed comment ignored through end of input"
        );
        Ok(())
    }

    #[test]
    fn foreign_content_fallback_keeps_unknown_tree_content() -> webby_core::WebbyResult<()> {
        let document = parse_document("<svg><foreignObject><p>Fallback</foreignObject></svg>")?;
        let dump = dump_dom(&document);

        assert!(dump.contains("<svg>"));
        assert!(dump.contains("<foreignobject>"));
        assert_eq!(extract_visible_text(&document), "Fallback");
        Ok(())
    }

    #[test]
    fn checked_in_optional_end_tag_fixture_matches_expected_dump() -> webby_core::WebbyResult<()> {
        let input = include_str!("../../../tests/fixtures/malformed/html-optional-end-tags.html");
        let expected =
            include_str!("../../../tests/fixtures/expected/html-optional-end-tags.dom.txt");

        assert_eq!(dump_dom(&parse_document(input)?), expected);
        Ok(())
    }

    #[test]
    fn text_whitespace_is_normalized() -> webby_core::WebbyResult<()> {
        let document = parse_document("<p> Hello\n   from\t\tWebby </p>")?;

        assert_eq!(extract_visible_text(&document), "Hello from Webby");
        Ok(())
    }

    #[test]
    fn entities_are_decoded_in_text_and_attributes() -> webby_core::WebbyResult<()> {
        let document = parse_document("<a href=\"/search?q=webby&amp;lang=rust\">A &amp; B</a>")?;

        assert_eq!(extract_visible_text(&document), "A & B");
        assert!(dump_dom(&document).contains("href=\"/search?q=webby&lang=rust\""));
        Ok(())
    }

    #[test]
    fn img_alt_width_and_height_attributes_are_preserved() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<img src=\"logo.png\" alt=\"Webby mark\" width=\"120\" height=\"80\">",
        )?;
        let dump = dump_dom(&document);

        assert!(dump.contains("alt=\"Webby mark\""));
        assert!(dump.contains("width=\"120\""));
        assert!(dump.contains("height=\"80\""));
        Ok(())
    }

    #[test]
    fn form_control_attributes_are_preserved() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<form action=\"/search\" method=\"get\"><label for=\"q\">Query</label><input type=\"search\" name=\"q\" value=\"webby\" placeholder=\"Search\"><button type=\"submit\" name=\"go\" value=\"1\">Go</button></form>",
        )?;
        let dump = dump_dom(&document);

        assert!(dump.contains("<form action=\"/search\" method=\"get\">"));
        assert!(dump.contains("<label for=\"q\">"));
        assert!(
            dump.contains(
                "<input name=\"q\" placeholder=\"Search\" type=\"search\" value=\"webby\">"
            )
        );
        assert!(dump.contains("<button name=\"go\" type=\"submit\" value=\"1\">"));
        Ok(())
    }

    #[test]
    fn richer_form_control_attributes_are_preserved() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<form action=\"/submit\" method=\"post\"><input type=\"checkbox\" name=\"ok\" value=\"yes\" checked disabled><input type=\"radio\" name=\"mode\" value=\"a\"><input type=\"password\" name=\"pw\" value=\"secret\"><input type=\"email\" name=\"mail\" value=\"a@example.test\"><select name=\"choice\"><option value=\"one\">One</option><option value=\"two\" selected>Two</option></select><textarea name=\"note\" placeholder=\"Note\">Hello</textarea><button type=\"reset\">Reset</button></form>",
        )?;
        let dump = dump_dom(&document);

        assert!(dump.contains("<form action=\"/submit\" method=\"post\">"));
        assert!(dump.contains(
            "<input checked=\"\" disabled=\"\" name=\"ok\" type=\"checkbox\" value=\"yes\">"
        ));
        assert!(dump.contains("<input name=\"mode\" type=\"radio\" value=\"a\">"));
        assert!(dump.contains("<input name=\"pw\" type=\"password\" value=\"secret\">"));
        assert!(dump.contains("<input name=\"mail\" type=\"email\" value=\"a@example.test\">"));
        assert!(dump.contains("<select name=\"choice\">"));
        assert!(dump.contains("<option selected=\"\" value=\"two\">"));
        assert!(dump.contains("<textarea name=\"note\" placeholder=\"Note\">"));
        assert!(dump.contains("<button type=\"reset\">"));
        Ok(())
    }

    #[test]
    fn iframe_attributes_are_preserved_and_collected() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<iframe src=\"child.html\" name=\"frame\" width=\"320\" height=\"180\" sandbox></iframe>",
        )?;
        let dump = dump_dom(&document);
        let iframes = collect_iframes(&document);

        assert!(dump.contains(
            "<iframe height=\"180\" name=\"frame\" sandbox=\"\" src=\"child.html\" width=\"320\">"
        ));
        assert_eq!(iframes.len(), 1);
        assert_eq!(iframes[0].src.as_deref(), Some("child.html"));
        assert_eq!(iframes[0].name.as_deref(), Some("frame"));
        assert_eq!(iframes[0].width.as_deref(), Some("320"));
        assert_eq!(iframes[0].height.as_deref(), Some("180"));
        assert_eq!(iframes[0].sandbox.as_deref(), Some(""));
        Ok(())
    }

    #[test]
    fn stylesheet_links_are_extracted_in_document_order() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<link rel=\"stylesheet\" href=\"base.css\"><link rel=\"StyleSheet preload\" href=\"theme.css\">",
        )?;
        let links = collect_stylesheet_links(&document);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].href, "base.css");
        assert_eq!(links[1].href, "theme.css");
        assert_eq!(links[1].rel, "StyleSheet preload");
        Ok(())
    }

    #[test]
    fn non_stylesheet_links_are_ignored() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<link rel=\"icon\" href=\"favicon.ico\"><link href=\"missing-rel.css\">",
        )?;

        assert!(collect_stylesheet_links(&document).is_empty());
        Ok(())
    }

    #[test]
    fn favicon_links_are_extracted_in_document_order() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<link rel=\"shortcut icon\" href=\"favicon.ico\" sizes=\"16x16\" type=\"image/x-icon\"><link rel=\"apple-touch-icon\" href=\"touch.png\">",
        )?;
        let links = collect_icon_links(&document);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].href, "favicon.ico");
        assert_eq!(links[0].sizes.as_deref(), Some("16x16"));
        assert_eq!(links[0].media_type.as_deref(), Some("image/x-icon"));
        assert_eq!(links[1].href, "touch.png");
        Ok(())
    }

    #[test]
    fn resource_hints_are_extracted_as_noop_metadata() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<link rel=\"preload\" href=\"hero.png\"><link rel=\"preconnect\" href=\"https://cdn.example\"><link rel=\"stylesheet\" href=\"site.css\">",
        )?;
        let hints = collect_resource_hints(&document);

        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].href.as_deref(), Some("hero.png"));
        assert_eq!(hints[1].rel, "preconnect");
        Ok(())
    }

    #[test]
    fn inline_scripts_are_collected_in_document_order() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<script>console.log('one')</script><script src=\"later.js\"></script><script>console.log('two')</script>",
        )?;
        let scripts = collect_inline_scripts(&document);

        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0].label, "inline script 1");
        assert_eq!(scripts[0].text, "console.log('one')");
        assert_eq!(scripts[1].label, "inline script 2");
        assert_eq!(scripts[1].text, "console.log('two')");
        Ok(())
    }

    #[test]
    fn external_scripts_are_collected_in_document_order() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<script src=\"one.js\"></script><script>console.log('inline')</script><script src=\"two.js\"></script>",
        )?;
        let scripts = collect_external_scripts(&document);

        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0].label, "external script 1");
        assert_eq!(scripts[0].src, "one.js");
        assert_eq!(scripts[1].label, "external script 2");
        assert_eq!(scripts[1].src, "two.js");
        Ok(())
    }

    #[test]
    fn scripts_are_collected_with_loading_metadata_in_document_order() -> webby_core::WebbyResult<()>
    {
        let document = parse_document(
            "<script src=\"one.js\"></script><script defer src=\"defer.js\"></script><script type=\"text/javascript\">console.log('inline')</script><script async src=\"async.js\"></script>",
        )?;
        let scripts = collect_scripts(&document);

        assert_eq!(scripts.len(), 4);
        assert_eq!(scripts[0].label, "external script 1");
        assert_eq!(scripts[1].label, "external script 2");
        assert!(scripts[1].defer);
        assert_eq!(scripts[2].label, "inline script 1");
        assert_eq!(scripts[2].script_type.as_deref(), Some("text/javascript"));
        assert_eq!(
            scripts[2].content,
            ScriptContent::Inline("console.log('inline')".to_string())
        );
        assert_eq!(scripts[3].label, "external script 3");
        assert!(scripts[3].async_attr);
        Ok(())
    }

    #[test]
    fn script_close_lookalike_does_not_truncate_script() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<script>console.log('</scripture>');console.log('</scriptfoo>');</script>",
        )?;
        let scripts = collect_inline_scripts(&document);

        assert_eq!(scripts.len(), 1);
        assert_eq!(
            scripts[0].text,
            "console.log('</scripture>');console.log('</scriptfoo>');"
        );
        Ok(())
    }

    #[test]
    fn inline_link_before_punctuation_does_not_add_extra_space() -> webby_core::WebbyResult<()> {
        let document = parse_document("<p>See <a href=\"/docs\">docs</a>.</p>")?;

        assert_eq!(extract_visible_text(&document), "See docs.");
        Ok(())
    }

    #[test]
    fn parses_unterminated_quoted_attribute_without_looping() -> webby_core::WebbyResult<()> {
        let document = parse_document("<a href=\"/docs>Docs")?;

        assert_eq!(extract_visible_text(&document), "<a href=\"/docs>Docs");
        Ok(())
    }

    #[test]
    fn repeated_less_than_inside_text_stays_text() -> webby_core::WebbyResult<()> {
        let document = parse_document("1 < 2 < 3")?;

        assert_eq!(extract_visible_text(&document), "1 < 2 < 3");
        Ok(())
    }

    #[test]
    fn unclosed_comment_is_ignored_to_end() -> webby_core::WebbyResult<()> {
        let document = parse_document("<p>Visible</p><!-- hidden forever")?;

        assert_eq!(extract_visible_text(&document), "Visible");
        assert!(!dump_dom(&document).contains("hidden"));
        Ok(())
    }

    #[test]
    fn unclosed_script_and_style_are_hidden_to_end() -> webby_core::WebbyResult<()> {
        let script_document = parse_document("<script>alert(1)")?;
        let style_document = parse_document("<style>body { color: red; }")?;

        assert_eq!(extract_visible_text(&script_document), "");
        assert_eq!(extract_visible_text(&style_document), "");
        Ok(())
    }

    #[test]
    fn raw_text_close_requires_exact_tag_name_boundary() -> webby_core::WebbyResult<()> {
        let document =
            parse_document("<script>ignored</scripture><p>Still hidden</p></script><p>Shown</p>")?;

        assert_eq!(extract_visible_text(&document), "Shown");
        assert!(dump_dom(&document).contains("Still hidden"));
        Ok(())
    }

    #[test]
    fn style_text_with_stylesheet_literal_is_not_truncated() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>p::before { content: \"</stylesheet>\"; } p { color: red; }</style><p>Shown</p>",
        )?;
        let dump = dump_dom(&document);

        assert!(dump.contains("p { color: red; }"));
        assert!(dump.contains("</stylesheet>"));
        assert_eq!(extract_visible_text(&document), "Shown");
        Ok(())
    }

    #[test]
    fn style_text_with_stylefoo_literal_is_not_truncated() -> webby_core::WebbyResult<()> {
        let document = parse_document(
            "<style>p::before { content: \"</stylefoo>\"; } p { color: blue; }</style><p>Shown</p>",
        )?;
        let dump = dump_dom(&document);

        assert!(dump.contains("p { color: blue; }"));
        assert!(dump.contains("</stylefoo>"));
        assert_eq!(extract_visible_text(&document), "Shown");
        Ok(())
    }

    #[test]
    fn dump_dom_escapes_text_quotes_newlines_and_backslashes() -> webby_core::WebbyResult<()> {
        let document = parse_document("quote \"slash \\ newline\nend")?;
        let dump = dump_dom(&document);

        assert!(dump.contains("\"quote \\\"slash \\\\ newline\\nend\""));
        Ok(())
    }

    #[test]
    fn dump_dom_escapes_attribute_quotes_entities_and_backslashes() -> webby_core::WebbyResult<()> {
        let document = parse_document("<a title=\"A &quot;quote&quot; and \\ slash\">Link</a>")?;
        let dump = dump_dom(&document);

        assert!(dump.contains("title=\"A \\\"quote\\\" and \\\\ slash\""));
        Ok(())
    }

    #[test]
    fn generated_malformed_html_inputs_do_not_panic_or_loop() -> webby_core::WebbyResult<()> {
        let fragments = [
            "<",
            "</",
            "<div",
            "<p><",
            "<a href=\"unterminated",
            "<!-- comment",
            "<style>p { color: red; }</stylefoo>",
            "<script>if (a < b) {",
            "<img src=x><br><input name=q",
            "<<<<<<",
        ];

        for repeat in 1..=8 {
            let mut html = String::new();
            for index in 0..repeat {
                let fragment = fragments[index % fragments.len()];
                html.push_str(fragment);
                html.push_str("<p>visible");
            }
            let document = parse_document(&html)?;
            let dump = dump_dom(&document);
            assert!(!dump.trim().is_empty());
        }
        Ok(())
    }
}
