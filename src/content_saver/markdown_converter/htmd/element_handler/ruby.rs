//! Handler for ruby annotations: <ruby>, <rt>, <rp>
//!
//! Converts CJK ruby annotations to parenthetical format:
//! <ruby>漢字<rt>かんじ</rt></ruby> -> 漢字(かんじ)

use markup5ever_rcdom::NodeData;

use super::super::Element;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<ruby>` element -> base(annotation)
///
/// Iterates children to separate base text from `<rt>` annotations.
/// Skips `<rp>` elements (fallback parentheses for non-ruby browsers).
///
/// # Examples
/// - `<ruby>漢字<rt>かんじ</rt></ruby>` → `漢字(かんじ)`
/// - `<ruby>東京<rp>(</rp><rt>とうきょう</rt><rp>)</rp></ruby>` → `東京(とうきょう)`
pub(super) fn ruby_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    // In faithful mode with attributes, serialize as HTML
    serialize_if_faithful!(handlers, element, 0);

    let mut base_text = String::new();
    let mut annotation_text = String::new();

    // Iterate through children to separate base text from rt annotations
    // Pattern from details.rs lines 28-54
    for child in element.node.children.borrow().iter() {
        match &child.data {
            NodeData::Element { name, .. } => {
                let tag = name.local.as_ref();
                if tag == "rt" {
                    // Collect annotation text from <rt> element
                    let content = handlers.walk_children(child, element.is_pre).content;
                    annotation_text.push_str(content.trim());
                } else if tag != "rp" {
                    // Process non-rp elements as base text
                    let content = handlers.walk_children(child, element.is_pre).content;
                    base_text.push_str(&content);
                }
                // Skip <rp> elements entirely (fallback parentheses)
            }
            NodeData::Text { contents } => {
                // Direct text nodes are base text
                base_text.push_str(&contents.borrow());
            }
            _ => {}
        }
    }

    let base_text = base_text.trim();
    let annotation_text = annotation_text.trim();

    if base_text.is_empty() {
        return None;
    }

    if annotation_text.is_empty() {
        return Some(base_text.to_string().into());
    }

    Some(format!("{}({})", base_text, annotation_text).into())
}

/// Handle `<rt>` element (ruby text) - processed by ruby_handler
///
/// When encountered as a direct child (not via ruby_handler iteration),
/// return empty since the annotation is meaningless without base text.
pub(super) fn rt_handler(
    _handlers: &dyn Handlers,
    _element: Element,
) -> Option<HandlerResult> {
    // Processed by ruby_handler when iterating parent's children
    Some("".into())
}

/// Handle `<rp>` element (ruby parenthesis) - skip entirely
///
/// These are fallback parentheses shown by browsers that don't support ruby.
/// Not needed in markdown output since we add our own parentheses.
pub(super) fn rp_handler(
    _handlers: &dyn Handlers,
    _element: Element,
) -> Option<HandlerResult> {
    // Fallback parentheses for non-ruby browsers; not needed in markdown
    Some("".into())
}
