//! Handler for heading elements (h1-h6)
//!
//! Provides DOM-based filtering of permalink anchors that commonly appear in
//! documentation sites (GitHub, Docusaurus, Starlight, Jekyll, Eleventy, etc.)

use std::rc::Rc;
use markup5ever_rcdom::{Node, NodeData};
use super::super::{Element, text_util::TrimDocumentWhitespace};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;
use super::super::text_util::is_invisible_unicode;

// ============================================================================
// Permalink Anchor Detection
// ============================================================================

/// Check if a node is a permalink anchor that should be skipped.
///
/// Returns true if ANY of these conditions are met:
///
/// 1. **CLASS-BASED**: `<a>` with class containing:
///    - "anchor", "permalink", "header-link", "heading-link", "hash-link"
///
/// 2. **ARIA-HIDDEN**: `<a>` with `aria-hidden="true"`
///    (often used for icon-only permalink anchors)
///
/// 3. **CONTENT-BASED**: `<a>` with text content being ONLY:
///    - "#", "§" (section sign), "¶" (pilcrow)
///    - "Navigate to header" (accessibility text)
fn is_permalink_anchor(node: &Rc<Node>) -> bool {
    let NodeData::Element { ref name, ref attrs, .. } = node.data else {
        return false;
    };
    
    // Only check anchor elements
    if &*name.local != "a" {
        return false;
    }
    
    let attrs = attrs.borrow();
    
    // Check for permalink anchor classes or aria-hidden
    for attr in attrs.iter() {
        let attr_name = &*attr.name.local;
        
        if attr_name == "class" {
            let class_value = attr.value.to_lowercase();
            if class_value.contains("anchor")
                || class_value.contains("permalink")
                || class_value.contains("header-link")
                || class_value.contains("heading-link")
                || class_value.contains("hash-link")
            {
                return true;
            }
        }
        
        // aria-hidden anchors are typically icon-only permalinks
        if attr_name == "aria-hidden" && &*attr.value == "true" {
            return true;
        }
    }
    
    // Check text content for common permalink symbols
    let text = get_text_content(node);
    let text_trimmed = text.trim();
    
    // Exact match for permalink symbols
    if text_trimmed == "#" || text_trimmed == "§" || text_trimmed == "¶" {
        return true;
    }
    
    // Check if anchor text contains only invisible Unicode characters
    // This catches anchors like <a href="#id">​</a> where ​ is U+200B
    let has_only_invisible = !text_trimmed.is_empty() 
        && text_trimmed.chars().all(is_invisible_unicode);
    if has_only_invisible {
        return true;
    }
    
    // "Navigate to header" accessibility text (case-insensitive)
    let text_lower = text_trimmed.to_lowercase();
    // Normalize whitespace for comparison
    let normalized: String = text_lower.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized == "navigate to header" {
        return true;
    }
    
    false
}

/// Check if a node is a heading wrapper div that should be unwrapped.
///
/// Matches wrapper divs used by Starlight/Astro documentation:
/// - `<div class="heading-wrapper">...</div>`
/// - `<div class="sl-heading-wrapper">...</div>`
fn is_heading_wrapper(node: &Rc<Node>) -> bool {
    let NodeData::Element { ref name, ref attrs, .. } = node.data else {
        return false;
    };
    
    if &*name.local != "div" {
        return false;
    }
    
    let attrs = attrs.borrow();
    for attr in attrs.iter() {
        if &*attr.name.local == "class" {
            let class = attr.value.to_lowercase();
            if class.contains("heading-wrapper") {
                return true;
            }
        }
    }
    
    false
}

// ============================================================================
// Text Content Extraction
// ============================================================================

/// Get text content of a node (recursive).
fn get_text_content(node: &Rc<Node>) -> String {
    let mut text = String::new();
    collect_text(node, &mut text);
    text
}

/// Recursively collect text content from a node tree.
///
/// Strips invisible Unicode characters during collection to prevent
/// them from appearing in the final markdown output.
fn collect_text(node: &Rc<Node>, buffer: &mut String) {
    match &node.data {
        NodeData::Text { contents } => {
            // Filter out invisible Unicode characters during text extraction
            let text: String = contents.borrow()
                .chars()
                .filter(|c| !is_invisible_unicode(*c))
                .collect();
            buffer.push_str(&text);
        }
        NodeData::Element { .. } => {
            for child in node.children.borrow().iter() {
                collect_text(child, buffer);
            }
        }
        _ => {}
    }
}

// ============================================================================
// Filtered Child Walking
// ============================================================================

/// Walk heading children, filtering out permalink anchors.
///
/// This replaces the standard `walk_children` call with filtered traversal:
/// 1. Skip permalink anchor elements entirely
/// 2. Unwrap heading wrapper divs (process their children recursively)
/// 3. Process other nodes through normal handlers
fn walk_heading_children(handlers: &dyn Handlers, node: &Rc<Node>) -> String {
    let mut content = String::new();
    
    for child in node.children.borrow().iter() {
        // Skip permalink anchors entirely
        if is_permalink_anchor(child) {
            continue;
        }
        
        // Unwrap heading wrapper divs (process their children instead)
        if is_heading_wrapper(child) {
            content.push_str(&walk_heading_children(handlers, child));
            continue;
        }
        
        // Process normal children through handlers
        if let Some(result) = handlers.handle(child) {
            content.push_str(&result.content);
        }
    }
    
    content
}

// ============================================================================
// Heading Handler
// ============================================================================

/// Handler for h1-h6 heading elements.
///
/// Converts HTML headings to ATX-style Markdown headings:
/// - `<h1>Title</h1>` -> `# Title`
/// - `<h2>Section</h2>` -> `## Section`
///
/// Filters out permalink anchors that commonly appear in documentation sites.
pub(super) fn headings_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    // Safe level extraction - element.tag is "h1", "h2", etc.
    let level = element.tag.chars()
        .nth(1)
        .and_then(|c| c.to_digit(10))
        .unwrap_or(1) as usize;
    
    // Use filtered child walking instead of walk_children
    // This filters out permalink anchors before accumulating content
    let content = walk_heading_children(handlers, element.node);
    let content = content.trim_document_whitespace();
    let content = content.trim();

    if content.is_empty() {
        return None;
    }

    // ATX style headings with proper block separation
    let heading = format!("\n\n{} {}\n\n", "#".repeat(level), content);
    Some(heading.into())
}
