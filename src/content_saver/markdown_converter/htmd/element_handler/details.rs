//! Handler for <details> and <summary> elements
//!
//! Converts HTML5 collapsible sections to markdown:
//! - `<summary>` content becomes a level 3 heading (### Summary Text)
//! - Remaining content is preserved as normal markdown blocks
//!
//! This replaces the regex-based DETAILS_RE/SUMMARY_RE patterns from html_cleaning.rs
//! with proper DOM-based processing.

use markup5ever_rcdom::NodeData;

use super::super::Element;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<details>` element - extract content, use summary as heading
///
/// Processes the details element by:
/// 1. Finding any `<summary>` child and converting it to a level 3 heading
/// 2. Processing all other children as normal markdown content
/// 3. Combining with proper block spacing
pub(super) fn details_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);

    let mut result = String::new();

    // Iterate through children to find summary and other content
    // Pattern from table.rs line 39 and list_processing.rs line 212
    for child in element.node.children.borrow().iter() {
        match &child.data {
            NodeData::Element { name, .. } => {
                // Pattern from table.rs line 41: name.local.as_ref()
                let tag_name = name.local.as_ref();

                if tag_name == "summary" {
                    // Extract summary text via walk_children for proper markdown conversion
                    let summary_content = handlers.walk_children(child, element.is_pre).content;
                    let summary_text = summary_content.trim();

                    if !summary_text.is_empty() {
                        result.push_str("\n\n### ");
                        result.push_str(summary_text);
                        result.push_str("\n\n");
                    }
                } else {
                    // Process non-summary element children
                    let child_content = handlers.walk_children(child, element.is_pre).content;
                    result.push_str(&child_content);
                }
            }
            NodeData::Text { contents } => {
                // Handle text nodes directly under details
                // Pattern from list_processing.rs line 215
                let text = contents.borrow().to_string();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    result.push_str(trimmed);
                }
            }
            // Skip comments, processing instructions, etc.
            _ => {}
        }
    }

    // Return with proper block spacing
    let content = result.trim_matches('\n');
    if content.is_empty() {
        return None;
    }

    Some(format!("\n\n{}\n\n", content).into())
}

/// Handle `<summary>` element - when encountered outside details context
///
/// When summary is processed independently (not as part of details handler),
/// format as bold text since it represents a clickable label.
pub(super) fn summary_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);

    let content = handlers.walk_children(element.node, element.is_pre).content;
    let content = content.trim();

    if content.is_empty() {
        return None;
    }

    // As standalone, format as bold text (clickable indicator)
    Some(format!("**{}**", content).into())
}
