# Module Spec: webby_layout

## Purpose

Compute geometry for styled content.

## v0.1 requirements

- Vertical block flow.
- Inline text line wrapping.
- Paragraph spacing.
- Heading sizing.
- Link hit rectangles.
- Scrollable content height.

## Public API sketch

```rust
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

pub struct LayoutTree {
    pub root: LayoutBox,
    pub scroll_height: f32,
    pub links: Vec<LinkHitBox>,
}
```

## Tests

- wrapping changes line count when viewport width changes
- block y positions are deterministic
- link rectangles are produced
- `br` forces line break
