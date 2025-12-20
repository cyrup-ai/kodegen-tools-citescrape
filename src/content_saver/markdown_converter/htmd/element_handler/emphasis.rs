use super::super::{Element, text_util::{StripWhitespace, concat_strings}, options::TranslationMode};
use super::{HandlerResult, Handlers, element_util::serialize_element};
use crate::serialize_if_faithful;

pub(super) fn emphasis_handler(
    handlers: &dyn Handlers,
    element: Element,
    marker: &str,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    // Try standard handler approach first
    let mut content = handlers.walk_children(element.node, element.is_pre).content;
    
    // FALLBACK: If walk_children returns empty but element has text content,
    // extract it directly from the DOM. This handles cases where the element
    // hasn't been properly traversed yet (e.g., in list_processing context).
    if content.is_empty() {
        let raw_text = extract_element_text_content(element.node);
        if !raw_text.trim().is_empty() {
            content = raw_text;
        } else {
            return None;  // Truly empty element
        }
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

/// Extract text content directly from element's DOM tree
///
/// Bypasses the handler system for recovery when walk_children() fails.
/// This is a fallback mechanism to ensure text content is never lost due to
/// handler traversal issues.
fn extract_element_text_content(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> String {
    use markup5ever_rcdom::NodeData;
    
    match &node.data {
        NodeData::Text { contents } => contents.borrow().to_string(),
        NodeData::Element { .. } => {
            let mut text = String::new();
            for child in node.children.borrow().iter() {
                text.push_str(&extract_element_text_content(child));
            }
            text
        }
        _ => String::new(),
    }
}
