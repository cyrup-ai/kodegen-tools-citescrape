use std::rc::Rc;
use std::rc::Weak;

use markup5ever_rcdom::{Node, NodeData};

/// RAII guard to ensure parent reference is restored even on panic.
/// 
/// This guard uses the Drop trait to guarantee that when it goes out of scope
/// (either normally or during stack unwinding from a panic), the parent reference
/// is restored to the original node's parent Cell.
struct ParentGuard<'a> {
    node: &'a Rc<Node>,
    value: Option<Option<Weak<Node>>>,
}

impl<'a> Drop for ParentGuard<'a> {
    fn drop(&mut self) {
        // Restore the parent reference if we still have it
        if let Some(value) = self.value.take() {
            self.node.parent.set(value);
        }
    }
}

impl<'a> ParentGuard<'a> {
    /// Create a new guard that takes ownership of the parent reference.
    /// The reference will be automatically restored when the guard is dropped.
    fn new(node: &'a Rc<Node>) -> Self {
        let value = node.parent.take();
        Self {
            node,
            value: Some(value),
        }
    }
    
    /// Get a reference to the parent value without consuming the guard.
    fn get(&self) -> &Option<Weak<Node>> {
        // We know value is Some because it's only taken in drop()
        self.value.as_ref().unwrap()
    }
}

pub(crate) fn get_node_tag_name(node: &Rc<Node>) -> Option<&str> {
    match &node.data {
        NodeData::Document => Some("html"),
        NodeData::Element { name, .. } => Some(&name.local),
        _ => None,
    }
}

pub(crate) fn get_parent_node(node: &Rc<Node>) -> Option<Rc<Node>> {
    // Create guard - it will restore parent reference on drop (normal or panic)
    let guard = ParentGuard::new(node);
    
    // Access the parent value through the guard
    let parent_weak = guard.get().as_ref()?;
    
    // Try to upgrade the weak reference
    parent_weak.upgrade()
    
    // Guard's Drop automatically restores node.parent here
}

// Check to see if node's parent's tag name matches the provided string.
pub(crate) fn parent_tag_name_equals(node: &Rc<Node>, tag_names: &Vec<&str>) -> bool {
    if let Some(parent) = get_parent_node(node)
        && let Some(actual_tag_name) = get_node_tag_name(&parent)
        && tag_names.contains(&actual_tag_name)
    {
        true
    } else {
        false
    }
}

