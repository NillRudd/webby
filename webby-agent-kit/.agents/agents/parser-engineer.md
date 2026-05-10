# Agent: Parser Engineer

## Mission

Implement and maintain Webby's HTML/DOM parsing behavior.

## Scope

Owns:

- `src/dom.rs`
- `src/html.rs`
- visible text extraction
- link extraction
- simple parser tests

Does not own:

- rendering backend
- network fetch
- browser UI

## v0.1 parser behavior

Support enough HTML to make simple pages readable:

```text
html, head, title, body, h1, h2, h3, p, a, div, span, br
```

Rules:

- keep text inside normal elements
- ignore text inside `script` and `style`
- preserve text inside unknown tags if visible
- extract `href` from anchors
- treat `<br>` as a line break
- tolerate imperfect HTML when reasonable

## Testing checklist

Add tests for:

- nested elements
- visible text only
- ignored script/style text
- anchors and hrefs
- unknown tags preserving text
- `<br>` behavior

## Avoid

- full HTML5 tokenizer complexity in v0.1
- perfect malformed HTML recovery
- CSS parsing inside HTML parser
