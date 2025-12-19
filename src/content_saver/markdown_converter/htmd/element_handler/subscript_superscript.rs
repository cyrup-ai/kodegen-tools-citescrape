//! Handler for subscript/superscript: <sub>, <sup>
//!
//! These elements have no universal markdown equivalent.
//! GitHub Flavored Markdown supports raw HTML, so we pass through.

use super::super::Element;
use super::element_util::serialize_element;
use super::{HandlerResult, Handlers};

/// Handle `<sub>` element -> preserve as HTML
///
/// Subscript has no markdown equivalent. Serialize to HTML for GFM compatibility.
/// Example: H<sub>2</sub>O -> H<sub>2</sub>O (preserved)
pub(super) fn sub_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    Some(HandlerResult {
        content: serialize_element(handlers, &element),
        markdown_translated: false,
    })
}

/// Handle `<sup>` element -> preserve as HTML
///
/// Superscript has no markdown equivalent. Serialize to HTML for GFM compatibility.
/// Example: E=mc<sup>2</sup> -> E=mc<sup>2</sup> (preserved)
pub(super) fn sup_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    Some(HandlerResult {
        content: serialize_element(handlers, &element),
        markdown_translated: false,
    })
}
