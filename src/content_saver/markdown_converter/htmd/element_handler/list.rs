use super::super::{
    Element,
    node_util::{get_node_tag_name, get_parent_node},
    options::TranslationMode,
    text_util::concat_strings,
};
use super::element_util::serialize_element;
use super::list_processing::process_list;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn list_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Faithful mode validation - preserve HTML when translation isn't clean
    if handlers.options().translation_mode == TranslationMode::Faithful {
        let has_start = element
            .attrs
            .first()
            .is_some_and(|attr| &attr.name.local == "start");
        serialize_if_faithful!(handlers, element, if has_start { 1 } else { 0 });

        if !element.markdown_translated
            || !element.node.children.borrow().iter().all(|node| {
                let tag_name = get_node_tag_name(node);
                // Text nodes (whitespace) have no tag, allow those
                tag_name == Some("li") || tag_name.is_none()
            })
        {
            return Some(HandlerResult {
                content: serialize_element(handlers, &element),
                markdown_translated: false,
            });
        }
    }

    // Use list_processing for BOTH <ul> and <ol>
    let is_ordered = element.tag == "ol";
    let content = process_list(handlers, element.node, is_ordered);

    if content.trim().is_empty() {
        return None;
    }

    // Spacing depends on parent context
    let parent = get_parent_node(element.node);
    let is_parent_li = parent
        .map(|p| get_node_tag_name(&p).is_some_and(|tag| tag == "li"))
        .unwrap_or(false);

    if is_parent_li {
        // Nested list: single newline wrapping
        Some(concat_strings!("\n", content, "\n").into())
    } else {
        // Root list: double newline wrapping (block element)
        Some(concat_strings!("\n\n", content, "\n\n").into())
    }
}
