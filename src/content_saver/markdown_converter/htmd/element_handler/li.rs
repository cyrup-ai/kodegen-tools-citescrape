use super::super::{
    Element,
    node_util::{get_node_tag_name, get_parent_node},
    options::BulletListMarker,
    text_util::{TrimDocumentWhitespace, concat_strings, indent_text_except_first_line},
};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn list_item_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    let content = handlers
        .walk_children(element.node, element.is_pre)
        .content
        .trim_start_document_whitespace()
        .to_string();

    let ul_li = |content: &str| {
        let marker = if handlers.options().bullet_list_marker == BulletListMarker::Asterisk {
            "*"
        } else {
            "-"
        };
        let spacing = " ".repeat(handlers.options().ul_bullet_spacing.into());
        let indented = indent_text_except_first_line(content, marker.len() + spacing.len(), true);

        Some(concat_strings!("\n", marker, spacing, indented).into())
    };

    let ol_li = |content: &str| {
        // Marker will be added in the ol handler
        Some(concat_strings!("\n", content, "\n").into())
    };

    if let Some(parent) = get_parent_node(element.node)
        && let Some(parent_tag_name) = get_node_tag_name(&parent)
        && parent_tag_name == "ol"
    {
        ol_li(&content)
    } else {
        ul_li(&content)
    }
}
