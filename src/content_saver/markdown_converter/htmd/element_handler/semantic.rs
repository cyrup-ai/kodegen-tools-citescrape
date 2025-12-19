//! Handler for semantic content elements: <abbr>, <cite>, <time>, <dfn>, <address>

use super::super::{Element, text_util::concat_strings};
use super::element_util::get_attr;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<abbr>` element -> TEXT (expansion)
/// Example: <abbr title="HyperText Markup Language">HTML</abbr> -> HTML (HyperText Markup Language)
pub(super) fn abbr_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    
    // Include expansion from title if present
    if let Some(title) = get_attr(element.attrs, "title") {
        return Some(concat_strings!(content, " (", title, ")").into());
    }
    
    Some(content.to_string().into())
}

/// Handle `<cite>` element -> *italic citation*
pub(super) fn cite_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    
    Some(concat_strings!("*", content, "*").into())
}

/// Handle `<time>` element -> content or datetime attribute
pub(super) fn time_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    
    // Prefer visible content, fallback to datetime attribute
    if !content.is_empty() {
        return Some(content.to_string().into());
    }
    
    if let Some(datetime) = get_attr(element.attrs, "datetime") {
        return Some(datetime.into());
    }
    
    None
}

/// Handle `<dfn>` element (definition) -> **bold term**
pub(super) fn dfn_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    
    Some(concat_strings!("**", content, "**").into())
}

/// Handle `<address>` element -> block with line breaks preserved
pub(super) fn address_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim_matches('\n');
    if content.is_empty() {
        return None;
    }
    
    Some(concat_strings!("\n\n", content, "\n\n").into())
}
