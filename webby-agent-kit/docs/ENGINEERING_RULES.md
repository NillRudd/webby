# Webby Engineering Rules

These rules are mandatory for all implementation, review, and refactor passes.

Webby is not safety-critical software, but it should be engineered like a serious browser-engine project. Prefer boring, explicit, testable code over clever code.

## Short version

1. No fake implementations.
2. No runtime `unwrap`, `expect`, `panic!`, `todo!`, or `unimplemented!`.
3. Every fallible operation returns `WebbyResult`.
4. Each crate owns exactly one layer of the browser pipeline.
5. Renderer never reaches back into DOM/style/parser.
6. One authoritative representation for state.
7. Parser/layout/render loops must prove progress.
8. All user input is validated at boundaries.
9. Dumps and render outputs are deterministic.
10. Every feature has happy-path, edge-case, error, and integration tests.

## 1. No hidden shortcuts

Do not implement behavior only to satisfy a known test fixture.

Every implementation must be general within the milestone scope.

Bad:
- hardcoding `examples/simple.html`
- special-casing one test string
- returning fake data from a module that should compute real data
- silently ignoring invalid input without documenting behavior

Required:
- implement the actual algorithm for the scoped feature
- add edge-case tests
- document intentional limitations

## 2. No forbidden runtime panic paths

Runtime/library code must not use:

```text
unwrap()
expect()
panic!()
todo!()
unimplemented!()
```

Also avoid panic-prone indexing/slicing unless guarded.

Bad:

let item = items[0];
let prefix = &text[..5];

Good:

let Some(item) = items.first() else {
    return Err(WebbyError::invalid_input("missing item"));
};

Tests may use panics when appropriate, but prefer ?, assertions, and helper functions.

## 3. Every fallible operation returns structured errors

Use WebbyResult<T> and WebbyError.

Do not return plain strings as errors from core crates.

Every error should make clear:

what failed
which input or stage failed, when safe/useful
whether it was URL, network, parse, CSS, style, layout, render, app, or CLI related
## 4. Keep module boundaries strict

Each crate owns its domain:

webby_url       URL/search/file input resolution
webby_net       resource loading
webby_dom       DOM data structures
webby_html      HTML parsing and visible text extraction
webby_css       CSS parsing
webby_style     cascade, defaults, computed styles
webby_layout    layout tree and geometry
webby_render    display list and software rendering
webby_app       app state, navigation, shell, event loop
webby_cli       command-line composition/debugging

Forbidden:

webby_render importing webby_html, webby_dom, webby_style, webby_net, or webby_app
webby_layout fetching resources or parsing HTML
webby_app reimplementing parser/style/layout/render algorithms
webby_cli owning core algorithms

Allowed:

CLI and app may compose existing crates.
Tests may use dev-dependencies to build realistic integration fixtures.
## 5. Preserve pipeline direction

The browser pipeline must remain directional:

input
→ URL resolution
→ resource loading
→ HTML/DOM
→ CSS/style
→ layout
→ display list
→ render
→ app presentation

Later stages must not reach backward into earlier stages unless explicitly designed.

Bad:

renderer walking DOM to get colors
app parsing HTML to find links
layout fetching image resources
## 6. One authoritative representation

Do not duplicate state that can drift.

Examples:

link href should have one source of truth in a text run
ordered layout content should have one authoritative order
history current index should not conflict with current URL
failed URL should not be silently encoded in unrelated fields

If duplicate views are useful, expose them as computed helper methods over the authoritative representation.

## 7. Small functions and explicit control flow

Target:

functions under 60 logical lines
one responsibility per function
early returns for error cases
clear helper functions

Avoid:

clever iterator chains that hide error handling
large functions that parse, style, layout, and render together
deeply nested control flow
god objects

If a function grows large, split it into named helpers.

## 8. Bounded loops and progress guarantees

Every parser, tokenizer, layout, and renderer loop must guarantee progress.

For input cursors:

every branch must advance the cursor, return, or produce an error
malformed input must not cause infinite loops

For rendering/layout:

tiny viewport sizes must not cause infinite loops
wrapping algorithms must handle long words
clipping must handle out-of-bounds rectangles safely

Add tests for malformed input and tiny dimensions.

## 9. Validate inputs at boundaries

Validate at public API and user-input boundaries:

CLI arguments
viewport sizes
URLs
file paths
dimensions
color values
CSS lengths
image width/height attributes

Internal helpers may assume validated input only if documented.

## 10. Deterministic outputs

Debug outputs must be deterministic:

DOM dump
style dump
layout dump
display-list dump
render snapshot/PPM

Avoid nondeterministic map iteration in public/debug output. Sort keys where needed.

## 11. Tests must prove behavior

Every feature should include:

happy-path test
edge-case test
error behavior test
integration test when crossing crate boundaries

Avoid tests that only assert “some output exists.”

Prefer tests that assert:

exact resolved URL
expected tree shape
expected style value
layout geometry relationship
display command order
error state semantics
## 12. Reviews must check architecture drift

Every review pass must check:

no module boundary violations
no fake implementations
no runtime panic paths
no state duplication
no stale docs
no fixture-specific hardcoding
validation commands pass
## 13. Documentation must move with behavior

When behavior changes, update:

module docs
CLI docs
architecture docs if boundaries change
roadmap if milestone scope changes
comments on public structs/functions

Stale docs are defects.

## 14. Dependencies must be justified

Before adding a dependency, state why it is needed.

Allowed dependency reasons:

platform/windowing integration
HTTP client
text/font rendering
image decoding
test utilities

Avoid dependencies that hide the project’s purpose:

Chromium
WebKit
WebView
Electron
full browser engines
huge frameworks for small internal tasks
## 15. Unsafe Rust is forbidden by default

No unsafe unless explicitly approved.

If unsafe is introduced:

isolate it in one module
document the invariant
add tests around the safe wrapper
explain why safe Rust was insufficient
## 16. Warnings are errors

The project must pass:

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
rg 'unwrap\(|expect\(|panic!|todo!|unimplemented!' crates

No warnings. No forbidden runtime panic macros.

## 17. Every milestone must end with explicit limitations

Each milestone response must state:

what was implemented
what was validated
what remains intentionally unsupported
what limitations are known
