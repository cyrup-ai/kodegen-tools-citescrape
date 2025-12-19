//! Handler for <div> elements
//!
//! Provides specialized handling for:
//! - Widget elements (social, cookie notices, ads) - filtered out
//! - `<div class="expressive-code">` wrapper containers (Astro syntax highlighter)
//! - Generic div elements with proper block spacing

use super::super::Element;
use super::element_util::{get_attr, is_widget_element};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn div_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Skip widget elements (social, cookie notices, ads)
    // These are non-content elements that should not appear in markdown output
    if is_widget_element(element.attrs) {
        return None;
    }

    // Check for expressive-code wrapper divs
    // These are the OUTER containers - the inner ec-line divs are handled by preprocessing
    // See: src/content_saver/markdown_converter/html_preprocessing/expressive_code.rs
    if let Some(class) = get_attr(element.attrs, "class") {
        // Handle expressive-code wrapper - just unwrap it, content is already processed
        if class.contains("expressive-code") {
            let content = handlers.walk_children(element.node, element.is_pre).content;
            // Return content directly - the inner <pre><code> will be handled normally
            return Some(content.into());
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
