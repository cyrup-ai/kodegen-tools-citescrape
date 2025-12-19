use super::super::{Element, text_util::{StripWhitespace, concat_strings}, options::TranslationMode};
use super::{HandlerResult, Handlers, element_util::serialize_element};
use crate::serialize_if_faithful;

pub(super) fn emphasis_handler(
    handlers: &dyn Handlers,
    element: Element,
    marker: &str,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    let content = handlers.walk_children(element.node, element.is_pre).content;
    if content.is_empty() {
        return None;
    }
    // Note: this is whitespace, NOT document whitespace, per the
    // [Commonmark spec](https://spec.commonmark.org/0.31.2/#emphasis-and-strong-emphasis).
    let (content, leading_whitespace) = content.strip_leading_whitespace();
    let (content, trailing_whitespace) = content.strip_trailing_whitespace();
    if content.is_empty() {
        // Handle whitespace-only emphasis elements mode-aware
        if handlers.options().translation_mode == TranslationMode::Faithful {
            // In Faithful mode, preserve the original HTML structure for round-trip fidelity
            return Some(HandlerResult {
                content: serialize_element(handlers, &element),
                markdown_translated: false,
            });
        }
        
        // In Pure mode, return just the whitespace without emphasis markers
        // (emphasis on whitespace alone is semantically meaningless)
        let ws = concat_strings!(
            leading_whitespace.unwrap_or(""),
            trailing_whitespace.unwrap_or("")
        );
        
        if ws.is_empty() {
            return None;  // Truly empty element with no content at all
        }
        
        return Some(ws.into());
    }
    let content = concat_strings!(
        leading_whitespace.unwrap_or(""),
        marker,
        content,
        marker,
        trailing_whitespace.unwrap_or("")
    );
    Some(content.into())
}
