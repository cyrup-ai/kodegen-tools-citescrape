//! Handler for <nav> elements
//!
//! Navigation elements contain site navigation that should not appear in markdown.
//! However, some documentation sites place the page title as an <h1> inside nav.
//! This handler extracts only h1 headings and discards all other navigation content.

use std::rc::Rc;
use markup5ever_rcdom::{Node, NodeData};
use super::super::Element;
use super::{HandlerResult, Handlers};

/// Recursively extract only h1 headings from element children.
/// 
/// Pattern adapted from [`headings.rs::walk_heading_children`](./headings.rs).
fn extract_h1_headings(handlers: &dyn Handlers, node: &Rc<Node>) -> String {
    let mut content = String::new();
    
    for child in node.children.borrow().iter() {
        if let NodeData::Element { ref name, .. } = child.data {
            let tag = &*name.local;
            if tag == "h1" {
                // Process h1 through the headings handler chain
                if let Some(result) = handlers.handle(child) {
                    content.push_str(&result.content);
                }
            } else {
                // Recursively search nested elements for h1
                content.push_str(&extract_h1_headings(handlers, child));
            }
        }
    }
    content
}

/// Handler for `<nav>` elements.
/// 
/// Extracts only `<h1>` children (page titles), discards all navigation content.
/// Returns `Some("")` to consume the element and prevent fallback to block_handler.
pub(super) fn nav_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let content = extract_h1_headings(handlers, element.node);
    let content = content.trim();
    
    // Always return Some to consume the element (even if empty)
    // Returning None would fall back to block_handler which would output the nav content
    if content.is_empty() {
        Some("".into())
    } else {
        Some(content.to_string().into())
    }
}
