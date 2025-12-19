use std::rc::Rc;

use html5ever::Attribute;
use markup5ever_rcdom::{Node, NodeData};

use super::super::{
    Element,
    node_util::{get_node_tag_name, get_parent_node},
    options::{CodeBlockFence, CodeBlockStyle, TranslationMode},
    text_util::{JoinOnStringIterator, TrimDocumentWhitespace, concat_strings},
};
use super::{HandlerResult, Handlers};
use super::element_util::{extract_raw_text, serialize_element};
use super::language_inference::{
    extract_language_from_class,
    infer_language_from_content,
    validate_html_language,
};
use crate::serialize_if_faithful;

pub(super) fn code_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // In faithful mode, all children of a code tag must be text to translate
    // as markdown.
    if handlers.options().translation_mode == TranslationMode::Faithful
        && !element
            .node
            .children
            .borrow()
            .iter()
            .all(|node| matches!(node.data, NodeData::Text { .. }))
    {
        return Some(HandlerResult {
            content: serialize_element(handlers, &element),
            markdown_translated: false,
        });
    }

    // Determine the type: inline code or a code block.
    let parent_node = get_parent_node(element.node);
    let is_code_block = parent_node
        .as_ref()
        .map(|parent| get_node_tag_name(parent).is_some_and(|t| t == "pre"))
        .unwrap_or(false);
    if is_code_block {
        handle_code_block(handlers, element, &parent_node.unwrap())
    } else {
        handle_inline_code(handlers, element)
    }
}

fn handle_code_block(
    handlers: &dyn Handlers,
    element: Element,
    parent: &Rc<Node>,
) -> Option<HandlerResult> {
    // USE extract_raw_text() instead of handlers.walk_children()
    // walk_children() collapses whitespace - extract_raw_text() preserves it!
    let raw_content = extract_raw_text(element.node);
    let content = raw_content.trim();

    // Skip empty code blocks
    if content.is_empty() {
        return None;
    }

    if handlers.options().code_block_style == CodeBlockStyle::Fenced {
        let fence = if handlers.options().code_block_fence == CodeBlockFence::Tildes {
            get_code_fence_marker("~", content)
        } else {
            get_code_fence_marker("`", content)
        };

        // Step 1: Try to get language from HTML class attributes
        let language = find_language_from_attrs(element.attrs).or_else(|| {
            if let NodeData::Element { ref attrs, .. } = parent.data {
                find_language_from_attrs(&attrs.borrow())
            } else {
                None
            }
        });

        // Step 2: Validate HTML hint against actual content
        // Rejects mismatches like "typescript" on Rust panic output
        let language = language.filter(|lang| validate_html_language(lang, content));

        // Step 3: Fallback to content-based inference if no valid hint
        let language = language.or_else(|| infer_language_from_content(content));

        serialize_if_faithful!(handlers, element, if language.is_none() { 0 } else { 1 });

        let mut result = String::from(&fence);
        if let Some(ref lang) = language {
            result.push_str(lang);
        }
        result.push('\n');
        result.push_str(content);
        result.push('\n');
        result.push_str(&fence);
        Some(result.into())
    } else {
        serialize_if_faithful!(handlers, element, 0);
        let code = content
            .lines()
            .map(|line| concat_strings!("    ", line))
            .join("\n");
        Some(code.into())
    }
}

/// Maximum fence length we'll ever generate.
/// Content with 10+ consecutive backticks is extremely rare.
/// Early termination at this threshold provides massive speedup for large files.
const MAX_FENCE_LEN: usize = 10;

fn get_code_fence_marker(symbol: &str, content: &str) -> String {
    let symbol_char = symbol.chars().next().unwrap_or('`');

    let mut max_consecutive: usize = 0;
    let mut current: usize = 0;

    for c in content.chars() {
        if c == symbol_char {
            current += 1;
            if current > max_consecutive {
                max_consecutive = current;
                // Early termination - no need to scan further
                if max_consecutive >= MAX_FENCE_LEN {
                    break;
                }
            }
        } else {
            current = 0;
        }
    }

    // Minimum 3 (markdown standard), maximum MAX_FENCE_LEN, one more than found
    let fence_len = (max_consecutive + 1).clamp(3, MAX_FENCE_LEN);
    symbol.repeat(fence_len)
}

/// Enhanced language extraction from class attribute
/// Supports: language-X, lang-X, hljs-X, brush:X patterns
fn find_language_from_attrs(attrs: &[Attribute]) -> Option<String> {
    attrs.iter()
        .find(|attr| &attr.name.local == "class")
        .and_then(|attr| extract_language_from_class(&attr.value))
}

fn handle_inline_code(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    // Case: <code>There is a literal backtick (`) here</code>
    //   to: ``There is a literal backtick (`) here``
    let mut use_double_backticks = false;
    // Case: <code>`starting with a backtick</code>
    //   to: `` `starting with a backtick ``
    let mut surround_with_spaces = false;
    let content = handlers.walk_children(element.node, element.is_pre).content;
    let chars = content.chars().collect::<Vec<char>>();
    let len = chars.len();
    for (idx, c) in chars.iter().enumerate() {
        if c == &'`' {
            let prev = if idx > 0 { chars[idx - 1] } else { '\0' };
            let next = if idx < len - 1 { chars[idx + 1] } else { '\0' };
            if prev != '`' && next != '`' {
                use_double_backticks = true;
                surround_with_spaces = idx == 0;
                break;
            }
        }
    }
    let content = if handlers.options().preformatted_code {
        handle_preformatted_code(&content)
    } else {
        content.trim_document_whitespace().to_string()
    };
    if use_double_backticks {
        if surround_with_spaces {
            Some(concat_strings!("`` ", content, " ``").into())
        } else {
            Some(concat_strings!("``", content, "``").into())
        }
    } else {
        Some(concat_strings!("`", content, "`").into())
    }
}

/// Newlines become spaces (+ an extra space if not in the middle of the code)
fn handle_preformatted_code(code: &str) -> String {
    let mut result = String::new();
    let mut is_prev_ch_new_line = false;
    let mut in_middle = false;
    for ch in code.chars() {
        if ch == '\n' {
            result.push(' ');
            is_prev_ch_new_line = true;
        } else {
            if is_prev_ch_new_line && !in_middle {
                result.push(' ');
            }
            result.push(ch);
            is_prev_ch_new_line = false;
            in_middle = true;
        }
    }
    if is_prev_ch_new_line {
        result.push(' ');
    }
    result
}
