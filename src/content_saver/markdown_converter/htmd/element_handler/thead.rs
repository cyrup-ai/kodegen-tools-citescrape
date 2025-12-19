use super::super::Element;
use super::{HandlerResult, Handlers};
use super::element_util::handle_or_serialize_by_parent;
use crate::serialize_if_faithful;

pub(super) fn thead_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    // This tag's ability to translate to markdown requires its children to be
    // markdown translatable as well.
    handle_or_serialize_by_parent(
        handlers,
        &element,
        &vec!["table"],
        element.markdown_translated,
    )
}
