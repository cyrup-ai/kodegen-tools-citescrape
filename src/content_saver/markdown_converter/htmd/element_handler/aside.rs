//! Handler for <aside> elements
//!
//! Provides specialized handling for:
//! - Widget asides (social, cookie notices, ads) - filtered out
//! - Generic aside elements treated as block elements

use super::super::Element;
use super::super::node_util::{get_parent_node, get_node_tag_name};
use super::element_util::{is_widget_element_with_context, detect_and_format_admonition};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn aside_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Get parent tag name for context-aware filtering
    let parent_node = get_parent_node(element.node);
    let parent_tag = parent_node.as_ref()
        .and_then(|parent| get_node_tag_name(parent));
    
    // Skip widget elements (but preserve accessibility in table contexts)
    if is_widget_element_with_context(element.attrs, parent_tag) {
        return None;
    }

    // Check for admonition blocks
    if let Some(admonition) = detect_and_format_admonition(handlers, &element) {
        return Some(admonition);
    }

    // Faithful mode: serialize as HTML
    serialize_if_faithful!(handlers, element, 0);

    // Default: treat as block element with proper spacing
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim_matches('\n');

    if content.is_empty() {
        return None;
    }

    Some(format!("\n\n{}\n\n", content).into())
}
