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

/// Detect if node contains navigation links (multiple <a> elements)
/// Returns (has_links, link_elements)
fn detect_navigation_links(node: &Rc<Node>) -> (bool, Vec<Rc<Node>>) {
    let mut link_elements = Vec::new();
    
    // Strategy 1: Direct <a> children
    for child in node.children.borrow().iter() {
        if let NodeData::Element { ref name, .. } = child.data
            && &*name.local == "a" {
                link_elements.push(child.clone());
            }
    }
    
    // Strategy 2: List-based navigation (<ul>/<ol> containing <li> with <a>)
    if link_elements.is_empty() {
        for child in node.children.borrow().iter() {
            if let NodeData::Element { ref name, .. } = child.data {
                let tag = &*name.local;
                if tag == "ul" || tag == "ol" {
                    // Extract links from list items
                    for li_child in child.children.borrow().iter() {
                        if let NodeData::Element { ref name, .. } = li_child.data
                            && &*name.local == "li" {
                                // Find <a> within <li>
                                for li_content in li_child.children.borrow().iter() {
                                    if let NodeData::Element { ref name, .. } = li_content.data
                                        && &*name.local == "a" {
                                            link_elements.push(li_content.clone());
                                        }
                                }
                            }
                    }
                }
            }
        }
    }
    
    let has_links = link_elements.len() > 1; // Multiple links = navigation menu
    (has_links, link_elements)
}

/// Process navigation links and format with newline separation
fn process_navigation_links(handlers: &dyn Handlers, link_elements: Vec<Rc<Node>>) -> String {
    let mut links = Vec::new();
    
    for link_node in link_elements {
        // Process through the anchor handler
        if let Some(result) = handlers.handle(&link_node) {
            let link_md = result.content.trim();
            if !link_md.is_empty() {
                links.push(link_md.to_string());
            }
        }
    }
    
    if links.is_empty() {
        return String::new();
    }
    
    // Join with newlines for readability
    links.join("\n")
}

/// Handler for `<nav>` elements.
/// 
/// Processes navigation menus with proper spacing between links.
/// Extracts `<h1>` page titles when present.
/// Returns formatted navigation with newline-separated links.
pub(super) fn nav_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let mut content = String::new();
    
    // Phase 1: Extract h1 headings (page titles)
    let h1_content = extract_h1_headings(handlers, element.node);
    if !h1_content.trim().is_empty() {
        content.push_str(&h1_content);
    }
    
    // Phase 2: Detect and process navigation links
    let (has_nav_links, link_elements) = detect_navigation_links(element.node);
    
    if has_nav_links {
        let nav_links = process_navigation_links(handlers, link_elements);
        if !nav_links.is_empty() {
            if !content.is_empty() {
                content.push_str("\n\n"); // Separate h1 from links
            }
            content.push_str(&nav_links);
        }
    }
    
    let content = content.trim();
    
    // Always return Some to consume the element (even if empty)
    // Returning None would fall back to block_handler
    if content.is_empty() {
        Some("".into())
    } else {
        // Wrap in block spacing for proper markdown separation
        Some(format!("\n\n{}\n\n", content).into())
    }
}
