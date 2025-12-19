use html5ever::tendril::{Tendril, fmt::UTF8};
use log;
use markup5ever_rcdom::{Node, NodeData};
use phf::phf_set;
use std::{borrow::Cow, rc::Rc};

use super::element_handler::ElementHandlers;

use super::{
    node_util::get_node_tag_name,
    options::TranslationMode,
    text_util::{
        compress_whitespace, index_of_markdown_ordered_item_dot,
        is_markdown_atx_heading,
    },
};

pub(crate) fn walk_node(
    node: &Rc<Node>,
    buffer: &mut String,
    handlers: &ElementHandlers,
    parent_tag: Option<&str>,
    trim_leading_spaces: bool,
    is_pre: bool,
) -> bool {
    let mut markdown_translated = true;
    match node.data {
        NodeData::Document => {
            let _ = walk_children(node, buffer, handlers, true, false);
            trim_buffer_end(buffer);
        }

        NodeData::Text { ref contents } => {
            // Append the text in this node to the buffer.
            let text = contents.borrow().to_string();
            if is_pre {
                // Handle pre and code
                let text = if parent_tag.is_some_and(|t| t == "pre") {
                    escape_pre_text_if_needed(text)
                } else {
                    text
                };
                buffer.push_str(&text);
            } else {
                // Handle other elements or texts
                let text = escape_if_needed(Cow::Owned(text));
                let text = compress_whitespace(text.as_ref());

                let to_add = if trim_leading_spaces
                    || (text.chars().next().is_some_and(|ch| ch == ' ')
                        && buffer.chars().last().is_some_and(|ch| ch == ' '))
                {
                    // We can't compress spaces between two text blocks/elements, so we
                    // compress them here by trimming the leading space of current text
                    // content.
                    text.trim_start_matches(' ').to_string()
                } else {
                    text.into_owned()
                };
                if !to_add.is_empty() {
                    buffer.push_str(&to_add);
                }
            }
        }

        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            // Visit this element.
            let tag = &*name.local;
            let is_head = tag == "head";

            let res = handlers.handle(
                node,
                tag,
                &attrs.borrow(),
                true, // Default to true, handler will update
                0,
                is_pre,
            );

            if let Some(res) = res {
                markdown_translated = res.markdown_translated;
                if !res.content.is_empty() || !is_head {
                    let content = normalize_content_for_buffer(buffer, res.content, is_pre);
                    if !content.is_empty() {
                        buffer.push_str(&content);
                    }
                }
            }
        }

        NodeData::Comment { ref contents } => {
            if handlers.options.translation_mode == TranslationMode::Faithful {
                buffer.push_str(&format!("<!--{}-->", contents));
            }
        }
        NodeData::Doctype { .. } => {}
        NodeData::ProcessingInstruction { .. } => unreachable!(),
    }

    markdown_translated
}

pub(crate) fn walk_children(
    node: &Rc<Node>,
    buffer: &mut String,
    handlers: &ElementHandlers,
    is_parent_block_element: bool,
    is_pre: bool,
    // Return value: `markdown_translated`.
) -> bool {
    // Combine similar adjacent blocks using two-pointer compaction.
    // This is O(n) instead of O(n²) because we avoid Vec::remove().
    let mut children = node.children.borrow_mut();
    
    if children.len() <= 1 {
        // Early exit if 0 or 1 children - nothing to combine
        drop(children);
    } else {
        let original_len = children.len();
        let mut write_idx = 0; // Position of last kept element
        
        // Process each element starting from index 1
        for read_idx in 1..original_len {
            // Check if current element can combine with last kept element
            if let Some(text_to_append) = can_combine(&children[write_idx], &children[read_idx]) {
                // Directly modify the write_idx node's text content
                let node_at_write = &children[write_idx];
                let children_of_write = node_at_write.children.borrow();
                
                if let Some(first_child) = children_of_write.first() {
                    if let NodeData::Text { contents } = &first_child.data {
                        // Directly modify the original node's RefCell
                        let mut current_text = contents.borrow_mut();
                        current_text.push_tendril(&text_to_append);
                        // write_idx stays the same for next iteration (node absorbed read_idx's text)
                    } else {
                        // Invariant violation: can_combine said OK but first child isn't Text
                        log::warn!(
                            "DOM walker: can_combine invariant violation at write index {}: first child is not Text node. Skipping combination.",
                            write_idx
                        );
                        drop(children_of_write);
                        write_idx += 1;
                        if write_idx != read_idx {
                            children.swap(write_idx, read_idx);
                        }
                    }
                } else {
                    // Invariant violation: can_combine said OK but no children
                    log::warn!(
                        "DOM walker: can_combine returned Some but node at write index {} has no children. Skipping combination.",
                        write_idx
                    );
                    drop(children_of_write);
                    write_idx += 1;
                    if write_idx != read_idx {
                        children.swap(write_idx, read_idx);
                    }
                }
            } else {
                // Can't combine: keep this element by moving it to write position
                write_idx += 1;
                
                // Swap only if positions are different (optimization)
                if write_idx != read_idx {
                    children.swap(write_idx, read_idx);
                }
            }
        }
        
        // Truncate to final size (write_idx + 1 elements kept)
        children.truncate(write_idx + 1);
        drop(children);
    }

    // Trim leading spaces of the first element/text in block elements (except pre/code)
    let mut trim_leading_spaces = !is_pre && is_parent_block_element;
    let tag = get_node_tag_name(node);
    let mut markdown_translated = true;
    for child in node.children.borrow().iter() {
        let is_block = get_node_tag_name(child).is_some_and(is_block_element);

        if is_block {
            // Trim trailing spaces for the previous element
            trim_buffer_end_spaces(buffer);
        }

        let buffer_len = buffer.len();

        markdown_translated &= walk_node(child, buffer, handlers, tag, trim_leading_spaces, is_pre);

        if buffer.len() > buffer_len {
            // Something was appended, update the flag
            trim_leading_spaces = is_block;
        }
    }

    markdown_translated
}

// Determine if the two nodes are similar, and should therefore be combined. If
// so, return the text of the second node to simplify the combining process.
fn can_combine(n1: &Node, n2: &Node) -> Option<Tendril<UTF8>> {
    // To be combined, both nodes must be elements.
    let NodeData::Element {
        name: name1,
        attrs: attrs1,
        template_contents: template_contents1,
        mathml_annotation_xml_integration_point: mathml_annotation_xml_integration_point1,
    } = &n1.data
    else {
        return None;
    };
    let NodeData::Element {
        name: name2,
        attrs: attrs2,
        template_contents: template_contents2,
        mathml_annotation_xml_integration_point: mathml_annotation_xml_integration_point2,
    } = &n2.data
    else {
        return None;
    };

    // Only combine inline content; block content (for example, one paragraph
    // following another) repetition is expected and should not be combined.
    if is_block_element(&name1.local) {
        return None;
    }

    // Their children must be a single text element.
    let c1 = n1.children.borrow();
    let c2 = n2.children.borrow();
    if c1.len() == 1
        && c2.len() == 1
        && let Some(d1) = c1.first()
        && let Some(d2) = c2.first()
        && let NodeData::Text {
            contents: _contents1,
        } = &d1.data
        && let NodeData::Text {
            contents: contents2,
        } = &d2.data
        // Don't combine adjacent hyperlinks.
        && *name1.local != *"a"
        && (name1 == name2
            // Treat `i` and `em` tags as the same element; likewise for `b` and
            // `strong`.
            || *name1.local == *"i" && *name2.local == *"em"
            || *name1.local == *"em" && *name2.local == *"i"
            || *name1.local == *"b" && *name2.local == *"strong"
            || *name1.local == *"strong" && name2.local == *"b")
        && template_contents1.borrow().is_none()
        && template_contents2.borrow().is_none()
        && attrs1 == attrs2
        && mathml_annotation_xml_integration_point1 == mathml_annotation_xml_integration_point2
    {
        // Clone the Tendril CONTENTS, not the RefCell
        Some(contents2.borrow().clone())
    } else {
        None
    }
}

/// Normalizes content before adding to buffer by:
/// 1. Collapsing excessive newlines (max 2 consecutive newlines)
/// 2. Collapsing adjacent spaces between inline elements (when not in pre context)
fn normalize_content_for_buffer(
    buffer: &str,
    mut content: String,
    is_pre: bool,
) -> String {
    if buffer.is_empty() {
        return content;
    }

    // Check trailing newlines in the buffer directly
    let last_newlines = buffer.chars().rev().take_while(|c| *c == '\n').count();
    let content_newlines = content.chars().take_while(|c| *c == '\n').count();
    let total_newlines = last_newlines + content_newlines;

    // Collapse excessive newlines (max 2)
    if total_newlines > 2 {
        let to_remove = std::cmp::min(total_newlines - 2, content_newlines);
        content.drain(..to_remove);
    }

    // Collapse adjacent spaces between inline elements (not in pre context)
    if !is_pre
        && last_newlines == 0
        && content_newlines == 0
        && buffer.chars().last().is_some_and(|c| c == ' ')
        && content.chars().next().is_some_and(|c| c == ' ')
    {
        content.remove(0);
    }

    content
}

fn trim_buffer_end(buffer: &mut String) {
    // Find the position where document whitespace ends
    let end = buffer.rfind(|c: char| !matches!(c, '\n' | '\t' | ' '))
        .map(|i| i + 1)
        .unwrap_or(0);
    
    if end < buffer.len() {
        buffer.truncate(end);
    }
}

fn trim_buffer_end_spaces(buffer: &mut String) {
    // Find the position where trailing spaces end
    let end = buffer.rfind(|c: char| c != ' ')
        .map(|i| i + 1)
        .unwrap_or(0);
    
    if end < buffer.len() {
        buffer.truncate(end);
    }
}

/// Cases:
/// '\'        -> '\\'
/// '==='      -> '\==='      // h1
/// '---'      -> '\---'      // h2
/// '```'      -> '\```'       // code fence
/// '~~~'      -> '\~~~'       // code fence
/// '# Not h1' -> '\\# Not h1' // markdown heading in html
/// '1. Item'  -> '1\\. Item'  // ordered list item
/// '- Item'   -> '\\- Item'   // unordered list item
/// '+ Item'   -> '\\+ Item'   // unordered list item
/// '> Quote'  -> '\\> Quote'  // quote
fn escape_if_needed(text: Cow<'_, str>) -> Cow<'_, str> {
    let Some(first) = text.chars().next() else {
        return text;
    };

    let mut need_escape = matches!(first, '=' | '~' | '>' | '-' | '+' | '#' | '0'..='9');

    if !need_escape {
        need_escape = text
            .chars()
            .any(|c| c == '\\' || c == '*' || c == '_' || c == '`' || c == '[' || c == ']');
    }

    if !need_escape {
        return super::html_escape::escape_html(text);
    }

    let mut escaped = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '*' => escaped.push_str("\\*"),
            '_' => escaped.push_str("\\_"),
            '`' => escaped.push_str("\\`"),
            '[' => escaped.push_str("\\["),
            ']' => escaped.push_str("\\]"),
            _ => escaped.push(ch),
        }
    }

    match first {
        '=' | '~' | '>' => {
            escaped.insert(0, '\\');
        }
        '-' | '+' => {
            if escaped.chars().nth(1).is_some_and(|ch| ch == ' ') {
                escaped.insert(0, '\\');
            }
        }
        '#' => {
            if is_markdown_atx_heading(&escaped) {
                escaped.insert(0, '\\');
            }
        }
        '0'..='9' => {
            if let Some(dot_idx) = index_of_markdown_ordered_item_dot(&escaped) {
                escaped.replace_range(dot_idx..(dot_idx + 1), "\\.");
            }
        }
        _ => {}
    }

    // Perform the HTML escape after the other escapes, so that the \\
    // characters inserted here don't get escaped again.
    super::html_escape::escape_html(escaped.into())
}

/// Cases:
/// '```' -> '\```' // code fence
/// '~~~' -> '\~~~' // code fence
fn escape_pre_text_if_needed(text: String) -> String {
    let Some(first) = text.chars().next() else {
        return text;
    };
    match first {
        '`' | '~' => {
            let mut text = text;
            text.insert(0, '\\');
            text
        }
        _ => text,
    }
}

// This is taken from the
// [CommonMark spec](https://spec.commonmark.org/0.31.2/#html-blocks).
static BLOCK_ELEMENTS: phf::Set<&'static str> = phf_set! {
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "pre",
    "script",
    "search",
    "section",
    "style",
    "summary",
    "table",
    "tbody",
    "td",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
};

pub(crate) fn is_block_element(tag: &str) -> bool {
    BLOCK_ELEMENTS.contains(tag)
}
