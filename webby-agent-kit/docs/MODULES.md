# Webby Module Reference

This file is the canonical module/crate reference. `docs/ARCHITECTURE.md`
defines the crate boundaries; this file records current behavior and useful
public APIs.

## `webby_core`

Shared low-level types:

- `WebbyError`
- `WebbyResult<T>`

Errors should carry enough context to debug the failing layer. `webby_core`
must stay small and avoid GUI, parser, layout, render, and network policy.

## `webby_url`

Resolves address/search input and form targets.

Responsibilities:

- preserve explicit `http://`, `https://`, and `file:` URLs
- infer bare domains as `https://`
- infer localhost and loopback targets as `http://`
- resolve existing file paths as `file://`
- turn other text into search URLs
- serialize successful GET form controls as query parameters
- serialize `application/x-www-form-urlencoded` POST bodies
- resolve POST form action URLs against the current page URL

GET form behavior:

- `action` resolves against the current page URL
- missing or empty action uses the current page URL
- empty field names are skipped
- spaces, unicode, and URL symbols are encoded deterministically

POST form behavior:

- `method="post"` uses `application/x-www-form-urlencoded`
- `action` resolves against the current page URL
- missing or empty action uses the current page URL
- body field ordering follows layout/document control order

## `webby_net`

Loads resources from URLs.

Responsibilities:

- `http`, `https`, `file`, and `data` loading
- optional request headers through `ResourceLoader::load_with_headers`
- URL-encoded form submission through `ResourceLoader::submit_form_urlencoded`
- response metadata such as requested URL, final URL, status, content type,
  response headers, and bytes
- small deterministic content-type sniffing for common image and HTML bytes
- UTF-8 text decoding helper

Must not parse HTML, style documents, layout boxes, draw pixels, or own browser
navigation.

## `webby_security`

Defines Webby's small deterministic security model.

Responsibilities:

- origin keys for `http`, `https`, and shared local `file://` pages
- same-origin checks
- CORS decisions for JavaScript `fetch`/`XMLHttpRequest`
- deterministic mixed-content diagnostics

Current behavior:

- HTTP(S) origins are `scheme://host:port` with known default ports filled in.
- `file://` pages share `file://local`; Webby does not implement opaque or
  per-directory file origins yet.
- Same-origin JavaScript requests are allowed.
- Cross-origin JavaScript requests require `Access-Control-Allow-Origin: *` or
  an exact Webby origin key such as `https://example.test:443`.
- Failed CORS checks become deterministic page diagnostics and controlled
  JavaScript errors.
- HTTPS pages that reference HTTP images, stylesheets, or scripts get
  deterministic mixed-content diagnostics. Webby warns but does not block those
  resources yet.

## `webby_cache`

Owns Webby's in-memory resource byte cache.

Policy:

- cache key is the requested URL string
- only successful byte loads are stored
- page resources, external stylesheets, and images can share the same cache via
  `CachedResourceLoader`
- default limits are 64 entries and 8 MiB of response body bytes
- eviction is deterministic least-recently-used, with key order as a tie-breaker
- normal address, link, and form navigations may reuse cached resources
- manual reload uses refresh mode: bypass lookup, reload bytes, and replace the
  cache entry on success
- back/forward traversal currently uses refresh mode too, so traversal failures
  still preserve the previous successful URL/history state instead of being
  masked by a cached page
- downstream image decode failures still leave the successfully loaded bytes in
  the byte cache; rendering keeps using deterministic image placeholders when
  decode fails

Diagnostics are deterministic strings for hit, miss, store, refresh,
load-failed, skip-store, and evict events.

## `webby_dom`

Represents Webby's document tree.

Core types:

- `Document`
- `Node`
- `NodeKind`
- `ElementData`

Requirements:

- stable traversal order
- normalized HTML tag names
- deterministic attribute storage
- no rendering or networking logic

## `webby_html`

Parses HTML into DOM, extracts visible text, and exposes HTML-owned resource
metadata.

Current scope:

- opening and closing tags
- quoted and unquoted attributes
- text nodes
- comments and ignored doctypes
- void elements: `br`, `img`, `meta`, `link`, `input`
- form attributes: `form action/method`, `input type/name/value/placeholder`,
  `input checked/disabled`, `button type/name/value`, `label for`,
  `select/option value/selected/disabled`, and `textarea placeholder`
- iframe attributes: `src`, `width`, `height`, `name`, and `sandbox`
- raw text handling for `script` and `style`
- stylesheet link extraction
- favicon/icon link extraction
- preload/preconnect resource-hint extraction for deterministic no-op
  diagnostics
- inline script extraction in document order
- visible text extraction that ignores `head`, `script`, and `style`

Malformed HTML should not panic or infinite-loop.

## `webby_css`

Parses Webby's small CSS subset.

Selectors:

- tag
- `.class`
- `#id`
- `tag.class`
- `tag#id`
- descendant
- child
- grouped
- universal
- multiple classes
- simple attribute existence/equality selectors
- scoped pseudo-classes: `:hover`, `:focus`, `:active`, `:checked`,
  `:disabled`, `:first-child`, `:last-child`, and simple `:nth-child(n)`

Declarations:

- `color`
- `background`
- `background-color`
- `font-size`
- `font-family`
- `line-height`
- `font-weight`
- `text-decoration`
- `text-align`
- `white-space`
- `display`
- `visibility`
- `position`
- `top`
- `right`
- `bottom`
- `left`
- `flex-direction`
- `gap`
- `row-gap`
- `column-gap`
- `grid-template-columns`
- `grid-template-rows`
- `grid-column`
- `grid-row`
- `justify-content`
- `align-items`
- `flex-grow`
- `flex`
- `margin`
- `padding`
- `border`
- `border-width`
- `border-color`
- `width`
- `height`
- `min-width`
- `max-width`
- `min-height`
- `max-height`
- `box-sizing`
- `transition-property`
- `transition-duration`
- `transition-delay`
- `transition-timing-function`
- `transition`

Sizing values support non-negative `px` lengths, `auto`, and percentages.
Percentage widths resolve against the containing block. Percentage heights are
parsed and carried through computed style, but are only meaningful in layout
when the containing height is definite; ordinary block flow usually has auto
height. Margin and padding support one-, two-, three-, and four-value shorthand
forms. Border shorthand supports width/color tokens in Webby's scoped subset.
`display` supports `block`, `inline`, `inline-block`, `flex`, `grid`, and
`none`.
`visibility` supports `visible` and `hidden`.
`position` supports `static`, `relative`, and `absolute`. Offsets support the
same scoped size values as width/height. `z-index` is not implemented yet.
Flex support includes `flex-direction: row|column`, pixel `gap`,
`justify-content: flex-start|center|space-between`,
`align-items: stretch|center|flex-start`, `flex-grow`, and simple numeric
`flex`.
Grid support includes `display: grid`, explicit `grid-template-columns` and
`grid-template-rows`, `gap`/`row-gap`/`column-gap`, and simple `grid-column` /
`grid-row` line placement. Grid tracks support fixed `px`, percentages, `fr`,
and `auto`; unsupported features such as `repeat()`, named lines, and `span`
placement are skipped by the forgiving declaration parser.
Media query support includes `@media screen` and `@media all` with
`min-width`, `max-width`, `width`, and optional `orientation` features.
Unsupported media queries are skipped with diagnostics; matching is evaluated
by `webby_style` against the caller-provided viewport.
Pseudo-class selectors are parsed by `webby_css` and matched by
`webby_style`. App-provided interaction state drives `:hover`, `:focus`, and
`:active`; DOM attributes and tree position drive `:checked`, `:disabled`, and
child-position pseudo-classes.
Transition support is intentionally scoped to deterministic visual transitions:
`transition-property`, `transition-duration`, optional `transition-delay`,
`transition-timing-function: linear|ease`, and simple `transition` shorthand
are parsed for `color`, `background-color`, `left`, and `top`. Unsupported
transition properties are skipped with diagnostics.
Table elements (`table`, `thead`, `tbody`, `tr`, `th`, `td`) have default
block-level user-agent-like styles. Header cells are bold with a light
background; header and body cells have simple padding and borders.
Common static-document elements have default behavior: semantic sectioning
elements are blocks, lists have indented block items with deterministic marker
fragments, `strong` is bold, `small` is smaller, `pre`/`code` use the
deterministic monospace font category, `pre` preserves whitespace, `blockquote`
has inset/border styling, and `hr` is a simple bordered rule.

Text declarations currently support deterministic font-family categories
(`sans` and `monospace`), `line-height: normal|<px>`, `text-align:
left|center|right`, and `white-space: normal|pre|nowrap`.

Malformed rules, unsupported selectors, and unsupported declarations are skipped
with diagnostics. Diagnostic offsets are best-effort debugging positions, not a
stable public contract.

## `webby_stylesheet`

Coordinates external stylesheet resources for app and CLI callers.

Responsibilities:

- collect stylesheet links exposed by `webby_html`
- resolve stylesheet URLs relative to the page URL
- load bytes through a caller-provided `webby_net::ResourceLoader`
- parse CSS through `webby_css`
- return parsed stylesheets plus deterministic non-fatal diagnostics

It does not own CSS cascade, layout, or rendering.

## `webby_script`

Coordinates script resources for app and CLI callers.

Responsibilities:

- collect script metadata exposed by `webby_html`
- resolve classic and module script URLs relative to the document URL
- load external script bytes through caller-provided `webby_net` loaders
- build deterministic classic script order: blocking, then `defer`, then
  Webby's simplified deterministic `async`
- build deterministic JavaScript module graphs for static local/relative
  imports
- detect module cycles without looping
- return ordered `webby_js::ScriptSource` values plus non-fatal diagnostics

It does not execute JavaScript, own DOM mutation, style, layout, or rendering.

Current module scope is small. Supported imports are static side-effect imports
such as `import "./dep.js"` and simple `from` imports such as
`import { value } from "./dep.js"`. Import bindings are not wired yet; modules
are executed dependency-first for side effects. `export` keywords are stripped
before execution. Unsupported or dynamic import syntax skips that module with a
diagnostic. Import maps are not implemented.

## `webby_js`

Executes Webby's first small JavaScript subset.

Current scope:

- ordered classic and module script sources supplied by `webby_script`
- deterministic `console.log` capture
- `window.innerWidth` and `window.innerHeight` from caller-provided viewport
  context
- small `document` API: `getElementById`, `querySelector`,
  `querySelectorAll`, `documentElement`, `body`, `title`, `createElement`, and
  `createTextNode`
- element/node API: `textContent`, `innerText`, `appendChild`, `remove`,
  `setAttribute`, `getAttribute`, `className`, `classList`, `dataset`,
  inline `style`, `children`, `childNodes`, sibling/parent/first/last accessors,
  `matches`, `closest`, `getBoundingClientRect`, `clientWidth`,
  `clientHeight`, `scrollWidth`, `scrollHeight`, and `id`
- small DOM event API: `addEventListener` for `click`, `input`, and `submit`,
  plus `onclick`, bubbling, `preventDefault`, and deterministic handler
  diagnostics
- browser API shims for `location.href`, `location.assign`,
  `history.back`, `history.forward`, `setTimeout`, and `clearTimeout`
- deterministic JavaScript `fetch`/`XMLHttpRequest` v0.1 shims: `GET` only,
  static string request URLs, CORS-checked response snapshots, `status`, `ok`,
  `url`, and `text()` response body
- Web Storage API shims for `localStorage` and `sessionStorage`: `getItem`,
  `setItem`, `removeItem`, `clear`, `key`, and `length`
- canvas `getContext("2d")` with deterministic `fillRect`, `strokeRect`,
  `clearRect`, `fillStyle`, `strokeStyle`, and `lineWidth` command recording
- syntax/runtime errors become non-fatal diagnostics
- configurable enable/disable through `BrowserConfig.javascript_enabled`
- deterministic script count, per-script byte, and loop-iteration limits

`boa_engine` is used because Webby needs a real embeddable JavaScript parser and
interpreter without using Chromium, V8, WebKit, Blink, Electron, platform
WebView, or another full browser engine.

Browser API calls are reported as structured actions. `webby_app` applies
navigation/history/timer effects through its existing app-state model, so
`webby_js` does not own browsing state. Script metadata is collected by
`webby_html`; script loading, ordering, and module graph construction are owned
by `webby_script`. Fetch/XHR resources are preloaded by the app/CLI pipeline
through the normal resource loader before script execution; `webby_js` only
exposes those bytes to JavaScript. When the caller uses the cached page
pipeline, external scripts and JavaScript request resources go through that same
shared resource cache. The direct `render_html(...)` string path does not fetch
linked external CSS, external scripts, modules, or JavaScript request resources.

Storage reads use caller-provided snapshots. Storage writes are returned as
structured actions; `webby_app` applies them to persistent `localStorage` and
per-tab `sessionStorage`. Quota overflow and disabled storage become controlled
JavaScript diagnostics.

Script, event, timer, fetch, and XHR DOM mutations return explicit dirty-state
metadata for DOM, style, layout, and render invalidation. `webby_app` then
recomputes through the normal style/layout/display-list/render pipeline using
the already loaded page resources for pure DOM/style changes.

Layout-backed DOM geometry is supplied by callers as a stable node-id rectangle
snapshot. Initial page-load scripts receive viewport values before layout is
computed; event/timer callbacks in `webby_app` receive geometry from the current
rendered page.

Selector forms outside Webby's supported CSS subset, keyboard events beyond the
current app bridge, dynamic fetch URL expressions, real Promises, intervals,
import maps, module import bindings, and broad network APIs are not
implemented yet. Timer callbacks are deterministic and test-driven; Webby does
not run a real wall-clock JavaScript event loop. This crate does not fetch
resources, compute style/layout, or render pixels. DOM mutation operations are
applied through `webby_dom` before the normal style/layout/render pipeline runs.

## `webby_style`

Converts DOM into styled nodes by combining defaults, external CSS, style
blocks, and inline styles.

Requirements:

- computed display, color, background, font size, font family, line height,
  font weight, text decoration, text alignment, white-space, visibility,
  margin, padding, border, width/height, min/max sizing, box sizing, and
  flex/grid values
- default styles for document, text, common semantic block elements, lists,
  links, images, and form controls
- hidden metadata/script/style content excluded from the style tree
- `display:none` removes styled nodes before layout
- `visibility:hidden` is inherited and preserved for layout/render/hit-testing
  decisions
- cascade order: defaults, external stylesheets in document order, style blocks
  in document order, inline styles
- selector specificity: universal lowest; tag element-level; class/attribute
  and pseudo-class class-level; id id-level; combinators sum their parts;
  inline highest
- viewport-aware media query matching for parsed `@media` rules
- interaction-aware pseudo-class matching for app-supplied hover/focus/active
  DOM node ids and DOM-backed checked/disabled/child selectors

`webby_style` owns selector matching and cascade behavior.

## `webby_text`

Owns deterministic text measurement and glyph rasterization.

Font strategy:

1. `WEBBY_FONT_PATH`, when set.
2. Known platform font paths.
3. Deterministic built-in fallback raster path.

Layout and rendering use the same measurement API. Webby exposes deterministic
`sans` and `monospace` font categories; monospace has a built-in deterministic
fallback path so `code` and `pre` remain stable when no system font is
available. Non-goals include arbitrary webfont loading, script shaping, bidi
text, ligatures, and emoji rendering.

## `webby_image`

Owns image reference collection and decoding.

Responsibilities:

- collect `img src` and `video poster` references from DOM
- resolve image URLs relative to document URL
- decode PNG and JPEG to RGBA8
- expose intrinsic dimensions to layout
- convert decoded resources into layout-facing image metadata
- report direct URL/decode errors as structured errors
- skip failed page-image loads/decodes in bulk loading so placeholders can be
  used downstream

Uses the `image` crate with default features disabled and PNG/JPEG enabled.

## `webby_layout`

Computes geometry from styled content.

Current behavior:

- vertical block flow
- ordered inline line boxes for text, span, link, image, media placeholders,
  and `br`
- `inline-block` currently participates in the same deterministic inline flow
  as inline replaced content
- shared `webby_text` metrics
- deterministic baseline and line-height handling
- `line-height` pixel overrides for inline line boxes
- `text-align: center|right` for simple block line boxes
- `white-space: normal|pre|nowrap` in scoped inline text layout
- paragraph and heading spacing
- link hit rectangles, including wrapped and nested inline content
- form control boxes and hit regions
- inline SVG and canvas replaced-element sizing from CSS/HTML dimensions,
  including basic SVG `viewBox` coordinate scaling
- audio/video placeholder sizing from CSS/HTML dimensions and video poster
  image metadata when available
- iframe replaced-box sizing from CSS/HTML dimensions and caller-supplied
  nested-page surfaces when available
- paint-facing SVG/canvas command metadata carried to render
- `visibility:hidden` preserves geometry but suppresses link/form hit regions
  and paint-facing image visibility
- scroll height
- image sizing precedence: CSS size, HTML attributes, decoded intrinsic size,
  then placeholder defaults
- block width/height resolution for content-box and border-box sizing
- percentage block widths resolved against the containing block
- min/max size clamps for block layout
- `position:relative` visual offsets without changing normal block flow
- `position:absolute` for block children, removed from normal block flow and
  positioned against the parent content box
- positioned link/form hit regions use final visual rectangles
- deterministic positioned-box metadata in layout dumps
- scoped flex row/column layout with gap, basic grow distribution, simple
  justify/cross-axis alignment, and deterministic source-order painting
- scoped grid layout with fixed, percentage, `fr`, and `auto` tracks, simple
  row-major auto-placement, explicit `grid-column`/`grid-row` line placement,
  row/column gaps, and deterministic source-order painting
- scoped table layout for `table`, `thead`, `tbody`, `tr`, `th`, and `td`:
  deterministic equal column widths, rows stacked top-to-bottom, simple cell
  padding/borders, and source-order painting
- deterministic list marker fragments for `li`
- preformatted text layout for `pre` and `white-space: pre`, preserving spaces
  and line breaks
- simple `hr` and blockquote box geometry through normal layout boxes

Current limitations: positioned inline text is laid out through the normal
inline flow, stacking/z-index is not implemented, flexbox and grid support are
small deterministic subsets rather than full CSS algorithms, and table layout
intentionally ignores colspan, rowspan, and advanced border-collapse.
Long words and nowrap text are clipped to the available run width rather than
overflowing indefinitely or triggering wrapping loops.

Layout owns geometry only. It does not fetch resources, parse HTML/CSS, draw
pixels, or own interaction state.

## `webby_render`

Converts layout output into display commands and pixels.

Responsibilities:

- display list construction
- deterministic display-list dumps
- rectangles, borders, lines, circles, text, form controls, SVG/canvas drawing
  commands, and image pixel blitting
- safe clipping
- PPM output

Renderer consumes layout/display-list data. It does not parse HTML, fetch
resources, decode images, own navigation, or own interaction state.

## `webby_app`

Native browser shell and testable app state.

Owns:

- native window adapter
- tab/session state for multiple independent browsing contexts
- browser chrome state
- address/search input behavior
- clickable tab strip and browser chrome controls
- navigation state
- link hit testing
- hover target feedback for chrome, links, and form controls
- scroll state
- form focus/editing/submission state
- profile composition for homepage, bookmarks, recent pages, and persistent
  successful history
- shared resource cache for page, stylesheet, and image loads
- iframe browsing contexts coordinated through the normal loader-backed page
  pipeline
- session cookie jar and cookie privacy config
- deterministic animation clock and transition enable/disable state
- loading and error states

Tabs:

- `AppState` keeps a compatibility view of the active tab while `BrowserTab`
  stores each tab's chrome, navigation, status, rendered page, scroll offset,
  and form editing state. `BrowserTab` is the authoritative tab/session state;
  the compatibility view is synchronized by tab operations and covered by
  `validate_active_tab_sync()`.
- normal navigation, reload, back, forward, scrolling, and form edits apply to
  the active tab only.
- closing the active tab selects the next tab at that index, or the previous
  tab when closing the last tab.
- the last tab cannot be closed by tab commands.
- profile config, bookmarks, recent pages, and persistent successful history
  remain app/profile-level state rather than per-tab state.

Does not own parser, style, layout, rendering, URL resolution, networking,
stylesheet, image, text, cache policy, or persistence algorithms.

Iframe behavior is intentionally small: `webby_app` loads iframe documents,
stores their independent rendered page, URL, diagnostics, and scroll offset,
and exposes link navigation inside that nested context. Layout/render see an
iframe as a replaced box with caller-supplied pixels. Nested iframes are
diagnostic-only for now, and `sandbox` is recorded as a deterministic
diagnostic rather than enforced.

Startup:

- uses the homepage from `webby_state`
- uses `BrowserConfig.javascript_enabled` to decide whether inline scripts run
- uses `BrowserConfig.animations_enabled` to decide whether visual transitions
  run
- defaults to `examples/simple.html`
- resolves through `webby_url`, loads through `webby_net`, and runs the normal
  parse/style/layout/display-list/render pipeline
- new tabs start with the configured homepage address
- `AppState::open_bookmarks_page` renders a simple generated bookmarks page
  from profile bookmarks without adding parser/layout/render logic to app code
- cookie handling is coordinated around `webby_net` loaders; `webby_app` does
  not parse resources or move cookie storage into engine crates
- normal navigation may send matching cookies and store `Set-Cookie` response
  headers; disabled cookies neither send nor store cookies
- `clear_cookies_on_exit` clears the profile cookie jar when the native app
  exits, including the non-interactive smoke path

Native shortcuts:

- `Ctrl`/`Command` + `T`: new tab
- `Ctrl`/`Command` + `W`: close active tab
- `Ctrl`/`Command` + `Tab`: next tab
- `Ctrl`/`Command` + `Shift` + `Tab`: previous tab
- `Alt`/`Ctrl`/`Command` + Left/Right: back/forward
- `Ctrl`/`Command` + `R`: reload
- `F12`: debug overlay

Form support covers GET query submissions and URL-encoded POST submissions.
Unsupported methods become deterministic app errors.
Controls outside a containing form can focus but do not implicitly submit.
Checkboxes, radio groups, selects, text/password/email/search inputs,
textareas, submit buttons, reset buttons, and inert `button` controls are
handled by app state; layout/render only expose geometry and drawing data.

Debug tools:

- F12 toggles a debug overlay in the native app.
- The overlay draws layout margin, border, padding, and content rectangles from
  the existing `LayoutTree`; it does not rerun layout or change page geometry.
- Clicking page content while the overlay is enabled selects the deepest layout
  box at that page coordinate and shows tag/id/class, style, dimensions, paint
  summary, and available link/form/image metadata.
- `examples/debug.html` is the demo fixture for the inspection pipeline.

## `webby_cli`

Debug and validation command-line composition.

Commands:

```bash
webby_cli --resolve-input "example.com"
webby_cli --fetch "https://example.com"
webby_cli --dump-dom examples/simple.html
webby_cli --dump-text examples/simple.html
webby_cli --dump-css examples/debug.html
webby_cli --dump-style examples/simple.html
webby_cli --dump-layout examples/simple.html --viewport-width 800
webby_cli --dump-display-list examples/simple.html --viewport-width 800
webby_cli --dump-diagnostics examples/debug.html --viewport-width 800
webby_cli --render-ppm examples/simple.html --viewport-width 800 --output snapshot.ppm
webby_cli --show-config
webby_cli --list-history
webby_cli --list-bookmarks
webby_cli --add-bookmark https://example.com/
webby_cli --remove-bookmark https://example.com/
webby_cli --clear-cookies
```

CLI commands compose crate APIs. They do not own core algorithms. Stylesheet
and cache diagnostics are surfaced deterministically; PPM output remains
binary-clean.

## `webby_state`

Owns persistent browser profile state.

Default profile directory resolution:

1. `WEBBY_PROFILE_DIR`
2. `$XDG_DATA_HOME/webby`
3. `$HOME/.local/share/webby`

Files:

```text
config.json
history.json
bookmarks.json
recent.json
cookies.json
local_storage.json
```

The format is deterministic pretty JSON with stable struct field order and a
trailing newline. Missing files use defaults. Corrupt files return structured
parse errors and do not silently reset state.

Default config:

```json
{
  "homepage": "examples/simple.html",
  "cookies_enabled": true,
  "persist_cookies": false,
  "clear_cookies_on_exit": false,
  "javascript_enabled": true,
  "storage_enabled": true,
  "animations_enabled": true
}
```

History stores successful navigation URLs in commit order. Failed navigations
are not recorded as successful history. Recent pages are most-recent-first and
de-duplicated. Bookmarks are sorted by URL, and adding a duplicate bookmark is a
deterministic no-op.

Cookie behavior:

- `CookieJar` parses a small deterministic `Set-Cookie` subset: `name=value`,
  `Domain`, `Path`, `Secure`, `Max-Age`, and `Expires`.
- host-only cookies only match the exact response host; domain cookies match the
  domain and subdomains when the response host is allowed to set that domain.
- path matching is segment-aware: `/foo` matches `/foo` and `/foo/bar`, but not
  `/foobar`; request cookie headers are sorted by longest path first, then name.
- `Max-Age=0` or negative values delete a cookie with the same name/domain/path.
- session cookies stay in memory only; persistent cookies are written to
  `cookies.json` only when `persist_cookies` is true.
- disabling persistence removes any stale `cookies.json` on save.
- `--clear-cookies` clears in-memory profile cookies and removes
  `cookies.json`.
- invalid `Set-Cookie` diagnostics are redacted and do not include raw cookie
  names or values.

Web Storage behavior:

- `local_storage.json` stores persistent `localStorage` in deterministic pretty
  JSON, keyed by Webby's origin key.
- HTTP(S) origin keys are `scheme://host:port`, using known default ports when
  the URL omits one. These keys come from `webby_security`.
- `file://` pages share the documented `file://local` origin key.
- Storage values are strings. The per-origin quota is
  `webby_state::STORAGE_QUOTA_BYTES` bytes, counted as UTF-8 key plus value
  bytes.
- `sessionStorage` is non-persistent and isolated per tab.
- corrupt `local_storage.json` returns a structured parse error.
