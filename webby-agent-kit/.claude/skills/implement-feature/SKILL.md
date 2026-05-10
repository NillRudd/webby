---
name: implement-webby-feature
description: Implement one small Webby browser-engine feature with tests and a concise summary.
---

Use this skill when adding one coherent feature to Webby.

1. Read `AGENTS.md` and `.agents/context-map.md`.
2. Identify the owning module.
3. State the exact behavior to implement.
4. Make the smallest code change.
5. Add or update tests for pure logic.
6. Run `cargo fmt` and `cargo test` if possible.
7. Return changed files, behavior, and test results.

Do not add Chromium, WebKit, Blink, Electron, CEF, or WebView rendering.
