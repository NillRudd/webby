# AGENTS.md

Agent instructions for **Webby**, a tiny from-scratch browser engine written in Rust.

## Project goal

Build **Webby**: a minimal browser with an address/search bar, simple navigation, networking, basic HTML parsing, text extraction, simple layout, and custom rendering.

Do not turn this into a Chromium/WebKit wrapper. External libraries are allowed for boring infrastructure, but the browser-engine pipeline should be implemented in this repo.

## Non-goals for v0.1

Do not implement these unless explicitly requested:

- JavaScript engine
- full CSS cascade
- flexbox or grid
- extensions
- cookies/login/session storage
- video/audio
- WebRTC
- full HTML5 correctness
- Chromium, Blink, CEF, Electron, or WebView-based rendering

## Preferred stack

- Rust
- `winit` for window/event loop
- `pixels`, `tiny-skia`, or `wgpu` for drawing
- `fontdue` or `cosmic-text` for text
- `reqwest` for HTTP/HTTPS
- `url` and `urlencoding` for URL handling
- normal Rust tests for parser/layout logic

Prefer simple dependencies. Avoid large frameworks unless they clearly reduce non-core work.

## v0.1 target behavior

Webby should support:

- open a window
- show an address/search bar
- allow typing into the bar
- pressing Enter resolves input as URL or search query
- fetch a URL over HTTP/HTTPS
- parse enough HTML to extract visible text
- render headings, paragraphs, links, and simple text
- click links and navigate
- support back/forward only if the base navigation is stable

Search/address behavior:

```text
example.com              -> https://example.com
https://example.com      -> https://example.com
rust browser engine      -> https://www.google.com/search?q=rust+browser+engine
```

## Architecture

Keep the pipeline explicit:

```text
User input
    -> URL resolver
    -> HTTP fetcher
    -> HTML parser
    -> DOM tree
    -> style defaults
    -> layout tree
    -> display list
    -> renderer
    -> window
```

Suggested modules:

```text
src/main.rs
src/app.rs
src/net.rs
src/url_resolver.rs
src/dom.rs
src/html.rs
src/style.rs
src/layout.rs
src/render.rs
src/input.rs
```

Do not mix parsing, layout, and rendering in one file once the prototype grows.

## Coding rules

- Use 4 spaces for indentation.
- Prefer clear structs/enums over clever abstractions.
- Keep functions small enough to test.
- Add tests for parser, URL resolver, and layout calculations.
- Do not use `unwrap()` in non-test code unless failure is truly impossible.
- Return `Result` from fallible operations.
- Keep browser-engine behavior deterministic where possible.

## Agent workflow

Before editing:

1. Read this file.
2. Read `.agents/context-map.md`.
3. Pick the relevant role file from `.agents/agents/`.
4. Pick the relevant skill file from `.agents/skills/`.
5. Make the smallest coherent change.
6. Add or update tests.
7. Run formatting and tests if possible.

## Current implementation priority

Build in this order:

1. Window opens.
2. Draw browser chrome: top bar + page area.
3. Type into address/search bar.
4. Resolve typed input to URL/search URL.
5. Fetch page HTML.
6. Display raw fetched text.
7. Parse HTML and extract visible text.
8. Render headings, paragraphs, and links.
9. Add clickable links.
10. Add scrolling.
11. Add simple box/layout model.
12. Add layout/debug overlay.

Visual progress is more important than theoretical completeness.

## Testing expectations

Useful tests:

- URL resolution
- visible text extraction
- HTML tree shape
- link extraction and relative URL resolution
- layout positions for simple documents
- display-list generation

Prefer snapshot-style tests for layout/display-list once structures stabilize.

## Commit/patch quality

A good patch should include:

- a focused implementation
- tests or a clear reason why none were added
- no unrelated refactors
- no new large dependency without justification
- a short summary of behavior changed

