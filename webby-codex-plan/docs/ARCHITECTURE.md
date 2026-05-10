# Webby Architecture

Webby is split into independent crates so each part can be tested without running the GUI.

## Pipeline

```text
User input
    ↓
webby_url
    ↓
webby_net
    ↓
webby_html
    ↓
webby_dom
    ↓
webby_style
    ↓
webby_layout
    ↓
webby_render
    ↓
webby_app
```

## Crates

### `webby_core`

Shared primitives:

- `WebbyError`
- `WebbyResult<T>`
- logging/tracing helpers
- shared geometry types if not owned by layout/render

Rules:

- No GUI dependency.
- No network dependency unless unavoidable.
- Must stay small.

### `webby_url`

Converts user input into a normalized target URL.

Responsibilities:

- classify input as URL, bare domain, localhost, file path, or search query
- percent-encode search queries
- normalize trailing slashes where appropriate
- expose search engine configuration

Should not:

- fetch network resources
- parse HTML
- depend on GUI

### `webby_net`

Loads resources.

Responsibilities:

- HTTP/HTTPS GET
- file loading
- redirects
- response metadata
- text decoding helpers

Should not:

- interpret HTML structure
- know layout/rendering details

### `webby_dom`

Owns document tree data structures.

Responsibilities:

- `Document`
- `Node`
- `NodeKind`
- `ElementData`
- attributes
- tree traversal helpers

Should not:

- parse HTML text directly unless through helper constructors for tests
- know rendering details

### `webby_html`

HTML tokenizer and parser.

Responsibilities:

- tokenize markup
- build DOM
- parse attributes
- handle comments, doctypes, raw script/style blocks
- extract visible text

Should not:

- layout pixels
- fetch URLs

### `webby_css`

CSS tokenizer/parser, added after default rendering works.

Responsibilities:

- parse stylesheet text
- parse selectors
- parse declarations
- produce structured CSS rules

Should not:

- directly mutate DOM
- perform layout

### `webby_style`

Converts DOM + CSS/defaults into styled tree.

Responsibilities:

- user-agent default styles
- selector matching
- specificity
- computed values

Should not:

- parse raw HTML
- draw pixels

### `webby_layout`

Computes geometry.

Responsibilities:

- viewport
- block flow
- inline text wrapping
- line boxes
- clickable link rectangles
- scroll extents

Should not:

- call network
- parse HTML strings
- draw directly to the window

### `webby_render`

Turns layout into display commands and pixels.

Responsibilities:

- display list
- text drawing
- rectangles/lines/backgrounds
- image placeholders initially
- render snapshots for tests

Should not:

- know parser internals
- own navigation state

### `webby_app`

Native browser shell.

Responsibilities:

- window/event loop
- address/search bar
- input handling
- scroll handling
- navigation state
- calls the engine pipeline
- displays error pages

Should not:

- contain parsing/layout algorithms directly

### `webby_cli`

Debug and validation tool.

Commands:

- `--resolve-input`
- `--fetch`
- `--dump-dom`
- `--dump-text`
- `--dump-style`
- `--dump-layout`
- `--render-snapshot`

Should be used heavily by Codex for validation.

## Data ownership

- Parsed DOM owns strings and attributes.
- Styled tree may reference DOM or own IDs depending on ergonomics.
- Layout tree should not require mutable access to DOM.
- Display list owns draw commands and can be rendered repeatedly.

## Error handling

Every crate should return structured errors. GUI-facing errors should be convertible to readable error pages.

Bad:

```rust
panic!("failed")
```

Good:

```rust
return Err(WebbyError::Parse { message, span });
```

## Testing strategy

- `webby_url`: pure unit tests.
- `webby_net`: fixture/file tests and minimal smoke tests.
- `webby_html`: tokenizer/parser unit tests.
- `webby_style`: default style and selector tests.
- `webby_layout`: deterministic geometry tests.
- `webby_render`: display-list snapshot tests first, pixel snapshots later.
- `webby_app`: limited smoke/manual tests.
