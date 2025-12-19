//! Handler for <header> elements
//!
//! Page headers typically contain site logos, navigation, and other non-content
//! elements. However, the page title as <h1> should be preserved.
//! This handler extracts only h1 headings and discards everything else.

use std::rc::Rc;
use markup5ever_rcdom::{Node, NodeData};
use super::super::Element;
use super::{HandlerResult, Handlers};

/// Recursively extract only h1 headings from element children.
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

/// Handler for `<header>` elements.
/// 
/// Extracts only `<h1>` children, discards logos/navigation/other header chrome.
pub(super) fn header_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let content = extract_h1_headings(handlers, element.node);
    let content = content.trim();
    
    if content.is_empty() {
        Some("".into())
    } else {
        Some(content.to_string().into())
    }
}
