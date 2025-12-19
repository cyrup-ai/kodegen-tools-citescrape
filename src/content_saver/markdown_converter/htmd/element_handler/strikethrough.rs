//! Handler for strikethrough elements: <del>, <s>, <strike>
//!
//! Converts to GFM strikethrough syntax: ~~text~~
//!
//! Whitespace handling follows CommonMark emphasis rules:
//! Strikethrough markers cannot be adjacent to whitespace inside the markers.

use super::super::{Element, text_util::{StripWhitespace, concat_strings}};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<del>`, `<s>`, `<strike>` elements -> ~~strikethrough~~
///
/// # Arguments
/// * `handlers` - Handler context for child walking and options
/// * `element` - The DOM element to convert
///
/// # Returns
/// * `Some(HandlerResult)` with strikethrough markdown
/// * `None` if content is empty or whitespace-only
pub(super) fn strikethrough_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    // In faithful mode with attributes, serialize as HTML to preserve fidelity
    // The 0 means: allow 0 attributes before falling back to HTML serialization
    serialize_if_faithful!(handlers, element, 0);

    let content = handlers.walk_children(element.node, element.is_pre).content;
    if content.is_empty() {
        return None;
    }

    // Handle whitespace per CommonMark emphasis rules:
    // "A left-flanking delimiter run is not part of a ... strikethrough if
    // any of the following conditions holds: ... followed by a whitespace character"
    // Move leading/trailing whitespace OUTSIDE the ~~ markers
    let (content, leading_ws) = content.strip_leading_whitespace();
    let (content, trailing_ws) = content.strip_trailing_whitespace();
    if content.is_empty() {
        return None;
    }

    let md = concat_strings!(
        leading_ws.unwrap_or(""),
        "~~",
        content,
        "~~",
        trailing_ws.unwrap_or("")
    );
    Some(md.into())
}
