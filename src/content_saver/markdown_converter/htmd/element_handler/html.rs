use markup5ever_rcdom::NodeData;

use super::super::{
    Element,
    node_util::get_parent_node,
    options::TranslationMode,
    text_util::concat_strings,
};
use super::{HandlerResult, Handlers};
use super::element_util::serialize_element;

pub(super) fn html_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // In faithful mode, this is markdown translatable only when it's the root
    // of the document.
    let markdown_translatable = if handlers.options().translation_mode == TranslationMode::Faithful
        && let Some(parent) = get_parent_node(element.node)
        && let NodeData::Document = parent.data
    {
        true
    } else {
        // It's always markdown translatable in pure mode.
        handlers.options().translation_mode == TranslationMode::Pure
    };

    if markdown_translatable {
        let content = handlers.walk_children(element.node, element.is_pre).content;
        let content = content.trim_matches('\n');
        Some(concat_strings!("\n\n", content, "\n\n").into())
    } else {
        Some(HandlerResult {
            content: serialize_element(handlers, &element),
            markdown_translated: false,
        })
    }
}
