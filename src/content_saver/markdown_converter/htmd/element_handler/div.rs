//! Handler for <div> elements
//!
//! Provides specialized handling for:
//! - Widget elements (social, cookie notices, ads, toolbars) - filtered out
//! - `<div class="expressive-code">` - Astro syntax highlighter (full extraction)
//! - Generic div elements with proper block spacing

use std::rc::Rc;
use markup5ever_rcdom::{Node, NodeData};

use super::super::Element;
use super::super::node_util::{get_parent_node, get_node_tag_name};
use super::element_util::{get_attr, is_widget_element_with_context, extract_raw_text, detect_and_format_admonition};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn div_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Get parent tag name for context-aware filtering
    let parent_node = get_parent_node(element.node);
    let parent_tag = parent_node.as_ref()
        .and_then(|parent| get_node_tag_name(parent));
    
    // Skip widget elements (but preserve accessibility in table contexts)
    if is_widget_element_with_context(element.attrs, parent_tag) {
        return Some("".into());
    }

    // Check for admonition blocks BEFORE other special handling
    if let Some(admonition) = detect_and_format_admonition(handlers, &element) {
        return Some(admonition);
    }

    // Check for expressive-code wrapper divs
    if let Some(class) = get_attr(element.attrs, "class") {
        if class.contains("expressive-code") {
            return handle_expressive_code(element);
        }
        
        // Handle standalone ec-line divs (outside expressive-code context)
        if class.contains("ec-line") {
            let text = extract_raw_text(element.node);
            return Some(text.into());
        }
    }

    // Faithful mode: serialize as HTML for non-special divs
    serialize_if_faithful!(handlers, element, 0);

    // Default: treat as block element with proper spacing
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim_matches('\n');

    if content.is_empty() {
        return None;
    }

    Some(format!("\n\n{}\n\n", content).into())
}

/// Handle Expressive Code blocks (Astro's syntax highlighter)
///
/// Expressive Code generates complex nested HTML:
/// ```html
/// <div class="expressive-code">
///   <pre data-language="rust">
///     <code>
///       <div class="ec-line"><div class="code">line 1</div></div>
///       <div class="ec-line"><div class="code">line 2</div></div>
///     </code>
///   </pre>
///   <button data-code="...">Copy</button>
/// </div>
/// ```
///
/// This function:
/// 1. Extracts language from `pre[data-language]`
/// 2. Finds all `.ec-line` elements and extracts their text
/// 3. Returns a proper fenced code block
fn handle_expressive_code(element: Element) -> Option<HandlerResult> {
    // Extract language from pre[data-language]
    let language = find_pre_language(element.node);
    
    // Extract code lines from .ec-line elements
    let code_lines = extract_ec_lines(element.node);
    
    if !code_lines.is_empty() {
        let code_text = code_lines.join("\n");
        let lang = language.as_deref().unwrap_or("");
        
        // Return as fenced code block with proper spacing
        return Some(format!("\n\n```{}\n{}\n```\n\n", lang, code_text).into());
    }
    
    // Fallback: if no .ec-line found, return None to let normal processing handle it
    None
}

/// Find `pre[data-language]` attribute in node tree
/// 
/// Recursively searches for a <pre> element with data-language attribute.
fn find_pre_language(node: &Rc<Node>) -> Option<String> {
    if let NodeData::Element { name, attrs, .. } = &node.data
        && name.local.as_ref() == "pre"
    {
        for attr in attrs.borrow().iter() {
            if attr.name.local.as_ref() == "data-language" {
                return Some(attr.value.to_string());
            }
        }
    }
    
    // Recursively search children
    for child in node.children.borrow().iter() {
        if let Some(lang) = find_pre_language(child) {
            return Some(lang);
        }
    }
    
    None
}

/// Extract text content from all `.ec-line` elements
fn extract_ec_lines(node: &Rc<Node>) -> Vec<String> {
    let mut lines = Vec::new();
    collect_ec_lines(node, &mut lines);
    lines
}

/// Recursively collect text from .ec-line elements
fn collect_ec_lines(node: &Rc<Node>, lines: &mut Vec<String>) {
    if let NodeData::Element { name, attrs, .. } = &node.data {
        // Check if this is an ec-line element
        if name.local.as_ref() == "div" {
            for attr in attrs.borrow().iter() {
                if attr.name.local.as_ref() == "class" && attr.value.contains("ec-line") {
                    // Extract text content from this line using existing utility
                    let text = extract_raw_text(node);
                    lines.push(text);
                    return; // Don't recurse into children (already extracted)
                }
            }
        }
    }
    
    // Recurse into children
    for child in node.children.borrow().iter() {
        collect_ec_lines(child, lines);
    }
}
