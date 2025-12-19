use super::super::{Element, text_util::TrimDocumentWhitespace};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn headings_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    // Safe level extraction - no unwrap()!
    // element.tag is "h1", "h2", etc. - extract digit at position 1
    let level = element.tag.chars()
        .nth(1)
        .and_then(|c| c.to_digit(10))
        .unwrap_or(1) as usize;  // Default to h1 if parsing fails
    
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim_document_whitespace();
    let content = content.trim();

    if content.is_empty() {
        return None;
    }

    // ALWAYS ATX style - no Setext ever
    // Use \n\n before and after for proper block separation
    let heading = format!("\n\n{} {}\n\n", "#".repeat(level), content);
    Some(heading.into())
}
