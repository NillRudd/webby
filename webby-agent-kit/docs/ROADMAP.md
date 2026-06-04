# Webby Roadmap

This roadmap is the controlling implementation plan. Codex should implement milestones in order unless the user explicitly changes the priority.

## Milestone 0: repository foundation

Goal: create a serious Rust workspace with validation, documentation, fixtures, and module boundaries.

### Tasks

- Create workspace `Cargo.toml`.
- Add crates listed in `docs/ARCHITECTURE.md`.
- Add shared error type in `webby_core`.
- Add example HTML files.
- Add `webby_cli` with placeholder commands that return useful errors instead of panicking.
- Add CI-equivalent validation commands to documentation.

### Definition of done

- `cargo check --workspace` passes.
- `cargo fmt --all -- --check` passes.
- `cargo test --workspace` passes.
- Every crate has a clear `lib.rs` or `main.rs` and module-level docs.

## Milestone 1: URL and search resolver

Goal: the address bar semantics work before browser UI exists.

### Tasks

- Implement `webby_url::resolve_input(input, search_engine)`.
- Support direct URLs, bare domains, localhost, file paths, and search queries.
- Percent-encode search queries correctly.
- Add tests for edge cases.
- Add CLI command: `webby_cli --resolve-input "query"`.

### Required behavior

```text
https://example.com        -> https://example.com/
http://example.com         -> http://example.com/
example.com                -> https://example.com/
localhost:8080             -> http://localhost:8080/
rust browser engine        -> https://www.google.com/search?q=rust+browser+engine
examples/simple.html       -> file://.../examples/simple.html
```

### Definition of done

- Resolver has no hardcoded special cases for tests except configurable search engine defaults.
- Invalid input returns structured errors.
- Unit tests cover URLs, domains, search strings, file paths, whitespace, and unicode.

## Milestone 2: network and file loading

Goal: Webby can load bytes from `http`, `https`, and `file` URLs.

### Tasks

- Implement `webby_net::load_resource(url)`.
- Support redirects using the HTTP client library.
- Return metadata: final URL, status code, content type, byte length.
- Support `file://` URLs.
- Decode text using UTF-8 first; gracefully handle invalid UTF-8.
- Add CLI command: `webby_cli --fetch https://example.com`.

### Definition of done

- Network code is isolated from parser/rendering code.
- Tests use local fixtures where possible.
- HTTP failures produce useful errors.
- CLI can print fetched response metadata.

## Milestone 3: HTML tokenizer and DOM parser

Goal: parse basic HTML into Webby's own DOM.

### Supported v0.1 tags

- `html`
- `head`
- `title`
- `body`
- `h1` through `h6`
- `p`
- `a`
- `div`
- `span`
- `br`
- `img` as metadata only, rendering later
- unknown tags as transparent containers

### Must ignore visible output from

- `script`
- `style`
- `noscript` initially optional
- comments
- doctype

### Tasks

- Implement tokenizer.
- Implement tree builder for simple nested HTML.
- Implement basic attribute parsing.
- Implement visible text extraction.
- Add CLI commands:
    - `webby_cli --dump-dom examples/simple.html`
    - `webby_cli --dump-text examples/simple.html`

### Definition of done

- Parser handles malformed but common HTML without panicking.
- Unknown tags do not destroy their visible child text.
- Script/style content is not shown as page text.
- Tests cover nested tags, links, attributes, comments, malformed HTML, and text whitespace normalization.

## Milestone 4: default style system

Goal: render pages without full CSS by applying user-agent-style defaults.

### Tasks

- Create default styles for body, headings, paragraphs, links, divs, spans, and line breaks.
- Represent computed style explicitly.
- Do not parse external CSS yet unless Milestone 4 is complete.
- Add style tree generation from DOM.

### Definition of done

- Headings are larger than normal text.
- Paragraphs have vertical spacing.
- Links have link metadata and distinct rendering style.
- Style tree can be dumped for debugging.

## Milestone 5: block/text layout

Goal: convert styled content into positioned layout boxes.

### Tasks

- Implement viewport dimensions.
- Implement block flow: vertical stacking.
- Implement word wrapping.
- Implement basic line boxes.
- Preserve clickable link rectangles.
- Add CLI command: `webby_cli --dump-layout examples/simple.html`.

### Definition of done

- Text wraps by available width.
- Headings, paragraphs, links, and `br` affect layout correctly.
- Layout output is deterministic.
- Snapshot tests or structured assertions cover layout results.

## Milestone 6: display list and software renderer

Goal: render the layout tree to pixels using Webby's own display list.

### Tasks

- Define display commands: background, text, line, border, image placeholder.
- Implement display list builder.
- Implement software renderer or `pixels`/`tiny-skia`-backed renderer.
- Add rendering tests for simple fixtures.

### Definition of done

- Renderer draws text and page background.
- Renderer draws links distinctly.
- Renderer does not know HTML or DOM details directly.
- Render tests compare stable display lists or image snapshots.

## Milestone 7: browser shell UI

Goal: native app with address/search bar and page viewport.

### Tasks

- Create `webby_app` native window.
- Draw top chrome: address/search bar.
- Handle keyboard input.
- Pressing Enter resolves input and loads page.
- Render loaded document below toolbar.
- Support scroll wheel.

### Definition of done

- User can type `example.com` and press Enter.
- Page content loads and displays below the bar.
- Failures show readable error page, not terminal-only panic.
- App remains responsive during normal loads or clearly indicates loading state.

## Milestone 8: navigation and links

Goal: pages become interactive enough to browse simple sites.

### Tasks

- Track current URL.
- Resolve relative links.
- Hit-test link rectangles.
- Click links to navigate.
- Add back/forward history if link navigation is stable.

### Definition of done

- Links on `example.com` or local fixtures can be clicked.
- Relative URLs resolve against the current page.
- Navigation state is not hardcoded to one site.
- Browser handles failed navigation gracefully.

## Milestone 9: minimal CSS parser

Goal: add real CSS after the default-style browser works.

### Supported selectors

- tag selectors: `p`
- class selectors: `.card`
- id selectors: `#main`
- simple descendant selectors optional only after basic selectors work

### Supported properties

- `color`
- `background-color`
- `font-size`
- `margin`
- `padding`
- `border-width`
- `border-color`
- `width`
- `height`

### Definition of done

- CSS parser is tested independently.
- Style resolver handles specificity deterministically.
- Unsupported declarations are ignored with diagnostics, not fatal errors.

## Milestone 10: real text rendering and text metrics

Goal: replace placeholder text drawing with real deterministic text measurement and glyph rendering.

### Tasks

- Add a shared text/font module.
- Use one authoritative text measurement API.
- Make layout and rendering use the same text metrics.
- Render actual glyph pixels into the software surface.
- Support font size, text color, bold approximation, and underline path.
- Make app chrome/address/status text use the shared renderer.
- Document font fallback behavior.

### Definition of done

- Text measurement is centralized.
- Layout wrapping uses shared text metrics.
- Renderer draws real text pixels.
- App chrome does not duplicate text rasterization logic.
- Output remains deterministic where possible.

## Milestone 11: image loading and rendering

Goal: render real images instead of placeholder boxes.

### Tasks

- Preserve `img src`, `alt`, `width`, and `height` attributes from HTML.
- Resolve image URLs relative to the current page URL.
- Load image resources through `webby_net`.
- Add image decoding using a justified dependency such as `image`.
- Support PNG and JPEG initially.
- Add decoded image metadata to the page pipeline before layout.
- Use intrinsic image dimensions during layout.
- Respect valid `width` and `height` attributes.
- Respect CSS `width` and `height` when present.
- Render decoded image pixels into the software surface.
- Keep deterministic placeholder rendering for failed image loads.

### Definition of done

- Local image fixtures render correctly.
- Remote image URLs can be loaded through the same resource-loading path.
- Invalid or missing images do not panic.
- Layout uses decoded image dimensions when available.
- Image rendering clips safely.
- `webby_layout` does not fetch or decode images.
- `webby_render` does not fetch network resources.

---

## Milestone 12: external stylesheets

Goal: support `<link rel="stylesheet" href="...">`.

### Tasks

- Parse and preserve stylesheet link metadata from the DOM.
- Resolve stylesheet URLs relative to the current page URL.
- Load external CSS through `webby_net`.
- Feed loaded CSS into `webby_css` and `webby_style`.
- Preserve ordering:
    - default styles
    - external stylesheets in document order
    - `<style>` blocks in document order
    - inline styles highest
- Handle failed stylesheet loads gracefully.
- Add diagnostics for stylesheet failures without failing the whole page.

### Definition of done

- Local external stylesheet fixtures affect rendering.
- Relative stylesheet URLs resolve correctly for `file://` and `http(s)://`.
- Failed stylesheet loads do not crash page loading.
- Cascade order is deterministic and tested.
- CSS loading logic stays out of `webby_layout` and `webby_render`.

---

## Milestone 13: improved CSS selector support

Goal: make simple real-world CSS work better without implementing the full CSS spec.

### Supported new selectors

- descendant selectors: `.card p`
- child selectors: `.card > p`
- grouped selectors: `h1, h2, h3`
- universal selector: `*`
- multiple classes: `.card.highlighted`
- simple attribute selectors optional:
    - `[href]`
    - `[type="text"]`

### Tasks

- Extend `webby_css` selector AST.
- Implement selector matching in `webby_style`.
- Add specificity calculations for new selectors.
- Add deterministic behavior for unsupported selectors.
- Add tests for cascade ordering and specificity.

### Definition of done

- Selector parsing is independent from style application.
- Descendant and child selectors work on nested DOM fixtures.
- Grouped selectors apply correctly.
- Unsupported selectors are ignored with diagnostics.
- No CSS matching logic leaks into layout/render/app crates.

---

## Milestone 14: inline layout improvements

Goal: make mixed text, links, spans, images, and line breaks behave more like a browser.

### Tasks

- Improve inline formatting inside block containers.
- Support mixed inline content order:
    - text
    - `span`
    - `a`
    - `img`
    - `br`
- Preserve correct link hit regions across wrapped lines.
- Improve baseline and line-height handling.
- Avoid gaps or incorrect wrapping around inline images.
- Add tests for nested inline elements.
- Add layout dumps that make inline fragments debuggable.

### Definition of done

- Mixed inline content renders in source order.
- Multi-line links remain clickable across all visible link regions.
- Inline images align predictably with text.
- `br` works inside paragraphs/headings/links/spans.
- Layout remains deterministic for tiny viewport widths.
- No renderer-specific hacks are added to the layout engine.

---

## Milestone 15: forms v0.1

Goal: support basic form controls visually and structurally.

### Supported elements

- `form`
- `input type="text"`
- `input type="search"`
- `input type="submit"`
- `button`
- `label`
- `textarea` optional

### Tasks

- Parse form-related elements and attributes.
- Add default styles for form controls.
- Add layout boxes for input/button elements.
- Render form controls in the software renderer.
- Add app state for focused form control.
- Support typing into simple text/search inputs.
- Support submit behavior for `GET` forms.
- Resolve form action relative to current URL.
- Build query strings correctly.

### Definition of done

- A local search form fixture works.
- User can type into an input field.
- Submit button or Enter in input triggers navigation.
- GET query serialization is tested.
- Invalid form data does not panic.
- JavaScript is not required.

---

## Milestone 16: browser state and persistence

Goal: make Webby feel usable across sessions.

### Tasks

- Add bookmarks.
- Add persistent history.
- Add recent pages.
- Add configurable homepage/start page.
- Add simple config file.
- Add CLI commands for inspecting config/history/bookmarks if useful.
- Store state in a deterministic, documented format such as JSON/TOML.

### Definition of done

- History persists across app restarts.
- Bookmarks can be added and opened.
- Config loading handles missing/corrupt files gracefully.
- User data location is documented.
- Persistence logic is isolated from rendering/layout/parser crates.

---

## Milestone 17: caching

Goal: avoid reloading every resource every time.

### Tasks

- Add resource cache abstraction.
- Cache page resources, stylesheets, and images.
- Use requested URL strings as cache keys and store response metadata.
- Respect basic cache invalidation rules initially:
    - normal address, link, and form navigation may use cached resources
    - manual reload refreshes the cache on successful reload
    - back/forward traversal currently refreshes resources to preserve tested
      failure semantics
    - failed resource loads do not poison future loads
- Add deterministic count and byte-size limits.
- Use deterministic least-recently-used eviction.
- Add cache diagnostics.
- Add tests for repeated loads.

### Definition of done

- Repeated local/HTTP resource loads can use the cache.
- Reload semantics are explicit and tested.
- Cache failures do not crash navigation.
- Cache code stays out of parser/layout/render crates.
- Cache behavior is documented.

---

## Milestone 18: tabs

Goal: support multiple independent browsing contexts.

### Tasks

- Add tab model to `webby_app`.
- Each tab owns:
    - current URL
    - address bar/input state
    - history
    - history index
    - scroll offset
    - page status
    - current rendered page
    - pending/failed URL state
    - form focus/edit state
- Add tab switching.
- Add new tab and close tab.
- Add basic keyboard shortcuts.
- Keep rendering only the active tab initially.
- Preserve existing single-tab behavior through tests.
- Keep bookmarks/history/config persistence app-level.

### Definition of done

- Multiple tabs can be opened and switched.
- Each tab preserves its own history and scroll state.
- Closing a tab does not corrupt other tabs.
- Closing the last tab is rejected.
- Navigation in one tab does not affect another tab.
- Native tab shortcuts delegate to `AppState`.
- App state remains testable without the native window adapter.

---

## Milestone 19: developer/debug tools

Goal: make Webby introspectable and impressive to demo.

### Tasks

- Add debug overlay toggle.
- Show box model rectangles:
    - margin
    - border
    - padding
    - content
- Add click-to-inspect element/layout box.
- Show selected node information:
    - tag
    - id/classes
    - computed styles
    - layout dimensions
    - display commands
- Add CLI commands for complete pipeline dumps:
    - DOM
    - CSS rules
    - style tree
    - layout tree
    - display list
- Add debug docs and demo fixtures.

### Definition of done

- User can visually inspect layout boxes in the app.
- Debug overlay does not affect normal layout.
- CLI and app debug output agree.
- Inspector data comes from existing pipeline structures, not duplicated parser/render logic.
- Demo page clearly shows the engine pipeline.

---

## Milestone 20: fixture-based integration test suite

Goal: create serious reusable fixtures for real browser-engine behavior.

The `tests/fixtures` directory should become the source of truth for cross-crate integration tests, rendering tests, layout tests, and regression cases.

### Tasks
- Populate `tests/fixtures/` with structured fixture groups:
    - `html/`
    - `css/`
    - `layout/`
    - `render/`
    - `forms/`
    - `images/`
    - `navigation/`
    - `sites/`
- Add fixture manifest format if useful, such as `fixture.toml`.
- Add local mini-sites with multiple linked pages, stylesheets, images, and forms.
- Add expected outputs where appropriate:
    - expected visible text
    - expected DOM dump
    - expected style dump
    - expected layout dump
    - expected display-list dump
    - expected rendered PPM/hash
- Add integration tests that run complete pipeline fixtures:
    - file URL
    - HTML parse
    - stylesheet loading
    - image loading
    - layout
    - display list
    - render output
- Add regression fixtures for previous bugs:
    - raw text close lookalikes
    - CSS malformed selectors
    - external stylesheet diagnostics
    - image fallback
    - form GET serialization
    - tab state isolation

### Definition of done

- `tests/fixtures` is no longer empty.
- Fixtures are documented and easy to add.
- New examples added to `examples`
- At least one complete local test site exists.
- Pipeline tests use fixtures instead of only inline strings.
- Regression cases are preserved as files.
- Test outputs are deterministic.

--

## Milestone 21: CSS box model and sizing completeness

Goal: make block layout sizing more browser-like and less hardcoded.

### Tasks

- Improve CSS box model handling:
    - margin
    - padding
    - border
    - width
    - height
    - min-width optional
    - max-width optional
    - min-height optional
    - max-height optional
- Support shorthand expansion more completely:
    - `margin: 1px`
    - `margin: 1px 2px`
    - `margin: 1px 2px 3px`
    - `margin: 1px 2px 3px 4px`
    - same for padding
    - simple border shorthand
- Add `box-sizing: content-box` and `border-box`.
- Implement `auto` width behavior for block elements.
- Improve percentage widths if feasible.
- Improve body/html viewport sizing.
- Add overflow clipping behavior only if simple and scoped.

### Definition of done

- Common box model examples render correctly.
- Width calculations are not fixture-specific.
- CSS shorthand behavior is tested.
- `box-sizing` is tested.
- Layout dump clearly shows content/padding/border/margin boxes.
- Tiny viewport behavior remains safe.

--

## Milestone 22: CSS display and visibility model

Goal: support common display behavior used by real static websites.

### Tasks

- Add support for:
    - `display: block`
    - `display: inline`
    - `display: inline-block`
    - `display: none`
- Add support for:
    - `visibility: visible`
    - `visibility: hidden`
- Ensure `display: none` removes content from layout/render/hit testing.
- Ensure `visibility: hidden` preserves layout but does not paint or interact.
- Add default display values for more HTML elements.
- Add tests for nested hidden content.
- Add tests for hidden links/forms/images.

### Definition of done

- Hidden elements do not render or interact incorrectly.
- `display: none` and `visibility: hidden` behave differently.
- Inline-block elements participate in inline layout predictably.
- App link/form hit testing respects display/visibility.

--

## Milestone 23: CSS positioning v0.1

Goal: support basic positioned layout used by many real sites.

### Tasks

- Add support for:
    - `position: static`
    - `position: relative`
    - `position: absolute`
- Add support for offsets:
    - `top`
    - `right`
    - `bottom`
    - `left`
- Add basic containing-block logic.
- Add `z-index` support for positioned elements if feasible.
- Preserve paint order deterministically.
- Ensure hit testing uses final positioned rectangles.
- Add layout dump information for positioned boxes.

### Definition of done

- Relative positioning moves painting/hit regions without corrupting normal flow.
- Absolute positioning removes the element from normal block flow.
- Positioned links/forms remain clickable.
- Paint order is deterministic and tested.
- Layout/render remain separated.

--

## Milestone 24: flexbox v0.1

Goal: support the most useful subset of modern layout.

### Supported subset

- `display: flex`
- `flex-direction: row`
- `flex-direction: column`
- `gap`
- `justify-content`:
    - `flex-start`
    - `center`
    - `space-between`
- `align-items`:
    - `stretch`
    - `center`
    - `flex-start`
- basic `flex: 1`
- basic `flex-grow`

### Tasks

- Add flex container layout mode in `webby_layout`.
- Keep flex algorithm scoped and deterministic.
- Add tests for common navbars/cards/forms.
- Add debug dump output for flex containers/items.
- Ensure rendering/hit testing follows flex-computed geometry.

### Definition of done

- Simple navbar fixture works.
- Card row/column fixture works.
- Flex gap works.
- Flex children preserve source order.
- Layout remains safe for tiny widths.
- Non-flex layout does not regress.

--

## Milestone 25: tables v0.1

Goal: support simple tables used in documentation and older websites.

### Supported elements

- `table`
- `thead`
- `tbody`
- `tr`
- `th`
- `td`

### Tasks

- Add default styles for table elements.
- Parse and preserve table structure through DOM.
- Implement basic table layout:
    - rows
    - columns
    - cell padding
    - simple borders
- Ignore complex features initially:
    - colspan
    - rowspan
    - border-collapse advanced behavior
- Add render and layout tests.

### Definition of done

- Simple documentation-style tables render.
- Header cells and body cells are visually distinct.
- Column widths are deterministic.
- Large/wide tables do not panic.

--

## Milestone 26: richer HTML element support

Goal: make common static websites less broken.

### Supported additions

- `main`
- `section`
- `article`
- `header`
- `footer`
- `nav`
- `aside`
- `ul`
- `ol`
- `li`
- `strong`
- `em`
- `small`
- `code`
- `pre`
- `blockquote`
- `hr`
- `figure`
- `figcaption`

### Tasks

- Add parser/default-style support where needed.
- Add layout/render behavior for lists.
- Add monospace/preformatted text behavior for `pre` and `code`.
- Add simple horizontal rule rendering.
- Add blockquote styling.
- Add semantic elements as block defaults.
- Add fixture pages using these elements.

### Definition of done

- Blog/article fixture renders acceptably.
- Documentation fixture renders acceptably.
- Lists render with bullets/numbers or deterministic fallback markers.
- Preformatted text preserves whitespace.
- Existing inline layout remains stable.

--

## Milestone 27: font and text improvements

Goal: make text rendering closer to real websites.

### Tasks

- Add CSS support for:
    - `font-family`
    - `line-height`
    - `letter-spacing` optional
    - `text-align`
    - `white-space`
- Improve bold/italic handling if feasible.
- Support monospace fallback for `code` and `pre`.
- Improve text wrapping for:
    - long words
    - punctuation
    - preformatted text
- Add support for:
    - `white-space: normal`
    - `white-space: pre`
    - `white-space: nowrap` optional
- Add tests for line-height and text alignment.

### Definition of done

- Text layout and rendering use consistent metrics.
- `pre` blocks preserve whitespace.
- `text-align: center/right` works in simple blocks.
- Long text does not overflow catastrophically unless documented.
- Font choices are documented and deterministic where possible.

--

## Milestone 28: browser UI polish

Goal: make Webby feel like an actual usable browser.

### Tasks

- Add clickable tab strip.
- Add close button per tab.
- Add back/forward/reload buttons.
- Add bookmark button.
- Add visible loading/error indicators.
- Add hover/cursor feedback for links and controls.
- Improve address bar selection/edit behavior.
- Add homepage/new-tab page.
- Add simple bookmarks page.
- Add keyboard shortcuts documentation.

### Definition of done

- Browser can be used mostly with mouse.
- Tabs can be selected and closed visually.
- Navigation buttons work.
- Address bar editing is less primitive.
- Error/loading states are visible and understandable.
- UI polish stays in `webby_app`.

--

## Milestone 29: storage, cookies, and sessions v0.1

Goal: support basic stateful browsing without JavaScript.

### Tasks

- Add cookie jar abstraction.
- Parse `Set-Cookie` headers for a small safe subset.
- Send `Cookie` headers on matching requests.
- Persist cookies if enabled by config.
- Add session-only cookies.
- Add clear-cookies command.
- Add simple local storage abstraction only if needed later; do not expose JS APIs yet.
- Add privacy config:
    - enable/disable cookies
    - clear on exit optional
- Add tests for domain/path matching.

### Definition of done

- Basic cookie-based sites can keep simple sessions.
- Cookie behavior is deterministic and documented.
- Invalid cookies do not panic.
- Cookie storage is isolated from layout/render/parser.
- Privacy controls are documented.

--


## Milestone 30: quality hardening and release demo

Goal: turn Webby into a serious portfolio/thesis-level project.

### Tasks

- Add fuzz-like parser tests using generated malformed HTML/CSS.
- Add integration tests from URL input through rendered output.
- Add large-document performance tests.
- Add memory/allocation notes for layout/rendering.
- Add architecture diagrams.
- Add manual demo script.
- Add screenshots/PPM demo outputs.
- Add known limitations document.
- Add release checklist.
- Add project README polish.

### Definition of done

- Full validation loop passes.
- Demo script works on a clean checkout.
- Architecture docs match actual implementation.
- Known limitations are honest and specific.
- No important TODOs are left without issue references or follow-up docs.
- Project can be cloned, built, tested, and demoed by another developer.

--

## Milestone 31: JavaScript engine integration

Goal: add a real JavaScript execution layer without pretending Webby is a full browser yet.

### Tasks

- Add a JavaScript engine integration using a justified dependency.
    - Prefer a small embeddable engine such as `boa_engine` or `rquickjs`.
    - Do not use Chromium, V8, WebKit, Blink, Electron, or WebView.
- Add a dedicated crate if justified, such as `webby_js`.
- Parse and collect `<script>` elements from HTML.
- Execute inline scripts.
- Support external scripts only if architecture remains clean.
- Add a minimal global environment:
    - `console.log`
    - `window` placeholder
    - `document` placeholder
- Collect console output as diagnostics.
- Convert script syntax/runtime errors into diagnostics, not page crashes.
- Add execution limits if the chosen JavaScript engine supports them.
- Add config support for enabling/disabling JavaScript if appropriate.

### Definition of done

- Inline scripts execute.
- `console.log` output is captured deterministically.
- Script errors become diagnostics.
- Unsupported browser APIs fail gracefully.
- Page rendering continues if a script fails.
- JavaScript execution is isolated in `webby_js`.
- Layout, style, render, and network crates do not execute JavaScript.

---

## Milestone 32: DOM mutation APIs

Goal: allow JavaScript to read and mutate Webby’s DOM through a small controlled API.

### Tasks

- Add stable DOM node IDs if not already present.
- Expose minimal `document` APIs:
    - `document.getElementById`
    - `document.querySelector`
    - `document.createElement`
    - `document.createTextNode`
- Expose minimal node/element APIs:
    - `textContent` get/set
    - `innerText` get/set if feasible
    - `appendChild`
    - `remove`
    - `setAttribute`
    - `getAttribute`
    - `className` get/set
    - `id` get/set
- Mark style/layout/render dirty after DOM mutations.
- Re-run the normal pipeline after script mutations.
- Keep mutation order deterministic.
- Return controlled JavaScript errors for unsupported APIs.

### Definition of done

- JavaScript can change text content and rendered output changes.
- JavaScript can append/remove elements.
- `setAttribute` can change class/id and trigger CSS rematching.
- `getElementById` works.
- `querySelector` works for the supported selector subset.
- Mutations flow through the normal style/layout/render pipeline.
- JavaScript does not directly render pixels or patch buffers.
- DOM mutation logic belongs to `webby_dom`; JS bindings belong to `webby_js`.

---

## Milestone 33: event system v0.1

Goal: support basic DOM-style events for user interaction.

### Tasks

- Add an event target model for DOM nodes.
- Support:
    - `addEventListener("click", ...)`
    - `addEventListener("input", ...)`
    - `addEventListener("submit", ...)`
    - `addEventListener("keydown", ...)` optional
- Support `onclick` property if feasible.
- Dispatch click events from app hit testing to DOM/JS.
- Dispatch input events when form input values change.
- Dispatch submit events when forms submit.
- Add a minimal event object:
    - `type`
    - `target`
    - `currentTarget`
    - `preventDefault()`
    - `defaultPrevented`
- Implement simple bubbling:
    - target
    - ancestors
    - document/window optional
- Support default actions:
    - link navigation
    - form submission
- Support `preventDefault` for links and forms.
- Convert event handler errors into diagnostics.

### Definition of done

- Click handlers run.
- Click events bubble in deterministic order.
- `preventDefault` stops link navigation.
- `preventDefault` stops form submission.
- Input events fire when input values change.
- Submit events fire on form submission.
- Event handlers can mutate DOM and trigger re-render.
- Hidden or `display: none` elements do not receive click events.
- Layout/render do not own event behavior.

---

## Milestone 34: timers, location, and history JavaScript APIs

Goal: add the first useful browser APIs beyond DOM mutation.

### Tasks

- Add `window.location` support:
    - `href` get
    - `href` set triggers navigation
    - `assign(url)`
    - `replace(url)` optional
- Add `history` subset:
    - `history.back()`
    - `history.forward()`
    - `history.pushState()` optional
- Add timers:
    - `setTimeout`
    - `clearTimeout`
    - `setInterval` optional
    - `clearInterval` optional
- Add deterministic timer scheduling in app state.
- Ensure old timers cannot mutate a new page after navigation.
- Make navigation from JavaScript use the existing navigation/history model.
- Convert API errors into diagnostics.

### Definition of done

- `window.location.href` returns the current URL.
- Setting `location.href` navigates.
- `location.assign` navigates.
- `history.back` and `history.forward` work from JavaScript.
- `setTimeout` callbacks run in deterministic tests.
- `clearTimeout` cancels callbacks.
- Timer callbacks can mutate DOM and re-render.
- Navigation cancels old page timers.
- JavaScript navigation does not corrupt history semantics.

---

## Milestone 35: fetch and XMLHttpRequest v0.1

Goal: support basic JavaScript network requests.

### Tasks

- Implement `fetch(url)` with:
    - `GET` only initially
    - `text()` response body
    - `status`
    - `ok`
    - `url`
- Add Promise integration if the chosen JS engine supports it cleanly.
- If Promise integration is too large, document the limitation and provide the smallest testable alternative.
- Add minimal `XMLHttpRequest` only after `fetch` is stable:
    - `open("GET", url)`
    - `send()`
    - `onload`
    - `status`
    - `responseText`
- Resolve relative request URLs against the current page URL.
- Use `webby_net` for actual loading.
- Define security behavior:
    - same-origin by default, or
    - explicitly documented permissive mode
- Document cache behavior for fetch/XHR.
- Convert request failures into controlled JavaScript errors or rejected promises.

### Definition of done

- JavaScript can fetch a local same-origin text resource.
- `fetch` exposes status, ok, url, and text body.
- Failed fetches do not panic.
- Relative fetch URLs resolve correctly.
- Cross-origin behavior is explicit and tested.
- JavaScript can fetch text and then mutate the DOM with it.
- Network logic stays in `webby_net`.
- Layout/render/style/html crates do not know about fetch/XHR.

---

## Milestone 36: dynamic style/layout invalidation

Goal: make dynamic DOM and style changes update the rendered page correctly.

### Tasks

- Add explicit dirty states:
    - DOM dirty
    - style dirty
    - layout dirty
    - render dirty
- Text mutations should mark layout/render dirty.
- Class/id/style mutations should mark style/layout/render dirty.
- Link/form attribute mutations should update hit/navigation metadata.
- Form input changes should update rendering.
- Script/event/timer/fetch mutations should flow through the dirty-state model.
- Re-render pages after dynamic mutations.
- Avoid unnecessary resource reloads for pure DOM/style changes.
- Add deterministic diagnostics for invalidation if useful.

### Definition of done

- `textContent` mutation updates rendered output without reloading the page.
- Class mutation rematches CSS.
- Inline style mutation updates style/layout/render.
- `appendChild` and `remove` trigger layout/render.
- `href` mutation updates link target.
- Form attribute mutation updates submission behavior.
- Stale hit regions are not used after mutation.
- JavaScript/event code never directly patches render buffers.
- Recompute uses the normal pipeline.

---

## Milestone 37: localStorage and sessionStorage

Goal: add basic Web Storage APIs.

### Tasks

- Implement `localStorage`:
    - `getItem`
    - `setItem`
    - `removeItem`
    - `clear`
    - `key`
    - `length`
- Implement `sessionStorage` with the same API.
- Persist `localStorage` through `webby_state`.
- Keep `sessionStorage` per-tab or per-page-session and non-persistent.
- Define origin key behavior:
    - scheme + host + port for HTTP(S)
    - documented behavior for `file://`
- Store values as strings.
- Add a deterministic storage quota.
- Return controlled JavaScript errors on quota overflow.
- Handle corrupt persisted storage safely.
- Add config to disable storage if appropriate.

### Definition of done

- `localStorage` set/get/remove/clear works.
- `localStorage` persists across profile reloads.
- `sessionStorage` does not persist across app restart.
- `sessionStorage` is isolated per tab.
- Different origins have isolated storage.
- Quota behavior is deterministic.
- Corrupt storage files are handled safely.
- Storage APIs can drive DOM updates.
- Storage does not leak into layout/render/style/html crates.

---

## Milestone 38: SVG and canvas basics

Goal: support the most useful subset of inline SVG and `<canvas>`.

### SVG supported subset

- `svg`
- `rect`
- `circle`
- `line`
- simple `path` optional
- `text` optional
- `fill`
- `stroke`
- `stroke-width`
- `width`
- `height`
- basic `viewBox`

### Canvas supported subset

- `canvas` element layout/render placeholder
- `getContext("2d")`
- `fillRect`
- `strokeRect`
- `clearRect`
- `fillText` optional
- `fillStyle`
- `strokeStyle`
- `lineWidth`

### Tasks

- Render inline SVG through the display-list/render pipeline.
- Add canvas state as bitmap or command list.
- Expose basic canvas API through JavaScript.
- Redraw canvas after JavaScript drawing commands.
- Size SVG/canvas from attributes and CSS.
- Ensure invalid SVG/canvas operations cannot panic.

### Definition of done

- Inline SVG rectangle renders pixels.
- SVG width/height affect layout.
- Invalid SVG falls back safely.
- Canvas `fillRect` changes rendered pixels.
- Canvas `clearRect` clears pixels.
- Canvas state survives until page reload.
- Canvas JavaScript errors are controlled.
- App pipeline renders SVG/canvas fixtures.
- Layout owns sizing; render owns drawing; JS owns canvas bindings.

---

## Milestone 39: media and asset compatibility basics

Goal: improve compatibility for common static and dynamic site assets without implementing a full media engine.

### Tasks

- Support favicon metadata extraction/loading if simple.
- Add additional image formats only if justified:
    - GIF first frame optional
    - WebP optional
- Support data URLs for:
    - images
    - stylesheets optional
    - scripts optional
- Treat preload/preconnect tags as diagnostics/no-op if not implemented.
- Add basic audio/video placeholders:
    - layout boxes
    - controls placeholder
    - poster image support if feasible
- Add MIME/content-type sniffing for common resources.
- Add graceful fallback for unsupported media.

### Definition of done

- Data URL images render.
- Invalid data URLs fall back safely.
- Favicon metadata is extracted/loaded if implemented.
- Unsupported audio/video render deterministic placeholders.
- Poster images render if implemented.
- Unsupported image formats do not crash rendering.
- MIME sniffing cannot panic on weird bytes.
- App and CLI pipelines handle data URL fixtures.
- No false claim of full audio/video playback.

---

## Milestone 40: dynamic mini-site compatibility hardening

Goal: harden Webby against realistic local mini-sites using all major implemented systems.

### Tasks

- Add at least five local mini-sites under `tests/fixtures/sites`:
    1. static blog
    2. image gallery
    3. form/search page
    4. JavaScript todo app
    5. dynamic fetch/storage app
- Add fixture manifests describing:
    - start URL
    - expected visible text
    - expected diagnostics
    - expected interactions
    - expected render/hash if useful
- Add end-to-end tests for:
    - loading each mini-site
    - clicking links
    - submitting forms
    - JavaScript DOM mutation
    - fetch updates
    - storage persistence
    - back/forward/reload
    - tab isolation where relevant
- Add performance smoke tests:
    - large HTML page
    - many DOM nodes
    - many CSS rules
    - many images
- Add regression tests for bugs found during Milestones 31–39.
- Add a compatibility report:
    - what works
    - what fails
    - what is intentionally unsupported

### Definition of done

- All mini-sites load without panic.
- Expected text and diagnostics match.
- Interactions produce expected state.
- Render outputs are deterministic enough for regression tests.
- Compatibility report is honest and specific.
- No fixture-specific hacks are added.
- If a fixture fails, either the underlying engine is fixed or the limitation is documented.

--

---

## Milestone 41: CSS Grid v0.1

Goal: support enough CSS Grid to render common modern layouts.

### Tasks

- Add CSS parsing support for:
    - `display: grid`
    - `grid-template-columns`
    - `grid-template-rows`
    - `gap`
    - `row-gap`
    - `column-gap`
    - `grid-column`
    - `grid-row`
- Support basic grid track types:
    - fixed `px`
    - percentages
    - `fr`
    - `auto`
- Implement grid layout in `webby_layout`.
- Support simple auto-placement in source order.
- Support explicit placement with `grid-column` and `grid-row`.
- Preserve deterministic layout for tiny viewports.
- Add layout dump information for grid containers and items.
- Ensure grid child hit testing works for links/forms/images.

### Definition of done

- Simple two-column layout fixture works.
- Card grid fixture works.
- Header/sidebar/content/footer grid fixture works.
- `gap` works.
- `fr` units work for simple cases.
- Explicit grid placement works.
- Grid layout does not affect non-grid layout.
- Layout/render/app boundaries remain clean.

---

## Milestone 42: media queries and responsive layout

Goal: support viewport-based responsive styling.

### Tasks

- Add CSS parser support for `@media`.
- Support media features:
    - `min-width`
    - `max-width`
    - `width`
    - `orientation` optional
- Support media types:
    - `screen`
    - `all`
- Ignore unsupported media queries with diagnostics.
- Recompute style/layout/render when viewport size changes.
- Add app resize handling if the native window backend supports it.
- Add CLI support for testing different viewport widths.
- Ensure external stylesheets and style blocks both support media rules.
- Keep cascade order deterministic.

### Definition of done

- Responsive fixture changes layout at breakpoints.
- CLI render with different viewport widths produces different expected layouts.
- App resize triggers re-layout.
- Unsupported media rules do not crash.
- Media query support lives in CSS/style layers, not app/layout hacks.
- Existing CSS behavior does not regress.

---

## Milestone 43: CSS pseudo-classes and interaction styling

Goal: support common interaction-dependent CSS used by real pages.

### Supported pseudo-classes

- `:hover`
- `:focus`
- `:active` optional
- `:checked`
- `:disabled`
- `:first-child`
- `:last-child`
- `:nth-child(n)` optional only if simple

### Tasks

- Extend `webby_css` selector parsing for supported pseudo-classes.
- Add interaction state tracking in `webby_app`.
- Add matching support in `webby_style`.
- Recompute style/layout/render when hover/focus/checked state changes.
- Ensure links, buttons, inputs, and labels can be styled by pseudo-classes.
- Add deterministic diagnostics for unsupported pseudo-classes.

### Definition of done

- Hovered links can change style.
- Focused inputs can change style.
- Disabled controls style correctly.
- Checked controls style correctly if checkbox/radio support exists.
- First/last child selectors work.
- Unsupported pseudo-classes are ignored with diagnostics.
- Interaction state stays in app; selector matching stays in style.

---

## Milestone 44: CSS transitions and animations v0.1

Goal: support simple visual transitions without implementing the whole animation engine.

### Tasks

- Parse simple transition properties:
    - `transition-property`
    - `transition-duration`
    - `transition-delay` optional
    - `transition-timing-function` optional with `linear` and `ease`
    - `transition` shorthand for simple cases
- Animate simple numeric/color properties:
    - color
    - background-color
    - opacity if opacity exists
    - left/top if positioning exists
- Add deterministic animation clock in app state.
- Re-render on animation ticks.
- Allow animations to be disabled in config/test mode.
- Ignore unsupported animations with diagnostics.
- Keyframes are optional and should not be added unless transitions are stable.

### Definition of done

- Hover/focus transition fixture works.
- Color transition works deterministically in tests.
- Animation timing is testable without real wall-clock dependence.
- Unsupported transition syntax does not crash.
- Rendering remains deterministic in test mode.
- App owns animation clock; style/layout/render stay pure.

---

## Milestone 45: script loading model

Goal: make script execution order closer to real websites.

### Tasks

- Support external scripts with `src`.
- Resolve script URLs relative to the current page URL.
- Load scripts through `webby_net` and cache path where appropriate.
- Implement script execution ordering:
    - blocking classic scripts
    - `defer`
    - `async` simplified but documented
- Support `type="text/javascript"` and missing type.
- Ignore unsupported script types with diagnostics.
- Ensure failed script loads become diagnostics, not page crashes.
- Ensure scripts can run before/after DOM construction according to the supported model.
- Preserve deterministic execution order for tests.

### Definition of done

- Inline and external scripts execute in deterministic order.
- Relative external script URLs resolve correctly.
- Failed external script loads produce diagnostics.
- `defer` scripts run after document parse.
- Unsupported script types are ignored safely.
- Existing JavaScript/DOM/event behavior still works.
- Script loading stays out of layout/render.

---

## Milestone 46: JavaScript modules v0.1

Goal: support a small subset of modern module-based pages.

### Tasks

- Support `<script type="module" src="...">`.
- Support inline module scripts if the chosen JS engine allows it.
- Resolve static imports for local/relative modules.
- Implement deterministic module graph loading.
- Detect cycles safely.
- Cache loaded modules within the page/module graph.
- Keep module scope separate from classic script global scope where feasible.
- Ignore unsupported import syntax with diagnostics.
- Do not implement import maps unless explicitly scoped later.

### Definition of done

- Simple module script fixture works.
- Relative imports work.
- Module execution order is deterministic.
- Cyclic imports do not infinite-loop.
- Module errors become diagnostics.
- Classic scripts and module scripts have documented interaction.
- Layout/render remain unaware of modules.

---

## Milestone 47: broader DOM and Web API surface

Goal: support common JavaScript used by normal websites.

### Tasks

- Add DOM APIs:
    - `classList`
    - `dataset`
    - `style`
    - `children`
    - `childNodes`
    - `parentNode`
    - `firstChild`
    - `lastChild`
    - `nextSibling`
    - `previousSibling`
    - `matches`
    - `closest`
    - `querySelectorAll`
- Add element geometry APIs:
    - `getBoundingClientRect`
    - `clientWidth`
    - `clientHeight`
    - `scrollWidth`
    - `scrollHeight`
- Add basic window/document APIs:
    - `document.body`
    - `document.documentElement`
    - `document.title`
    - `window.innerWidth`
    - `window.innerHeight`
- Ensure DOM API mutations use the existing dirty-state pipeline.
- Add diagnostics for unsupported APIs.

### Definition of done

- Common DOM utility code works on fixture pages.
- `classList` mutations affect CSS matching.
- `dataset` reads/writes data attributes.
- `getBoundingClientRect` returns layout-backed values.
- `querySelectorAll` works for supported selectors.
- Unsupported APIs fail gracefully.
- DOM/Web API logic stays out of layout/render.

---

## Milestone 48: forms v1

Goal: support more real-world form controls and submission behavior.

### Supported additions

- `input type="checkbox"`
- `input type="radio"`
- `input type="password"`
- `input type="email"`
- `select`
- `option`
- `textarea`
- `button type="button"`
- `button type="reset"`
- disabled controls
- checked controls

### Tasks

- Parse and preserve added form-control attributes.
- Add default styles and layout for new controls.
- Render controls visibly.
- Add interaction support:
    - checkbox toggle
    - radio group selection
    - select value selection simplified
    - textarea editing
    - reset button
- Improve GET serialization for all successful controls.
- Add POST form submission v0.1:
    - `application/x-www-form-urlencoded`
    - request body construction
    - response navigation
- Add basic validation diagnostics for unsupported form behavior.

### Definition of done

- Checkbox/radio/select/textarea fixtures work.
- Successful controls serialize correctly.
- Disabled controls do not serialize.
- Reset restores initial values.
- POST form fixture works against local test server/helper.
- Form state stays in app; layout/render only expose geometry/drawing.
- JavaScript events integrate with form changes if event system exists.

---

## Milestone 49: origin, CORS, and security model basics

Goal: make network and script behavior less ad hoc.

### Tasks

- Define Webby origin model:
    - scheme
    - host
    - port
    - file origin behavior
- Apply origin checks to:
    - fetch/XHR
    - script loading if needed
    - storage
    - cookies
    - iframes later
- Add CORS support for fetch/XHR:
    - same-origin allowed
    - cross-origin requires permissive headers
    - failed CORS becomes controlled error
- Add mixed-content diagnostics:
    - HTTPS page loading HTTP resource
- Add basic referrer policy behavior if feasible.
- Add CSP diagnostics/no-op initially if full CSP is too large.
- Document security limitations clearly.

### Definition of done

- Same-origin checks are centralized and tested.
- Cross-origin fetch behavior is explicit.
- Storage/cookie origin behavior is consistent.
- CORS failures do not panic.
- Mixed-content diagnostics are deterministic.
- Security model is documented honestly.
- No security checks are hidden inside layout/render.

---

## Milestone 50: iframes and nested browsing contexts

Goal: support basic embedded pages.

### Tasks

- Parse and preserve `iframe` attributes:
    - `src`
    - `width`
    - `height`
    - `name`
    - `sandbox` diagnostics only initially
- Load iframe documents through the normal page pipeline.
- Give each iframe its own browsing context:
    - URL
    - document
    - scroll offset
    - diagnostics
- Render iframe content clipped inside iframe rectangle.
- Support iframe link navigation inside the iframe context.
- Support parent page unaffected by iframe navigation.
- Add origin/security restrictions according to current security model.
- Ignore unsupported iframe features with diagnostics.

### Definition of done

- Local iframe fixture renders.
- Iframe navigation does not replace parent page.
- Iframe content clips correctly.
- Parent and iframe scroll/hit testing are separate.
- Failed iframe load shows placeholder/error inside iframe.
- Engine stages remain reusable; app coordinates browsing contexts.

---

## Milestone 51: Shadow DOM and custom elements v0.1

Goal: support a small subset of Web Components used by modern sites.

### Tasks

- Add `attachShadow({ mode })` support for simple open shadow roots.
- Support shadow tree DOM mutations.
- Support rendering shadow contents instead of/in addition to light DOM according to scoped behavior.
- Add basic custom element registry:
    - `customElements.define`
    - constructor callback
    - connected callback optional
- Support style scoping in shadow roots in a simplified documented way.
- Ignore unsupported lifecycle features with diagnostics.
- Ensure shadow DOM does not crash CSS/layout/render.

### Definition of done

- Simple custom element fixture renders.
- Shadow-root content can be created and displayed.
- Light DOM fallback behavior is documented.
- Shadow DOM styling behavior is deterministic.
- Unsupported custom element features produce diagnostics.
- No shadow-specific hacks enter render/layout beyond needed tree representation.

---

## Milestone 52: accessibility and keyboard interaction

Goal: make basic browsing possible without only using the mouse.

### Tasks

- Add focus traversal:
    - Tab
    - Shift+Tab
- Add keyboard activation:
    - Enter on links/buttons
    - Space on buttons/checkboxes
- Add accessible name calculation for:
    - labels
    - alt text
    - button text
    - aria-label optional
- Add basic role/state metadata:
    - link
    - button
    - textbox
    - checkbox
    - radio
- Add focus ring rendering.
- Add keyboard scrolling.
- Add tests for keyboard navigation.

### Definition of done

- User can navigate links/forms with keyboard.
- Focus order is deterministic.
- Buttons/links activate from keyboard.
- Inputs remain editable from keyboard.
- Focus styling works.
- Accessibility metadata is inspectable/debuggable.
- Layout/render do not own interaction behavior.

---

## Milestone 53: networking robustness

Goal: make real website loading more reliable.

### Tasks

- Improve redirect handling:
    - maximum redirect count
    - redirect loop detection
    - method preservation rules if POST exists
- Add timeout handling.
- Add compressed response support:
    - gzip
    - brotli optional
    - deflate optional
- Improve content-type and charset handling.
- Add HTML charset detection:
    - HTTP header
    - `<meta charset>`
- Add better URL normalization.
- Add download/resource size limits.
- Add network diagnostics with URL/status/error context.
- Add tests using local HTTP server fixtures.

### Definition of done

- Redirect loops fail gracefully.
- Timeouts produce structured errors.
- Gzip responses load.
- Non-UTF-8 text pages decode according to documented behavior.
- Large responses are bounded.
- Network diagnostics are useful.
- Parser/layout/render remain network-agnostic.

---

## Milestone 54: persistent disk cache

Goal: make cache survive app restarts.

### Tasks

- Add disk-backed cache storage.
- Preserve existing in-memory cache behavior as fast path if useful.
- Store:
    - URL key
    - response metadata
    - body bytes
    - stored time/sequence
- Add deterministic cache index format.
- Add eviction by byte size/count.
- Handle corrupt cache entries safely.
- Add clear-cache CLI command.
- Add config for enabling/disabling disk cache.
- Integrate with page, stylesheet, image, script, and fetch loading.

### Definition of done

- Resource cache persists across app restarts.
- Corrupt cache entries are ignored or removed safely.
- Cache eviction is deterministic.
- Clear-cache command works.
- Disk cache does not affect parser/layout/render crates.
- Cache behavior is documented.

---

## Milestone 55: real-world reduced website corpus

Goal: test against reduced versions of real website patterns.

### Tasks

- Add reduced local fixtures modeled after:
    - documentation site
    - news article page
    - blog with responsive layout
    - search page
    - dashboard-like app
    - ecommerce product grid
    - login form
- Do not copy copyrighted site code directly.
- Create synthetic/reduced fixtures that mimic structure and behavior.
- Add compatibility scoring:
    - render completeness
    - interaction completeness
    - diagnostics count
    - unsupported features
- Add expected screenshots/hashes where deterministic.
- Add regression tests for each corpus fixture.

### Definition of done

- Corpus fixtures run locally and deterministically.
- Compatibility report shows progress per fixture.
- No fixture-specific engine hacks are introduced.
- Failures are either fixed generally or documented.
- This corpus becomes the main compatibility gate.

---

## Milestone 56: performance pass

Goal: keep Webby usable as pages grow.

### Tasks

- Add performance measurements for:
    - HTML parsing
    - CSS parsing/matching
    - style tree generation
    - layout
    - display-list building
    - rendering
    - JavaScript execution if implemented
- Add large-page benchmarks or smoke tests.
- Add profiling notes.
- Reduce obvious unnecessary clones/allocations.
- Add caching for selector matching or style computation if justified.
- Add incremental recomputation where simple and safe.
- Add memory limits or diagnostics for very large pages.
- Keep tests deterministic.

### Definition of done

- Large DOM fixture loads within documented budget.
- Many CSS rules fixture loads within documented budget.
- Image-heavy fixture remains bounded.
- Performance docs identify bottlenecks.
- Optimizations do not break architecture boundaries.
- No unsafe optimizations unless explicitly approved.

---

## Milestone 57: error recovery and malformed web hardening

Goal: handle broken real-world input gracefully.

### Tasks

- Add malformed HTML/CSS/JS/network fixture corpus.
- Add fuzz-like generated tests for:
    - HTML tokenizer/parser
    - CSS parser
    - URL resolver
    - image decoder boundary behavior
    - layout tiny/huge dimensions
- Improve recovery for common broken markup.
- Ensure parser/layout/render loops always make progress.
- Add diagnostics where helpful.
- Add crash-regression tests for bugs found during fuzz-like testing.

### Definition of done

- Malformed inputs do not panic.
- Infinite loops are guarded by tests.
- Diagnostics are deterministic.
- Fuzz-like tests are stable enough for CI.
- Recovery behavior is documented.
- No fixture-specific hacks are added.

---

## Milestone 58: real-site smoke testing

Goal: test Webby against a curated set of real websites without claiming full compatibility.

### Tasks

- Add a manual/optional smoke-test list:
    - `https://example.com`
    - `https://info.cern.ch`
    - simple documentation sites
    - simple blogs
    - static landing pages
    - search engine result page if feasible
- Add script to run smoke tests manually.
- Capture:
    - load success/failure
    - diagnostics
    - screenshots/PPM output
    - unsupported feature notes
- Avoid making network-dependent tests required in normal CI.
- Add reduced local fixtures for any repeated real-site failures.

### Definition of done

- Smoke-test procedure is documented.
- Results can be reproduced manually.
- Network-dependent tests are optional.
- Findings feed back into local reduced fixtures.
- Compatibility claims remain honest.

---

## Milestone 59: browser UX finish

Goal: make Webby usable for basic browsing/searching.

### Tasks

- Improve address bar:
    - select all
    - copy/paste
    - cursor movement
    - history suggestions optional
- Add find-in-page.
- Add page title display.
- Add favicon display if implemented.
- Add status bar for hovered link URL.
- Add downloads for unsupported/downloaded resources.
- Add copy current URL.
- Add open file action.
- Add better error pages.
- Add loading progress/diagnostics UI.
- Add keyboard shortcut help.

### Definition of done

- User can browse/search without relying on terminal output.
- Common navigation actions are visible/clickable.
- Errors are understandable.
- Download/open-file behavior is deterministic.
- UX logic stays in `webby_app`.
- Engine crates remain UI-agnostic.

---

## Milestone 60: release-quality hardening and final demo

Goal: package Webby as a credible from-scratch browser-engine project.

### Tasks

- Update README with:
    - project goals
    - build instructions
    - screenshots
    - supported features
    - limitations
- Add architecture diagrams.
- Add demo script.
- Add demo mini-sites.
- Add final compatibility report.
- Add release checklist.
- Add contribution guide if useful.
- Ensure docs match implemented behavior.
- Ensure all validation commands pass.
- Tag known future work clearly.

### Definition of done

- Project can be cloned, built, tested, and demoed by another developer.
- README accurately describes what Webby can and cannot do.
- Demo clearly shows navigation, search, rendering, forms, images, CSS, JavaScript basics, and tabs.
- Full validation loop passes.
- Known limitations are honest and specific.
- No important TODOs remain without follow-up issue references or roadmap entries.

---

## Milestone 61: HTML parsing correctness upgrade

Goal: make Webby handle real-world broken HTML much closer to browser behavior.

### Tasks

- Improve tree construction for malformed nesting.
- Add better handling for optional closing tags:
    - `p`
    - `li`
    - `tr`
    - `td`
    - `th`
    - `thead`
    - `tbody`
- Improve handling for:
    - comments
    - doctypes
    - raw text elements
    - escapable raw text elements
    - foreign content fallback
- Add parser recovery fixtures based on common malformed HTML.
- Add comparison-style tests against expected DOM dumps.
- Add diagnostics for unrecoverable parser cases.
- Keep parser forgiving and non-panicking.

### Definition of done

- Common malformed real-world HTML renders sensibly.
- Optional closing tag fixtures pass.
- Parser recovery behavior is deterministic.
- Parser loops always make progress.
- No layout/render code depends on parser quirks.
- Known unsupported HTML5 parser cases are documented.

---

## Milestone 62: CSS cascade and inheritance correctness upgrade

Goal: make computed style behavior closer to real CSS.

### Tasks

- Improve inheritance for inherited properties:
    - color
    - font-family
    - font-size
    - font-weight
    - line-height
    - text-align
    - visibility
- Add CSS-wide keywords:
    - `inherit`
    - `initial`
    - `unset`
- Add `!important` support.
- Improve source ordering across:
    - user-agent defaults
    - external stylesheets
    - style blocks
    - inline styles
    - important declarations
- Add better diagnostics for invalid declarations.
- Add cascade tests for nested elements and conflicting rules.

### Definition of done

- Inherited properties behave predictably.
- `inherit`, `initial`, and `unset` work for supported properties.
- `!important` ordering is deterministic.
- Inline style and important declarations interact correctly.
- Existing CSS behavior does not regress.
- Style dump clearly shows final computed values.

---

## Milestone 63: scrolling, overflow, and clipping model

Goal: support scrollable areas and overflow behavior used by real sites.

### Tasks

- Add CSS support for:
    - `overflow`
    - `overflow-x`
    - `overflow-y`
- Support values:
    - `visible`
    - `hidden`
    - `scroll`
    - `auto`
- Add scroll containers in layout/app state.
- Support wheel scrolling inside nested scroll containers.
- Clip rendering to scroll container bounds.
- Ensure hit testing respects scroll offsets and clipping.
- Add basic scrollbar rendering optional.
- Preserve page-level scrolling behavior.

### Definition of done

- Nested scroll containers work.
- Overflow hidden clips content.
- Hit testing does not select clipped invisible content.
- Scroll state is deterministic and per-tab/per-container.
- Rendering clips safely.
- Page scroll and nested scroll do not corrupt each other.

---

## Milestone 64: downloads and content disposition

Goal: handle resources that should be downloaded instead of rendered.

### Tasks

- Detect downloadable responses by:
    - content type
    - `Content-Disposition`
    - unsupported media type
- Add download manager state in `webby_app`.
- Add deterministic download destination behavior.
- Add CLI download command if useful.
- Handle duplicate filenames safely.
- Handle failed downloads with structured errors.
- Add download progress diagnostics if feasible.
- Do not try to render unsupported binary content as HTML.

### Definition of done

- Unsupported file links can be downloaded.
- Download failures do not crash the app.
- Filenames are sanitized.
- Existing page navigation does not treat downloads as page loads.
- Download state is testable without native window.
- Download behavior is documented.

---

## Milestone 65: authentication and basic HTTP credentials

Goal: support basic authenticated browsing flows without implementing a password manager.

### Tasks

- Support HTTP Basic authentication challenge handling.
- Add app state for credential prompt/result.
- Allow user-provided credentials in tests and CLI.
- Store credentials in memory only unless explicitly configured.
- Add support for authenticated resource loads through `webby_net`.
- Ensure failed authentication becomes a visible error state.
- Do not implement password saving yet.
- Do not leak credentials into diagnostics/logs.

### Definition of done

- Basic-auth test server fixture works.
- Wrong credentials fail gracefully.
- Credentials are not persisted by default.
- Diagnostics redact credentials.
- Auth handling stays in network/app coordination layers.
- Parser/layout/render remain auth-agnostic.

---

## Milestone 66: privacy controls and data clearing

Goal: give Webby user-facing control over stored browsing data.

### Tasks

- Add settings for:
    - enable/disable cookies
    - enable/disable localStorage
    - enable/disable disk cache
    - clear data on exit
- Add commands/actions to clear:
    - history
    - bookmarks optional
    - cookies
    - localStorage
    - sessionStorage
    - cache
- Add profile summary command.
- Add app UI action or CLI commands for clearing data.
- Ensure clearing data cannot corrupt active tabs.
- Document where data is stored.

### Definition of done

- User can clear browsing data deterministically.
- Disabled storage/cookies/cache are respected.
- Clear-on-exit behavior is tested.
- Corrupt/missing data files are handled safely.
- Privacy settings are documented.
- Engine crates remain storage-policy agnostic.

---

## Milestone 67: navigation lifecycle and page cancellation

Goal: make navigation robust when users interrupt loads or switch pages.

### Tasks

- Add explicit navigation lifecycle states:
    - idle
    - resolving
    - loading main resource
    - loading subresources
    - executing scripts
    - rendering
    - complete
    - failed
    - cancelled
- Support cancellation of in-progress navigation if feasible.
- Ensure late resource/script/timer results cannot mutate a replaced page.
- Add page generation/token checks.
- Make reload/back/forward/link/form/JS navigation use the lifecycle model.
- Add diagnostics for cancelled navigations.

### Definition of done

- Starting a new navigation invalidates old pending work.
- Late results cannot corrupt current page.
- Cancelled navigation is distinct from failure.
- Navigation lifecycle is testable without native UI.
- Existing navigation semantics remain correct.
- App state remains deterministic.

---

## Milestone 68: memory and resource ownership hardening

Goal: reduce leaks, accidental clones, and unbounded memory growth.

### Tasks

- Audit large data ownership:
    - DOM
    - stylesheets
    - layout tree
    - display list
    - images
    - cache
    - JS values
- Add memory limits for:
    - decoded images
    - DOM node count
    - stylesheet size
    - script size
    - cache size
- Add structured errors/diagnostics when limits are exceeded.
- Avoid unnecessary clones in hot paths.
- Add tests for limit enforcement.
- Add memory notes to architecture docs.

### Definition of done

- Large resources are bounded.
- Limit failures are graceful.
- No obvious unbounded vectors/maps for untrusted input.
- Memory-heavy fixtures are tested.
- Performance and memory tradeoffs are documented.
- No unsafe Rust is introduced.

---

## Milestone 69: rendering backend abstraction

Goal: prepare Webby for better rendering backends without rewriting the engine.

### Tasks

- Define a rendering backend trait or abstraction.
- Keep current software renderer as one backend.
- Add optional accelerated backend only if scoped and justified.
- Ensure display list remains backend-independent.
- Ensure tests can still use deterministic software backend.
- Add backend selection in config/CLI if useful.
- Keep native app decoupled from renderer internals.

### Definition of done

- Software renderer still passes all tests.
- Display list is backend-neutral.
- App can render through backend abstraction.
- Deterministic test renderer remains default for tests.
- No rendering backend leaks into layout/style/DOM.
- Backend docs explain supported behavior and limitations.

---

## Milestone 70: advanced compatibility triage and roadmap reset

Goal: evaluate Webby against real-world reduced sites and decide the next direction based on evidence.

### Tasks

- Run the compatibility corpus from Milestone 55.
- Run optional real-site smoke tests from Milestone 58.
- Categorize failures:
    - HTML parser
    - CSS selector/cascade
    - layout
    - rendering
    - JavaScript
    - DOM API
    - events
    - network/security
    - storage/cookies
    - performance
- Produce a compatibility scorecard.
- Convert top recurring failures into new roadmap issues/milestones.
- Remove stale roadmap items that are no longer useful.
- Update README and known limitations.

### Definition of done

- Compatibility report is current and honest.
- Top failure categories are known.
- Next roadmap direction is evidence-based.
- No speculative features are prioritized over recurring failures.
- Project docs match implemented behavior.

### Post-Milestone 70 direction

The reduced local compatibility corpus remains Webby's deterministic gate.
Future roadmap items should be created from repeated evidence in
`docs/COMPATIBILITY.md`, especially:

- remote/snapshot resource coordination for linked stylesheets, scripts,
  images, and fonts
- high-frequency CSS diagnostics from real-site smoke output
- JavaScript DOM/Web API gaps observed repeatedly in local or smoke fixtures
- SVG/media compatibility gaps that affect reduced fixtures

Do not prioritize speculative browser features without a reduced fixture or
repeated smoke finding.
