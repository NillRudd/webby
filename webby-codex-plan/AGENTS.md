# AGENTS.md

Instructions for AI coding agents working on **Webby**.

Webby is a serious from-scratch browser project written in Rust. The goal is not to wrap Chromium, Blink, WebKit, CEF, Electron, or a system WebView. External libraries are allowed for infrastructure, but the browser-engine pipeline must be implemented in this repository.

## Primary objective

Build a minimal but real browser engine and browser shell:

1. A native window opens.
2. A search/address bar accepts user input.
3. Input resolves to either a URL or a search query.
4. Webby fetches the page over HTTP/HTTPS.
5. Webby parses basic HTML into its own DOM.
6. Webby extracts visible content while ignoring scripts/styles for v0.1 rendering.
7. Webby lays out headings, paragraphs, links, simple blocks, and text.
8. Webby renders the page using its own drawing path.
9. Links can be clicked and navigated.
10. The implementation is tested, documented, and maintainable.

## Non-negotiable rules

- No Chromium, Blink, CEF, Electron, Tauri WebView, WebKit, Gecko, Servo, or platform WebView for page rendering.
- Do not implement fake behavior that only handles one hardcoded page.
- Do not hardcode tests into implementation.
- Do not hide failures by weakening tests.
- Do not skip architecture updates when architecture changes.
- Do not commit large generated files, screenshots, build artifacts, or caches.
- Do not add JavaScript support until the v0.1 engine is stable.
- Do not build full CSS, flexbox, grid, media, cookies, sessions, or extension systems for v0.1.

## Expected agent behavior

Before editing code:

1. Read `docs/ROADMAP.md`.
2. Read `docs/ARCHITECTURE.md`.
3. Read the relevant module spec in `docs/modules/`.
4. Read `docs/QUALITY_GATES.md`.
5. Identify the current milestone.
6. Create a small implementation plan.

During implementation:

1. Make cohesive changes.
2. Keep public APIs explicit and documented.
3. Add or update tests with every feature.
4. Run the full validation loop before finishing.
5. Update documentation when behavior changes.

After implementation:

1. Summarize files changed.
2. Summarize validation commands run.
3. Note any remaining limitations honestly.
4. Ensure the definition of done for the milestone is satisfied.

## Style

- Rust edition: 2024 if available, otherwise 2021.
- Indentation: 4 spaces.
- Prefer small modules with clear ownership.
- Prefer explicit structs/enums over stringly typed state.
- Avoid global mutable state.
- Avoid panics in normal user-facing flows.
- Use `Result<T, WebbyError>` for fallible engine operations.
- Keep UI/event-loop code separate from engine code.
- Add unit tests for pure logic and integration tests for pipelines.

## Required validation commands

Run these before reporting completion, unless the project stage makes one impossible. If impossible, explain exactly why.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p webby_cli -- --dump-text examples/simple.html
cargo run -p webby_cli -- --resolve-input "rust browser engine"
```

When the GUI exists, also run a manual smoke test:

```bash
cargo run -p webby_app
```

## Repository layout target

```text
webby/
    AGENTS.md
    Cargo.toml
    crates/
        webby_core/
        webby_url/
        webby_net/
        webby_dom/
        webby_html/
        webby_css/
        webby_style/
        webby_layout/
        webby_render/
        webby_app/
        webby_cli/
    examples/
        simple.html
        links.html
        layout.html
    tests/
        fixtures/
    docs/
        ROADMAP.md
        ARCHITECTURE.md
        QUALITY_GATES.md
        CODEX_MASTER_PROMPT.md
        modules/
        decisions/
```

## v0.1 feature boundary

v0.1 is complete when Webby can:

- open a native app window,
- type into an address/search bar,
- resolve input into a URL,
- fetch HTTPS pages,
- render readable text from simple HTML pages,
- support headings, paragraphs, links, line breaks, and basic block flow,
- click links and navigate,
- expose CLI debug commands for DOM/text/layout,
- pass formatting, linting, tests, and smoke tests.
