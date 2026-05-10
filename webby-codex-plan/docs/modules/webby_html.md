# Module Spec: webby_html

## Purpose

Parse HTML into Webby's DOM and extract visible text.

## v0.1 parser requirements

- Opening tags.
- Closing tags.
- Attributes with quoted and unquoted values.
- Text nodes.
- Comments.
- Doctype ignored.
- Void elements: `br`, `img`, `meta`, `link`, `input`.
- Raw text ignore for `script` and `style`.
- Malformed nesting should not panic.

## Public API sketch

```rust
pub fn parse_document(input: &str) -> WebbyResult<Document>;
pub fn extract_visible_text(document: &Document) -> String;
```

## Visible text rules

- Ignore `head`, `script`, and `style` content.
- `br` creates newline.
- Block elements create paragraph-like separation.
- Collapse excessive whitespace.
- Unknown tags keep visible children.

## Tests

- nested normal tags
- malformed tags
- attributes
- comments
- script/style ignored
- links preserve `href`
- `br` creates line break
