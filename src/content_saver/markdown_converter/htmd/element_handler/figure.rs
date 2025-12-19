//! Handler for figures: <figure>, <figcaption>
//!
//! Converts figure elements to image + italicized caption.
//! The <img> inside figure is handled by the existing img_handler.

use super::super::Element;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<figure>` element - figure container
///
/// Processes children (img + figcaption) with proper block spacing.
/// The img_handler produces `![alt](src)` and figcaption_handler
/// produces `*caption*`.
pub(super) fn figure_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    // Process all children (img and figcaption handlers will format appropriately)
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim_matches('\n');
    if content.is_empty() {
        return None;
    }
    
    Some(format!("\n\n{}\n\n", content).into())
}

/// Handle `<figcaption>` element - figure caption
///
/// Outputs caption as italicized text below the image.
pub(super) fn figcaption_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    
    // Italicize caption text
    Some(format!("\n*{}*\n", content).into())
}
