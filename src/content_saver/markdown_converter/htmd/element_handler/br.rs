use super::super::{Element, options::BrStyle};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn br_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);

    match handlers.options().br_style {
        BrStyle::TwoSpaces => Some("  \n".into()),
        BrStyle::Backslash => Some("\\\n".into()),
    }
}
