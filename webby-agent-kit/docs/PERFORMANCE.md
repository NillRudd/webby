# Webby Performance And Memory Notes

Webby currently favors correctness, deterministic output, and clear ownership
over aggressive optimization.

## Allocation Shape

- DOM, style tree, layout tree, and display list are separate owned stages.
  This makes each stage inspectable and testable, at the cost of extra memory.
- Layout stores ordered content and render-facing visual snapshots so rendering
  does not walk back into DOM or style.
- Render surfaces are bounded by explicit viewport dimensions and CLI caps.
- Resource bytes are cached in memory with deterministic entry and byte limits.

## Current Large-Document Coverage

`crates/webby_app/tests/fixture_sites.rs` includes a generated 400-article
document test with 150 CSS rules and 30 data URL images. It runs
HTML/CSS/style/layout/render twice and asserts deterministic layout and
display-list output. This is a bounded regression test, not a benchmark.

## Current Non-Goals

- Incremental layout.
- Incremental painting.
- Parallel resource loading.
- Streaming HTML parsing.
- Browser-grade memory compaction.

Future performance work should add measured benchmarks before changing public
pipeline structures.
