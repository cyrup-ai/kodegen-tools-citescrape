//! Handler for <dialog> elements - skip entirely (no markdown equivalent)

use super::super::Element;
use super::{HandlerResult, Handlers};

/// Skip dialog elements - modal UI with no markdown equivalent.
/// 
/// Note: <dialog> is currently in block_handler list in mod.rs but needs
/// explicit skip handler to ensure content is not extracted.
pub(super) fn dialog_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    Some("".into())
}
