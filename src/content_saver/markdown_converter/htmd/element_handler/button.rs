//! Handler for <button> elements - skip entirely (no markdown equivalent)

use super::super::Element;
use super::{HandlerResult, Handlers};

/// Skip button elements - they are interactive UI with no markdown equivalent.
/// 
/// Pattern matches existing form_iframe_handler in mod.rs line 320.
pub(super) fn button_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    Some("".into())
}
