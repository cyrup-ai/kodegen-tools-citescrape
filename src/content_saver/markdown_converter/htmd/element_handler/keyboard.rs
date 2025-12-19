//! Handler for keyboard input: <kbd>
//!
//! Converts keyboard shortcuts to inline code.
//! Example: <kbd>Ctrl</kbd>+<kbd>C</kbd> -> `Ctrl`+`C`

use super::super::{Element, text_util::concat_strings};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<kbd>` elements -> `inline code`
///
/// # Arguments
/// * `handlers` - Handler context for child walking and options
/// * `element` - The DOM element to convert
///
/// # Returns
/// * `Some(HandlerResult)` with inline code markdown
/// * `None` if content is empty after trimming
pub(super) fn kbd_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    // In faithful mode with attributes, serialize as HTML
    serialize_if_faithful!(handlers, element, 0);

    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }

    // Wrap in single backticks for inline code representation
    Some(concat_strings!("`", content, "`").into())
}
