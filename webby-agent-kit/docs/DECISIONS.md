# Webby Architecture Decisions

This file is the canonical index for architectural decisions. Longer records may
live in `docs/decisions/` when they need more context.

## ADR 0001: From-Scratch Engine

Webby is implemented as a small browser engine and shell in Rust. It must not
use Chromium, Blink, WebKit, CEF, Electron, Gecko, Servo, or platform WebView
for page rendering.

Consequences:

- The page pipeline stays explicit and testable.
- External libraries are allowed for infrastructure such as HTTP, fonts,
  windows, image decoding, and serialization.
- Browser behavior is added milestone by milestone through Webby's own crates.

Source: `docs/decisions/0001-from-scratch-engine.md`.

## ADR 0002: Crate-Owned Pipeline Layers

Each crate owns one layer of the pipeline. App and CLI compose crates; they do
not own parser, style, layout, render, networking, CSS, image, text, or
persistence algorithms.

Consequences:

- Reviews treat boundary drift as a blocking issue.
- Render consumes layout/display-list data and never reaches back into DOM or
  style.
- Layout never fetches resources or draws pixels.

Source: `docs/ARCHITECTURE.md`.

## ADR 0003: Deterministic Local Profile State

Persistent browser state lives in `webby_state` as deterministic JSON files for
config, history, bookmarks, and recent pages.

Consequences:

- Missing files use defaults.
- Corrupt files return structured parse errors.
- Failed navigations are not recorded as successful history.
- App and CLI use the persistence API without parsing JSON directly.

Source: `docs/MODULES.md`.
