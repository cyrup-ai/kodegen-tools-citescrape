//! Handler for technical inline elements: <samp>, <var>, <output>
//!
//! All converted to inline code as they represent code-related content.

use super::super::{Element, text_util::concat_strings};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<samp>` element (sample output) -> `inline code`
pub(super) fn samp_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    inline_code_handler(handlers, element)
}

/// Handle `<var>` element (variable) -> `inline code`
pub(super) fn var_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    inline_code_handler(handlers, element)
}

/// Handle `<output>` element (calculation result) -> `inline code`
pub(super) fn output_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    inline_code_handler(handlers, element)
}

/// Common inline code conversion
fn inline_code_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    
    Some(concat_strings!("`", content, "`").into())
}
