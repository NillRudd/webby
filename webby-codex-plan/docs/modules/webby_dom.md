# Module Spec: webby_dom

## Purpose

Represent Webby's document tree.

## Public API sketch

```rust
pub struct Document {
    pub root: Node,
}

pub struct Node {
    pub kind: NodeKind,
    pub children: Vec<Node>,
}

pub enum NodeKind {
    Document,
    Element(ElementData),
    Text(String),
}

pub struct ElementData {
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
}
```

## Requirements

- Stable traversal order.
- Case normalization for HTML tag names.
- No rendering logic.
- Convenience helpers for tests are allowed.
