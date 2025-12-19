use super::super::Element;
use super::{HandlerResult, Handlers};
use super::element_util::handle_or_serialize_by_parent;
use crate::serialize_if_faithful;

pub(super) fn caption_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    handle_or_serialize_by_parent(
        handlers,
        &element,
        &vec!["table"],
        element.markdown_translated,
    )
}
