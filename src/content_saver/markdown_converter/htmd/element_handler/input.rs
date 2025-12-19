//! Handler for form input elements - skip entirely (no markdown equivalent)

use super::super::Element;
use super::{HandlerResult, Handlers};

/// Skip input/select/textarea elements - interactive form controls.
/// 
/// These are handled separately from <form> because they can appear
/// outside form tags in modern HTML.
pub(super) fn input_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    Some("".into())
}
