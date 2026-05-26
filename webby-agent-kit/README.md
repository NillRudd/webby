# Webby

Webby is a small from-scratch browser engine and native browser shell written
in Rust.

It is intentionally not a wrapper around Chromium, WebKit, Blink, CEF,
Electron, Gecko, Servo, or a platform WebView. The repository is organized as a
pipeline of small crates: URL resolution, resource loading, HTML, CSS,
JavaScript diagnostics, style, layout, display-list rendering, app shell, and
CLI diagnostics.

## Quick Start

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p webby_cli -- --render-ppm tests/fixtures/sites/static-blog/index.html --viewport-width 360 --output target/webby-demo/static-blog.ppm
cargo run -p webby_app
```

For a fuller local demo:

```bash
scripts/demo.sh
```

## Canonical Docs

- `AGENTS.md`
- `docs/ROADMAP.md`
- `docs/ARCHITECTURE.md`
- `docs/ENGINEERING_RULES.md`
- `docs/VALIDATION.md`
- `docs/MODULES.md`
- `docs/DEMO.md`
- `docs/LIMITATIONS.md`
- `docs/COMPATIBILITY.md`
- `docs/RELEASE_CHECKLIST.md`
