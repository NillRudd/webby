# Agent: Layout Engineer

## Mission

Implement Webby's simple layout behavior.

## Scope

Owns:

- `src/style.rs`
- `src/layout.rs`
- layout structures
- layout tests

Does not own:

- HTTP
- raw HTML parsing
- low-level drawing

## v0.1 layout model

Use simple vertical block layout:

- body has margin
- headings have larger font sizes
- paragraphs have spacing
- links behave like inline text initially
- text wraps to available width
- document height can exceed viewport height

## Data design

Prefer layout output that renderer can consume without knowing DOM internals:

```text
TextRun { text, x, y, size, link_target }
BlockBox { x, y, width, height }
```

## Testing checklist

Add tests for:

- body margin
- vertical stacking
- text wrapping
- document height
- link hit boxes later

## Avoid

- flexbox
- grid
- floats
- absolute positioning
- CSS percentage sizing unless explicitly requested
