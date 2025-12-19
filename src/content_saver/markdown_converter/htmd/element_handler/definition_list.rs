//! Handler for definition lists: <dl>, <dt>, <dd>
//!
//! Converts to PHP Markdown Extra / Pandoc definition list syntax:
//! ```text
//! Term
//! : Definition
//! ```

use super::super::Element;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<dl>` element - definition list container
///
/// Wraps child content (dt/dd pairs) with proper block spacing.
pub(super) fn dl_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim_matches('\n');
    if content.is_empty() {
        return None;
    }
    
    Some(format!("\n\n{}\n\n", content).into())
}

/// Handle `<dt>` element - definition term
///
/// Outputs term text on its own line (no prefix).
pub(super) fn dt_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    
    // Term on its own line
    Some(format!("\n{}\n", content).into())
}

/// Handle `<dd>` element - definition description
///
/// Outputs definition with `: ` prefix per PHP Markdown Extra syntax.
pub(super) fn dd_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    
    // Description with `: ` prefix (definition list syntax)
    Some(format!(": {}\n", content).into())
}
