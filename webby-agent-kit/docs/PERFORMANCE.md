# Webby Performance And Memory Notes

Webby currently favors correctness, deterministic output, and clear ownership
over aggressive optimization.

## Measured Smoke Gate

`crates/webby_app/tests/performance_smoke.rs` measures the public engine stages
with `std::time::Instant`:

- HTML parsing
- CSS parsing
- selector matching and style-tree generation
- layout
- display-list construction
- software rendering
- JavaScript execution

The generated stage fixture contains 280 articles and 180 CSS rules. Each stage
has a deliberately generous 10-second debug-build smoke budget. The complete
large-document and image-heavy public-pipeline cases each have a 20-second
budget. These are regression ceilings for ordinary development machines, not
microbenchmark claims.

The image-heavy case renders 80 decoded image instances and bounds the retained
surface to 32 MiB. The large-node case verifies that documents above 1,000 DOM
nodes emit an advisory app-pipeline diagnostic without failing valid rendering.

## Allocation Shape

- DOM, style tree, layout tree, and display list are separate owned stages.
  This makes each stage inspectable and testable, at the cost of extra memory.
- Layout stores ordered content and render-facing visual snapshots so rendering
  does not walk back into DOM or style.
- Render surfaces are bounded by explicit viewport dimensions and CLI caps.
- Resource bytes are cached in memory with deterministic entry and byte limits,
  with an optional disk-backed profile tier.
- Large untrusted expansion points are capped before later stages retain more
  memory: HTML input is limited to 4 MiB, parsed DOMs to 16,384 nodes,
  stylesheets to 1 MiB, coordinated script sources to 256 KiB, and decoded
  images to 16,000,000 pixels.

## Current Large-Document Coverage

`crates/webby_app/tests/fixture_sites.rs` includes a generated 400-article
document test with 150 CSS rules and 30 data URL images. It runs
HTML/CSS/style/layout/render twice and asserts deterministic layout and
display-list output. `crates/webby_app/tests/performance_smoke.rs` adds measured
stage, large-node, and image-heavy smoke gates.

## Profiling Notes

The likely growth costs are intentionally visible:

- selector matching walks stylesheet rules while constructing the style tree
- layout owns ordered fragments and render-facing snapshots
- software rendering allocates an RGBA surface proportional to viewport width
  times scroll height
- JavaScript context startup is meaningful for small scripts

The Milestone 56 pass does not add selector caches or incremental layout.
Current fixtures do not justify cache invalidation complexity yet. Existing
JavaScript dirty-state metadata already keeps rerender decisions explicit for
future incremental work.

## Current Non-Goals

- Incremental layout.
- Incremental painting.
- Parallel resource loading.
- Streaming HTML parsing.
- Browser-grade memory compaction.

Future optimization work should use these measurements and profiling notes
before changing public pipeline structures.
