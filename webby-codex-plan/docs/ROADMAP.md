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

## Milestone 10: quality hardening

Goal: turn prototype into a serious project.

### Tasks

- Add fuzz-like parser tests using generated malformed inputs.
- Add integration tests from URL resolver through text extraction.
- Add architecture docs update.
- Add performance notes for large documents.
- Add manual demo script.

### Definition of done

- Validation loop passes.
- Demo script works on a clean checkout.
- Known limitations are documented.
- No important TODOs are left without issue references or follow-up docs.
