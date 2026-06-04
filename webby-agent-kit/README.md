# Webby

Webby is a small from-scratch browser engine and native browser shell written
in Rust. It is a learning-oriented engine with real pipeline boundaries, not a
wrapper around Chromium, WebKit, Blink, CEF, Electron, Gecko, Servo, or a
platform WebView.

## Goals

- Keep browser subsystems small enough to read and test independently.
- Run local pages through a real URL, fetch, parse, style, layout, display-list,
  software-render, and browser-shell pipeline.
- Prefer deterministic debug output and honest limitations over hidden
  compatibility shortcuts.

## What Works

Webby supports local and HTTP resource loading, forgiving HTML parsing, CSS
selectors and common visual properties, block and inline layout, basic
positioning, flexbox, tables, text rasterization, images, links, GET forms,
external stylesheets, a constrained JavaScript runtime, cache and profile
state, cookies, tabs, accessibility basics, and native shell controls.

The CLI exposes deterministic dumps for DOM, CSS diagnostics, style, layout,
display lists, and PPM rendering. The native shell composes the same engine
crates.

## Architecture

```text
input -> URL -> resource loading -> HTML/DOM -> CSS/style
      -> layout -> display list -> software render -> native app
```

Subsystem ownership and dependency direction are documented in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Build And Test

Install a current stable Rust toolchain, then run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Start the native shell:

```bash
cargo run -p webby_app
```

In a headless environment the app exits cleanly after reporting that no native
window is available.

## Demo

Generate a deterministic local demo bundle:

```bash
scripts/demo.sh
```

The bundle is written to `target/webby-demo/`. It covers search resolution,
navigation-oriented DOM inspection, CSS/style output, forms, images,
JavaScript diagnostics, layout, display lists, and software-rendered PPM
images. An interactive native-shell walkthrough, including tabs, is in
[`docs/DEMO.md`](docs/DEMO.md).

The checked-in software-render reference is
[`docs/demo/color-block.ppm`](docs/demo/color-block.ppm). PPM keeps the
rendering artifact dependency-free and byte-for-byte reproducible.

Optional network-dependent smoke testing against a small real-site list is
documented in [`docs/REAL_SITE_SMOKE.md`](docs/REAL_SITE_SMOKE.md).

## Limits

Webby is not a production browser. It does not aim for full HTML5 parsing,
modern CSS coverage, browser-grade JavaScript compatibility, process
isolation, or complete security hardening. The precise supported surface and
known gaps are tracked in:

- [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md)
- [`docs/FINAL_COMPATIBILITY_REPORT.md`](docs/FINAL_COMPATIBILITY_REPORT.md)
- [`docs/LIMITATIONS.md`](docs/LIMITATIONS.md)

## Canonical Docs

- [`AGENTS.md`](AGENTS.md)
- [`docs/ROADMAP.md`](docs/ROADMAP.md)
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- [`docs/ENGINEERING_RULES.md`](docs/ENGINEERING_RULES.md)
- [`docs/VALIDATION.md`](docs/VALIDATION.md)
- [`docs/MODULES.md`](docs/MODULES.md)
- [`docs/DEMO.md`](docs/DEMO.md)
- [`docs/DECISIONS.md`](docs/DECISIONS.md)

Contributor workflow is in [`CONTRIBUTING.md`](CONTRIBUTING.md).
