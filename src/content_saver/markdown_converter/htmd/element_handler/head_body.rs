use super::super::Element;
use super::{HandlerResult, Handlers};
use super::element_util::handle_or_serialize_by_parent;

pub(super) fn head_body_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    handle_or_serialize_by_parent(handlers, &element, &vec!["html"], true)
}
