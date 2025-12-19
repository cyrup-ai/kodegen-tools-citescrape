//! Handler for <aside> elements
//!
//! Provides specialized handling for:
//! - Widget asides (social, cookie notices, ads) - filtered out
//! - Generic aside elements treated as block elements

use super::super::Element;
use super::element_util::is_widget_element;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn aside_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Skip widget elements (social, cookie notices, ads)
    // These are non-content elements that should not appear in markdown output
    if is_widget_element(element.attrs) {
        return None;
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
