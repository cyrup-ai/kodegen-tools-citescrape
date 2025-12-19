use super::super::{
    Element,
    text_util::{JoinOnStringIterator, TrimDocumentWhitespace, concat_strings},
};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn blockquote_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim_start_matches('\n');
    let content = content
        .trim_end_document_whitespace()
        .lines()
        .map(|line| concat_strings!("> ", line))
        .join("\n");
    Some(concat_strings!("\n\n", content, "\n\n").into())
}
