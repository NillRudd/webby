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
    ├──> webby_js diagnostics
    ↓
webby_stylesheet → webby_css → webby_style
    ↓                  webby_image
webby_layout  ←────── decoded image metadata
    ↑
webby_text
    ↓
webby_render
    ↓
webby_app

webby_state is a sidecar profile persistence crate used by webby_app and
webby_cli. It does not participate in the page render pipeline.

webby_cache is a sidecar resource-byte cache used by app and CLI pipeline
coordinators around `webby_net` loads. It does not parse, decode, style,
layout, or render resources.
```

## Diagrams

### Page Load

```text
address/search input
        |
        v
webby_url resolves URL
        |
        v
webby_app / webby_cli coordinate loaders
        |
        +--> webby_cache checks/stores bytes
        |
        +--> webby_state cookie jar supplies/stores cookie headers
        |
        v
webby_net loads page bytes
        |
        v
webby_html parses DOM and exposes resource metadata
        |
        +--> webby_stylesheet loads linked CSS through webby_net
        +--> webby_image loads/decodes images through webby_net
        +--> webby_app loads iframe documents through PagePipeline
        +--> webby_js executes inline scripts and reports DOM/browser actions
        |
        v
webby_css parses CSS rules
        |
        v
webby_style applies cascade/defaults/media queries/pseudo-classes
        |
        v
webby_layout computes block/inline/flex/grid/table geometry using webby_text metrics
        |
        v
webby_render builds display list and RGBA surface
        |
        v
webby_app presents the active tab
```

JavaScript browser actions such as `location.href`, `history.back`, and
`setTimeout` flow back to `webby_app` as structured requests. The app shell
applies them through the same navigation/history/timer state used by native UI
input; `webby_js` never loads resources or mutates history directly.

For JavaScript `fetch`/`XMLHttpRequest` v0.1, the app/CLI pipeline resolves
static request URLs against the page URL, loads bytes through `webby_net`, and
applies `webby_security` same-origin/CORS checks before script execution.
`webby_js` receives only decoded response snapshots or controlled security
errors for its JavaScript shims, keeping network ownership outside the JS crate
and layout/render/style unaware of network requests.

Iframe pages follow the same forward pipeline. `webby_app` coordinates nested
browsing contexts by collecting iframe metadata from `webby_html`, resolving
and loading iframe resources through the active loader, rendering each child
document with a child `PagePipeline`, and supplying the resulting surface to
layout/render as replaced-element image metadata. `webby_layout` and
`webby_render` never fetch or parse iframe documents; they only consume the
caller-supplied geometry and pixels.

For Web Storage, `webby_state` owns persisted `localStorage` data and uses the
canonical `webby_security` origin key. `webby_app` passes per-origin
local/session storage snapshots into `webby_js` and applies structured storage
actions after script/event/timer execution. `sessionStorage` stays in the active
tab/session model and never persists; layout, style, render, HTML, CSS, and
network crates do not know about storage policy.

Inline SVG and canvas follow the same forward pipeline. HTML/DOM preserves the
elements and attributes, style computes normal display and sizing properties,
layout turns them into replaced boxes with paint-facing drawing metadata, and
render consumes that metadata as display commands. JavaScript canvas APIs record
deterministic canvas commands through DOM attributes; they do not draw pixels
directly. Basic SVG `viewBox` scaling is resolved in layout because it maps
element-local geometry into the laid-out viewport.

### Debug And CLI Dumps

```text
DOM dump        HTML -> webby_html
CSS dump        HTML -> webby_html -> webby_stylesheet/webby_css
Style dump      HTML/CSS -> webby_style
Layout dump     style tree -> webby_layout
Display dump    layout tree -> webby_render display list
Render PPM      display list -> webby_render software surface
Diagnostics     loader/cache/stylesheet/image/form/JavaScript diagnostics
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
- HTTP/HTTPS `application/x-www-form-urlencoded` POST for form submission
- file loading
- data URL loading for static resource bytes
- redirects
- caller-provided request headers for stateful loads
- response metadata
- response headers for coordinators such as cookie handling
- small MIME/content-type sniffing helpers for common static resources
- text decoding helpers

Should not:

- interpret HTML structure
- know layout/rendering details

### `webby_security`

Owns reusable origin and security-policy decisions.

Responsibilities:

- deterministic origin model for `http`, `https`, and Webby's shared local-file
  origin
- same-origin checks
- CORS checks for JavaScript `fetch`/`XMLHttpRequest` resources
- mixed-content diagnostics for HTTPS pages referencing HTTP subresources

Should not:

- load resources
- parse HTML/CSS
- execute JavaScript
- own app navigation, cookies, storage, layout, or rendering

### `webby_cache`

Stores successful resource bytes loaded through `webby_net`.

Responsibilities:

- key entries by requested URL string
- store response metadata, byte length, and deterministic store sequence
- enforce count and byte limits
- evict by deterministic least-recently-used policy
- expose hit/miss/store/refresh/evict diagnostics

Should not:

- fetch resources directly without a caller-provided loader
- parse CSS or HTML
- decode images
- know layout or rendering details

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
- expose inline/external script metadata in document order
- expose stylesheet, favicon/icon, and resource-hint metadata in document order
- extract visible text

Should not:

- layout pixels
- fetch URLs

### `webby_js`

Minimal JavaScript execution.

Responsibilities:

- integrate the embeddable `boa_engine` JavaScript engine
- execute ordered script text supplied by app/CLI pipeline coordinators
- provide `console.log`, `window`, and Webby's small `document` binding
- register and dispatch Webby's small DOM event subset for app-selected
  targets
- record JavaScript DOM operations and apply them through `webby_dom`
- return deterministic console/error diagnostics
- enforce Webby's script count, script byte, and loop-iteration limits

Should not:

- fetch external scripts
- own DOM tree mutation algorithms directly
- style, layout, or render pixels
- depend on app, CLI, layout, render, style, CSS, or network crates

### `webby_script`

Shared script resource coordination for app and CLI callers.

Responsibilities:

- resolve external classic and module script URLs relative to the document URL
- load script bytes through caller-provided `webby_net` resource loaders
- build deterministic classic script execution order
- build small static module graphs, cache modules within one graph, and detect
  cycles
- return ordered `webby_js::ScriptSource` values and deterministic diagnostics

Should not:

- execute JavaScript
- own DOM mutation or browser navigation
- perform style, layout, or rendering

### `webby_css`

CSS tokenizer/parser, added after default rendering works.

Responsibilities:

- parse stylesheet text
- parse selectors
- parse scoped pseudo-class selectors
- parse declarations
- produce structured CSS rules
- parse Webby's scoped sizing values, including `auto`, percentages,
  min/max sizing, and `box-sizing`
- parse Webby's scoped display/visibility values
- parse Webby's scoped positioning and offset values
- parse Webby's scoped flexbox and grid values
- parse Webby's scoped `@media` values
- parse Webby's scoped transition values and simple transition shorthand
- parse Webby's scoped text values: font family category, pixel line height,
  text alignment, and white-space mode

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
- display/visibility inheritance and cascade semantics
- media query matching against caller-provided viewport context
- pseudo-class matching against caller-provided interaction context and DOM
  state
- transition cascade metadata for later app-driven visual interpolation
- position/offset cascade semantics
- flex container/item computed values
- default table element styling
- static-document defaults for lists, inline emphasis, code/pre, blockquote,
  horizontal rules, figures, and captions
- text cascade for font family, line height, text alignment, and white-space

Should not:

- parse raw HTML
- own animation clocks
- draw pixels

### `webby_stylesheet`

Shared external stylesheet resource coordination for app and CLI callers.

Responsibilities:

- resolve linked stylesheet URLs relative to the document URL
- load stylesheet bytes through caller-provided `webby_net` resource loaders
- parse linked CSS through `webby_css`
- return deterministic non-fatal stylesheet diagnostics

Should not:

- own CSS cascade or computed styles
- perform layout or rendering

### `webby_text`

Shared deterministic text measurement and glyph rasterization.

Responsibilities:

- load the default font source
- expose one authoritative text measurement API
- rasterize glyph coverage for the software renderer
- provide a deterministic fallback when a system font is unavailable

Should not:

- know DOM, CSS, or layout tree internals
- own browser navigation or GUI state

### `webby_image`

Collects image references, resolves image URLs, and decodes image bytes.

Responsibilities:

- find `img src` and `video poster` references in DOM order
- resolve image URLs relative to the document URL
- decode PNG and JPEG bytes into RGBA pixels
- provide decoded intrinsic dimensions to layout

Should not:

- fetch resources without a caller-provided `webby_net::ResourceLoader`
- layout boxes
- draw pixels

### `webby_layout`

Computes geometry.

Responsibilities:

- viewport
- block flow
- inline text wrapping
- inline and inline-block participation in ordered inline flow
- content-box and border-box sizing
- margin/padding/border box geometry
- percentage block width resolution and min/max clamps
- relative and absolute positioned block geometry
- scoped flex row/column layout
- scoped grid layout and item placement
- scoped table row/column/cell layout
- deterministic list marker and preformatted text layout
- deterministic text alignment, line-height overrides, and white-space handling
- deterministic `sans`/`monospace` font-category metrics through `webby_text`
- use caller-supplied decoded image metadata for intrinsic image sizing
- carry inline SVG/canvas drawing metadata while owning only geometry
- line boxes
- clickable link rectangles
- hit-region suppression for non-visible content
- final positioned link/form hit rectangles
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
- circles and SVG/canvas primitive commands from layout metadata
- deterministic image placeholders for failed or missing decoded images
- decoded image pixel blitting
- deterministic PPM/software-render output for tests

Should not:

- know parser internals
- fetch or decode images
- own navigation state

### `webby_app`

Native browser shell.

Responsibilities:

- window/event loop
- address/search bar
- clickable browser chrome and tab strip state
- input handling
- hover feedback state
- deterministic animation clock for scoped visual transitions
- scroll handling
- tab/session state
- per-tab navigation, chrome, page status, scroll, and form edit state
- calls the engine pipeline
- displays error pages
- composes persistent profile APIs for homepage, bookmarks, recent pages, and
  successful navigation history
- coordinates cookie sending/storage around resource loaders according to
  profile privacy config
- uses `webby_script` to coordinate external classic scripts and module graphs
  before passing ordered script sources into `webby_js`
- supplies `webby_js` with viewport and current layout geometry snapshots for
  DOM/Web APIs such as `window.innerWidth` and `getBoundingClientRect`
- composes transition frames from display lists without moving style, layout, or
  rendering ownership into app state
- coordinates iframe nested browsing contexts through the same loader-backed
  page pipeline

Should not:

- contain parsing/layout algorithms directly
- parse persistence file formats directly

### `webby_state`

Persistent browser profile state.

Responsibilities:

- deterministic JSON config/history/bookmarks/recent/cookie files
- documented profile directory resolution
- missing-file defaults
- structured errors for corrupt profile files and I/O failures
- bookmark and successful-history persistence APIs
- cookie jar, `Set-Cookie` subset parsing, domain/path matching, session versus
  persistent cookies, and clear-cookies profile API

Should not:

- parse HTML, CSS, or DOM
- own navigation algorithms
- fetch page resources
- layout or render pages

### `webby_cli`

Debug and validation tool.

Commands:

- `--resolve-input`
- `--fetch`
- `--dump-dom`
- `--dump-text`
- `--dump-css`
- `--dump-style`
- `--dump-layout`
- `--dump-display-list`
- `--dump-diagnostics`
- `--render-ppm`
- `--show-config`
- `--list-history`
- `--list-bookmarks`
- `--add-bookmark`
- `--remove-bookmark`
- `--clear-cookies`

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
return Err(WebbyError::Parse { message });
```

## Testing strategy

- `webby_url`: pure unit tests.
- `webby_net`: fixture/file tests and minimal smoke tests.
- `webby_cache`: cache policy, diagnostics, and repeated-load tests.
- `webby_html`: tokenizer/parser unit tests.
- `webby_style`: default style and selector tests.
- `webby_layout`: deterministic geometry tests.
- `webby_render`: display-list and pixel-level software-render tests.
- `webby_app`: testable state and pipeline tests, plus manual/native smoke when relevant.
