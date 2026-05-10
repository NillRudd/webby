# Module Spec: webby_style

## Purpose

Convert DOM into styled nodes. v0.1 should use default user-agent-like styles before implementing full CSS.

## Requirements

- Computed display type: block, inline, none.
- Font size.
- Text color.
- Background color.
- Margins for headings and paragraphs.
- Link styling metadata.

## Public API sketch

```rust
pub struct StyledNode<'a> {
    pub node: &'a Node,
    pub style: ComputedStyle,
    pub children: Vec<StyledNode<'a>>,
}
```

## Tests

- headings get larger font sizes
- links are marked clickable/styled
- script/style/head are display none
