//! Handler for highlighted/marked text: <mark>
//!
//! Converts to bold as a universal fallback since ==highlight== syntax
//! is not supported in CommonMark or GFM.

use super::super::Element;
use super::{HandlerResult, Handlers};
use super::emphasis::emphasis_handler;

/// Handle `<mark>` elements -> **bold** (highlight fallback)
///
/// Delegates to emphasis_handler with "**" marker.
/// DO NOT call serialize_if_faithful! here - emphasis_handler handles it.
///
/// # Arguments
/// * `handlers` - Handler context passed through to emphasis_handler
/// * `element` - The DOM element to convert
///
/// # Returns
/// * `Some(HandlerResult)` with bold markdown from emphasis_handler
/// * `None` if content is empty
pub(super) fn mark_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    // Delegate to emphasis_handler with bold marker
    // emphasis_handler handles serialize_if_faithful! internally
    emphasis_handler(handlers, element, "**")
}
