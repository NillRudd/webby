//! Document tree data structures owned by Webby.

use std::collections::BTreeMap;

/// Stable DOM node identifier assigned in deterministic tree order.
pub type NodeId = u64;

/// Parsed document root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Root document node.
    pub root: Node,
}

impl Document {
    /// Creates an empty document.
    pub fn empty() -> Self {
        Self {
            root: Node::document(),
        }
    }

    /// Assigns stable node IDs in document-order traversal. The document root
    /// is always `0`; descendants start at `1`.
    pub fn assign_stable_ids(&mut self) {
        let mut next_id = 0;
        assign_ids_from(&mut self.root, &mut next_id);
    }

    /// Returns the next unused node id for appending new nodes.
    pub fn next_node_id(&self) -> NodeId {
        max_node_id(&self.root).saturating_add(1)
    }

    /// Finds an immutable node by id.
    pub fn find_node(&self, id: NodeId) -> Option<&Node> {
        find_node(&self.root, id)
    }

    /// Finds a mutable node by id.
    pub fn find_node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        find_node_mut(&mut self.root, id)
    }

    /// Sets an element attribute. Returns false if the id is missing or not an
    /// element.
    pub fn set_attribute(&mut self, id: NodeId, name: &str, value: impl Into<String>) -> bool {
        let Some(node) = self.find_node_mut(id) else {
            return false;
        };
        let NodeKind::Element(element) = &mut node.kind else {
            return false;
        };
        element
            .attributes
            .insert(name.to_ascii_lowercase(), value.into());
        true
    }

    /// Replaces a node's children with a single text node.
    pub fn set_text_content(&mut self, id: NodeId, text: impl Into<String>) -> bool {
        let next_id = self.next_node_id();
        let Some(node) = self.find_node_mut(id) else {
            return false;
        };
        match &mut node.kind {
            NodeKind::Text(value) => {
                *value = text.into();
            }
            NodeKind::Document | NodeKind::Element(_) => {
                let mut child = Node::text(text);
                child.id = next_id;
                node.children.clear();
                node.children.push(child);
            }
        }
        true
    }

    /// Appends `child` to an existing parent. Returns false if the parent is
    /// missing or is a text node.
    pub fn append_child(&mut self, parent_id: NodeId, child: Node) -> bool {
        let Some(parent) = self.find_node_mut(parent_id) else {
            return false;
        };
        if matches!(parent.kind, NodeKind::Text(_)) {
            return false;
        }
        parent.children.push(child);
        true
    }

    /// Ensures an element has a shadow root. Returns false if the host is
    /// missing or cannot host children.
    pub fn attach_shadow_root(&mut self, host_id: NodeId) -> bool {
        let Some(host) = self.find_node_mut(host_id) else {
            return false;
        };
        if matches!(host.kind, NodeKind::Text(_)) {
            return false;
        }
        host.shadow_children.clear();
        true
    }

    /// Appends `child` to an element's shadow root. Returns false if the host
    /// is missing or cannot host children.
    pub fn append_shadow_child(&mut self, host_id: NodeId, child: Node) -> bool {
        let Some(host) = self.find_node_mut(host_id) else {
            return false;
        };
        if matches!(host.kind, NodeKind::Text(_)) {
            return false;
        }
        host.shadow_children.push(child);
        true
    }

    /// Removes a node by id and returns it. The document root cannot be
    /// removed.
    pub fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        if id == self.root.id {
            return None;
        }
        remove_node(&mut self.root, id)
    }
}

/// A DOM node with stable child traversal order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Stable node identifier.
    pub id: NodeId,
    /// Node payload.
    pub kind: NodeKind,
    /// Child nodes in source/tree order.
    pub children: Vec<Node>,
    /// Shadow-root children in tree order. When present, these are the
    /// render-facing children for Webby's scoped Shadow DOM subset.
    pub shadow_children: Vec<Node>,
}

impl Node {
    /// Creates a document node.
    pub fn document() -> Self {
        Self {
            id: 0,
            kind: NodeKind::Document,
            children: Vec::new(),
            shadow_children: Vec::new(),
        }
    }

    /// Creates an element node and normalizes the tag name to lowercase ASCII.
    pub fn element(tag_name: impl AsRef<str>, attributes: BTreeMap<String, String>) -> Self {
        Self {
            id: 0,
            kind: NodeKind::Element(ElementData {
                tag_name: tag_name.as_ref().to_ascii_lowercase(),
                attributes,
            }),
            children: Vec::new(),
            shadow_children: Vec::new(),
        }
    }

    /// Creates a text node.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: NodeKind::Text(text.into()),
            children: Vec::new(),
            shadow_children: Vec::new(),
        }
    }

    /// Children used by style/layout for Webby's composed tree. Shadow
    /// children replace light children when a shadow root has content.
    pub fn render_children(&self) -> &[Node] {
        if self.shadow_children.is_empty() {
            &self.children
        } else {
            &self.shadow_children
        }
    }

    /// All structural children, including shadow-root children, for metadata
    /// collection and mutation lookup.
    pub fn tree_children(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().chain(self.shadow_children.iter())
    }
}

/// Node payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// Synthetic document root.
    Document,
    /// HTML element.
    Element(ElementData),
    /// Text content.
    Text(String),
}

/// Element name and attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementData {
    /// Lowercase tag name.
    pub tag_name: String,
    /// Attributes sorted by key for deterministic tests and dumps.
    pub attributes: BTreeMap<String, String>,
}

fn assign_ids_from(node: &mut Node, next_id: &mut NodeId) {
    node.id = *next_id;
    *next_id = next_id.saturating_add(1);
    for child in &mut node.children {
        assign_ids_from(child, next_id);
    }
    for child in &mut node.shadow_children {
        assign_ids_from(child, next_id);
    }
}

fn max_node_id(node: &Node) -> NodeId {
    node.children
        .iter()
        .chain(node.shadow_children.iter())
        .map(max_node_id)
        .max()
        .map_or(node.id, |child_id| node.id.max(child_id))
}

fn find_node(node: &Node, id: NodeId) -> Option<&Node> {
    if node.id == id {
        return Some(node);
    }
    node.children
        .iter()
        .chain(node.shadow_children.iter())
        .find_map(|child| find_node(child, id))
}

fn find_node_mut(node: &mut Node, id: NodeId) -> Option<&mut Node> {
    if node.id == id {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_node_mut(child, id) {
            return Some(found);
        }
    }
    for child in &mut node.shadow_children {
        if let Some(found) = find_node_mut(child, id) {
            return Some(found);
        }
    }
    None
}

fn remove_node(parent: &mut Node, id: NodeId) -> Option<Node> {
    if let Some(index) = parent.children.iter().position(|child| child.id == id) {
        return Some(parent.children.remove(index));
    }
    for child in &mut parent.children {
        if let Some(removed) = remove_node(child, id) {
            return Some(removed);
        }
    }
    if let Some(index) = parent
        .shadow_children
        .iter()
        .position(|child| child.id == id)
    {
        return Some(parent.shadow_children.remove(index));
    }
    for child in &mut parent.shadow_children {
        if let Some(removed) = remove_node(child, id) {
            return Some(removed);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Document, Node, NodeKind};
    use std::collections::BTreeMap;

    #[test]
    fn empty_document_has_document_root() {
        let document = Document::empty();

        assert_eq!(document.root.id, 0);
        assert!(matches!(document.root.kind, NodeKind::Document));
        assert!(document.root.children.is_empty());
    }

    #[test]
    fn element_constructor_normalizes_tag_name() {
        let node = Node::element("H1", BTreeMap::new());

        assert!(matches!(
            node.kind,
            NodeKind::Element(element) if element.tag_name == "h1"
        ));
    }

    #[test]
    fn stable_ids_are_assigned_in_tree_order() {
        let mut document = Document::empty();
        document
            .root
            .children
            .push(Node::element("body", BTreeMap::new()));
        document.root.children[0].children.push(Node::text("Hello"));

        document.assign_stable_ids();

        assert_eq!(document.root.id, 0);
        assert_eq!(document.root.children[0].id, 1);
        assert_eq!(document.root.children[0].children[0].id, 2);
    }

    #[test]
    fn mutation_helpers_update_tree_without_panicking() {
        let mut document = Document::empty();
        document
            .root
            .children
            .push(Node::element("body", BTreeMap::new()));
        document.assign_stable_ids();

        assert!(document.set_attribute(1, "ID", "main"));
        assert!(document.set_text_content(1, "Changed"));
        assert_eq!(document.root.children[0].children[0].id, 2);
        assert!(document.remove_node(2).is_some());
        assert!(document.root.children[0].children.is_empty());
    }

    #[test]
    fn shadow_children_are_assigned_and_found_deterministically() {
        let mut document = Document::empty();
        document
            .root
            .children
            .push(Node::element("x-card", BTreeMap::new()));
        document.assign_stable_ids();

        assert!(document.attach_shadow_root(1));
        assert!(document.append_shadow_child(1, Node::text("Shadow")));
        document.assign_stable_ids();

        let host = document.find_node(1);
        assert!(host.is_some_and(|node| node.render_children().len() == 1));
        assert!(matches!(
            document.find_node(2).map(|node| &node.kind),
            Some(NodeKind::Text(text)) if text == "Shadow"
        ));
    }
}
