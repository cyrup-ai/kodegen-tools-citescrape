use html5ever::tendril::{Tendril, fmt::UTF8};
use log::warn;
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
            // Zero-copy borrow from Tendril - no allocation
            let borrowed = contents.borrow();
            let text: &str = borrowed.as_ref();

            if is_pre {
                // Handle pre and code blocks
                let result = if parent_tag.is_some_and(|t| t == "pre") {
                    escape_pre_text_if_needed(Cow::Borrowed(text))
                } else {
                    Cow::Borrowed(text)
                };
                buffer.push_str(&result);
            } else {
                // Handle other elements - start with borrowed, allocate only if needed
                let text = escape_if_needed(Cow::Borrowed(text));
                let text = compress_whitespace(&text);

                // Use starts_with/ends_with instead of iterator methods
                if trim_leading_spaces || (text.starts_with(' ') && buffer.ends_with(' ')) {
                    // Trim returns &str slice, push directly to buffer
                    let trimmed = text.trim_start_matches(' ');
                    if !trimmed.is_empty() {
                        buffer.push_str(trimmed);
                    }
                } else if !text.is_empty() {
                    // Cow derefs to &str, no allocation needed
                    buffer.push_str(&text);
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
        NodeData::ProcessingInstruction { ref target, ref contents } => {
            // Processing instructions are rare in HTML5 but can occur in XHTML.
            // In Faithful mode, preserve as HTML-safe representation.
            // In Pure mode, skip silently (no markdown equivalent).
            if handlers.options.translation_mode == TranslationMode::Faithful {
                buffer.push_str(&format!("<!--?{} {}?-->", target, contents));
            }
        }
    }

    markdown_translated
}

/// Combines similar adjacent inline elements in-place using two-pointer compaction.
/// This is O(n) instead of O(n^2) because we avoid Vec::remove().
///
/// The mutable borrow of `node.children` is released when this function returns,
/// making subsequent immutable borrows in the caller safe.
fn combine_similar_children(node: &Rc<Node>) {
    let mut children = node.children.borrow_mut();

    if children.len() <= 1 {
        return; // Nothing to combine
    }

    let original_len = children.len();
    let mut write_idx = 0;

    for read_idx in 1..original_len {
        if let Some(text_to_append) = can_combine(&children[write_idx], &children[read_idx]) {
            let node_at_write = &children[write_idx];
            let children_of_write = node_at_write.children.borrow();

            if let Some(first_child) = children_of_write.first() {
                if let NodeData::Text { contents } = &first_child.data {
                    let mut current_text = contents.borrow_mut();
                    current_text.push_tendril(&text_to_append);
                } else {
                    warn!(
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
                warn!(
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
            write_idx += 1;
            if write_idx != read_idx {
                children.swap(write_idx, read_idx);
            }
        }
    }

    children.truncate(write_idx + 1);
    // Mutable borrow of node.children released here when function returns
}

pub(crate) fn walk_children(
    node: &Rc<Node>,
    buffer: &mut String,
    handlers: &ElementHandlers,
    is_parent_block_element: bool,
    is_pre: bool,
) -> bool {
    // Combine similar adjacent inline elements first.
    // Mutable borrow is contained within this function call and released on return.
    combine_similar_children(node);

    // Safe to take immutable borrow - no explicit drop() needed
    let mut trim_leading_spaces = !is_pre && is_parent_block_element;
    let tag = get_node_tag_name(node);
    let mut markdown_translated = true;

    for child in node.children.borrow().iter() {
        let is_block = get_node_tag_name(child).is_some_and(is_block_element);

        if is_block {
            trim_buffer_end_spaces(buffer);
        }

        let buffer_len = buffer.len();

        markdown_translated &= walk_node(child, buffer, handlers, tag, trim_leading_spaces, is_pre);

        if buffer.len() > buffer_len {
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
        // Take the Tendril value (zero-copy) since n2 is being discarded
        Some(contents2.take())
    } else {
        None
    }
}

/// Normalizes content before adding to buffer by:
/// 1. Collapsing excessive newlines (max 2 consecutive newlines)
/// 2. Collapsing adjacent spaces between inline elements (when not in pre context)
///
/// Uses O(k) byte operations where k is the number of trailing/leading newlines,
/// instead of O(n) character iteration over the entire buffer.
fn normalize_content_for_buffer(
    buffer: &str,
    mut content: String,
    is_pre: bool,
) -> String {
    if buffer.is_empty() {
        return content;
    }

    // O(k) where k is number of trailing newlines - typically 0-3
    // Safe: '\n' is ASCII (0x0A), single byte in UTF-8
    let last_newlines = buffer
        .as_bytes()
        .iter()
        .rev()
        .take_while(|&&b| b == b'\n')
        .count();

    // O(k) where k is number of leading newlines
    let content_newlines = content
        .as_bytes()
        .iter()
        .take_while(|&&b| b == b'\n')
        .count();

    let total_newlines = last_newlines + content_newlines;

    // Collapse excessive newlines (max 2)
    if total_newlines > 2 {
        let to_remove = std::cmp::min(total_newlines - 2, content_newlines);
        // Safe: we're removing ASCII newlines which are 1 byte each
        content.drain(..to_remove);
    }

    // Collapse adjacent spaces between inline elements (not in pre context)
    // O(1) byte access instead of O(n) char iteration
    if !is_pre
        && last_newlines == 0
        && content_newlines == 0
        && buffer.as_bytes().last() == Some(&b' ')
        && content.as_bytes().first() == Some(&b' ')
    {
        content.remove(0);
    }

    content
}

fn trim_buffer_end(buffer: &mut String) {
    // Find the position where document whitespace ends
    let end = buffer
        .char_indices()
        .rev()
        .find(|(_, c)| !matches!(c, '\n' | '\t' | ' '))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);

    if end < buffer.len() {
        buffer.truncate(end);
    }
}

fn trim_buffer_end_spaces(buffer: &mut String) {
    // Find the position where trailing spaces end
    let end = buffer
        .char_indices()
        .rev()
        .find(|(_, c)| *c != ' ')
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);

    if end < buffer.len() {
        buffer.truncate(end);
    }
}

/// Lookup table for body escape characters: \ * _ ` [ ]
/// All are ASCII, so byte-level indexing is safe.
const BODY_ESCAPE_LUT: [bool; 256] = {
    let mut lut = [false; 256];
    lut[b'\\' as usize] = true;
    lut[b'*' as usize] = true;
    lut[b'_' as usize] = true;
    lut[b'`' as usize] = true;
    lut[b'[' as usize] = true;
    lut[b']' as usize] = true;
    lut
};

/// Lookup table for line-start escape characters: = ~ > - + # 0-9
const LINE_START_LUT: [bool; 256] = {
    let mut lut = [false; 256];
    lut[b'=' as usize] = true;
    lut[b'~' as usize] = true;
    lut[b'>' as usize] = true;
    lut[b'-' as usize] = true;
    lut[b'+' as usize] = true;
    lut[b'#' as usize] = true;
    // Digits 0-9
    lut[b'0' as usize] = true;
    lut[b'1' as usize] = true;
    lut[b'2' as usize] = true;
    lut[b'3' as usize] = true;
    lut[b'4' as usize] = true;
    lut[b'5' as usize] = true;
    lut[b'6' as usize] = true;
    lut[b'7' as usize] = true;
    lut[b'8' as usize] = true;
    lut[b'9' as usize] = true;
    lut
};

/// Escapes markdown special characters in text content.
///
/// Cases handled:
/// - Body escapes: `\` `*` `_` `` ` `` `[` `]` -> backslash-prefixed
/// - Line-start `=` `~` `>` -> backslash-prefixed (prevents h1/h2/blockquote)
/// - Line-start `-` `+` followed by space -> backslash-prefixed (prevents list)
/// - Line-start `#` followed by space -> backslash-prefixed (prevents heading)
/// - Line-start `N.` followed by space -> escaped dot (prevents ordered list)
fn escape_if_needed(text: Cow<'_, str>) -> Cow<'_, str> {
    if text.is_empty() {
        return text;
    }

    let bytes = text.as_bytes();
    let first_byte = bytes[0];

    // O(1) checks using lookup tables
    let needs_line_start_escape = LINE_START_LUT[first_byte as usize];
    let needs_body_escape = bytes.iter().any(|&b| BODY_ESCAPE_LUT[b as usize]);

    // Fast path: no escaping needed at all
    if !needs_body_escape && !needs_line_start_escape {
        return super::html_escape::escape_html(text);
    }

    // Slow path: need to escape something
    // Use lazy allocation pattern from compress_whitespace
    let mut result: Option<String> = None;
    let mut last_copy_idx = 0;

    for (byte_idx, ch) in text.char_indices() {
        let escape_str = match ch {
            '\\' => Some("\\\\"),
            '*' => Some("\\*"),
            '_' => Some("\\_"),
            '`' => Some("\\`"),
            '[' => Some("\\["),
            ']' => Some("\\]"),
            _ => None,
        };

        if let Some(escaped) = escape_str {
            // Lazy allocation on first escape using get_or_insert_with
            // Modest allocation: original size + small buffer for escapes
            let r = result.get_or_insert_with(|| String::with_capacity(text.len() + 16));
            // Bulk copy unchanged portion
            r.push_str(&text[last_copy_idx..byte_idx]);
            r.push_str(escaped);
            last_copy_idx = byte_idx + ch.len_utf8();
        }
    }

    // Finalize the escaped string
    let mut escaped = if let Some(mut r) = result {
        // Copy remaining tail
        r.push_str(&text[last_copy_idx..]);
        r
    } else {
        // No body escapes found, but we may need line-start escape
        text.into_owned()
    };

    // Handle line-start escaping with O(1) byte access
    escaped = handle_line_start_escaping(escaped, first_byte);

    super::html_escape::escape_html(escaped.into())
}

/// Handles line-start markdown escaping using O(1) byte access.
#[inline]
fn handle_line_start_escaping(mut escaped: String, first_byte: u8) -> String {
    match first_byte {
        b'=' | b'~' | b'>' => {
            // Always escape these at line start (prevents h1/h2/blockquote)
            escaped.insert(0, '\\');
        }
        b'-' | b'+' => {
            // Only escape if followed by space (prevents unordered list)
            // O(1) byte access instead of O(n) chars().nth(1)
            if escaped.as_bytes().get(1) == Some(&b' ') {
                escaped.insert(0, '\\');
            }
        }
        b'#' => {
            // Escape if it's a valid ATX heading pattern
            if is_markdown_atx_heading(&escaped) {
                escaped.insert(0, '\\');
            }
        }
        b'0'..=b'9' => {
            // Escape the dot in ordered list patterns like "1. "
            if let Some(dot_idx) = index_of_markdown_ordered_item_dot(&escaped) {
                escaped.replace_range(dot_idx..(dot_idx + 1), "\\.");
            }
        }
        _ => {}
    }
    escaped
}

/// Escapes code fence characters at the start of any line within pre text.
/// 
/// Cases:
/// '```'        -> '\```'      // fence at start of text
/// 'Line\n```'  -> 'Line\n\```' // fence at start of line
/// 'inline ```' -> 'inline ```' // NOT escaped (mid-line)
fn escape_pre_text_if_needed<'a>(text: Cow<'a, str>) -> Cow<'a, str> {
    if text.is_empty() {
        return text;
    }
    
    // Fast-path: check if any escaping is needed
    // This avoids allocation when no fence chars appear at line starts
    let needs_escape = text.starts_with('`') 
        || text.starts_with('~')
        || text.contains("\n`")
        || text.contains("\n~");
    
    if !needs_escape {
        // Zero-copy: return unchanged Cow (may be borrowed or owned)
        return text;
    }
    
    // Allocate with ~5% overhead estimate for escape characters
    let mut result = String::with_capacity(text.len() + text.len() / 20);
    let mut at_line_start = true;
    
    for c in text.chars() {
        if at_line_start && (c == '`' || c == '~') {
            // Escape fence-starting characters at line start
            result.push('\\');
        }
        result.push(c);
        at_line_start = c == '\n';
    }
    
    Cow::Owned(result)
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
