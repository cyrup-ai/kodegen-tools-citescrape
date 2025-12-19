use super::super::{
    Element,
    dom_walker::is_block_element,
    node_util::parent_tag_name_equals,
    options::TranslationMode,
    text_util::concat_strings,
};
use super::{HandlerResult, Handlers};
use html5ever::Attribute;
use html5ever::serialize::{HtmlSerializer, SerializeOpts, Serializer, TraversalScope, serialize};

use markup5ever_rcdom::{NodeData, SerializableHandle};
use std::io::{self, Write};

// A handler for tags whose only criteria (for faithful translation) is the tag
// name of the parent.
pub(super) fn handle_or_serialize_by_parent(
    handlers: &dyn Handlers,
    // The element to check.
    element: &Element,
    // A list of allowable tag names for this element's parent.
    tag_names: &Vec<&str>,
    // The value for `markdown_translate` to pass if this tag is markdown translatable.
    markdown_translated: bool,
) -> Option<HandlerResult> {
    // In faithful mode, fall back to HTML when this element's parent tag is not
    // in `tag_names` (e.g., `<tbody>` outside `<table>`, `<td>` outside `<tr>`, etc.).
    if handlers.options().translation_mode == TranslationMode::Faithful
        && !parent_tag_name_equals(element.node, tag_names)
    {
        Some(HandlerResult {
            content: serialize_element(handlers, element),
            markdown_translated: false,
        })
    } else {
        let content = handlers.walk_children(element.node, element.is_pre).content;
        let content = content.trim_matches('\n');
        Some(HandlerResult {
            content: concat_strings!("\n\n", content, "\n\n"),
            markdown_translated,
        })
    }
}

// Given a node (which must be an element), serialize it (transform it back
// to HTML).
pub(crate) fn serialize_element(handlers: &dyn Handlers, element: &Element) -> String {
    let f = || -> io::Result<String> {
        let so = SerializeOpts {
            traversal_scope: TraversalScope::IncludeNode,
            ..Default::default()
        };
        let mut bytes = vec![];
        // If this is a block element, then serialize it and all its children.
        // Otherwise, serialize just this element, but use the current contents in
        // the place of children. This follows the Commonmark spec: [HTML
        // blocks](https://spec.commonmark.org/0.31.2/#html-blocks) contain only
        // HTML, not Markdown, while [raw HTML
        // inlines](https://spec.commonmark.org/0.31.2/#raw-html) contain Markdown.
        if !is_block_element(element.tag) {
            // Write this element's start tag.
            let NodeData::Element { name, attrs, .. } = &element.node.data else {
                return Err(io::Error::other("Not an element.".to_string()));
            };
            let mut ser = HtmlSerializer::new(&mut bytes, so.clone());
            ser.start_elem(
                name.clone(),
                attrs.borrow().iter().map(|at| (&at.name, &at.value[..])),
            )?;
            // Write out the contents, without escaping them. The standard serialization process escapes the contents, hence this manual approach.
            ser.writer
                .write_all(handlers.walk_children(element.node, element.is_pre).content.as_bytes())?;
            // Write the end tag, if needed (HtmlSerializer logic will automatically omit this).
            ser.end_elem(name.clone())?;

            String::from_utf8(bytes).map_err(io::Error::other)
        } else {
            let sh: SerializableHandle = SerializableHandle::from(element.node.clone());
            serialize(&mut bytes, &sh, so)?;
            let s = String::from_utf8(bytes).map_err(io::Error::other)?;
            // We must avoid consecutive newlines in HTML blocks, since this
            // terminates the block per the CommonMark spec. Therefore, this
            // code replaces instances of two or more newlines with a single
            // newline, followed by escaped newlines. This is a hand-coded
            // version of the following regex:
            //
            // ```Rust
            // Regex::new(r#"(\r?\n\s*)(\r?\n\s*)"#).unwrap())
            //  .replace_all(&s, |caps: &Captures| {
            //      caps[1].to_string()
            //      + &(caps[2].replace("\r", "&#13;").replace("\n", "&#10;"))
            //  })
            // ```
            //
            // 1.  If the next character is an \\r or \\n, output it.
            // 2.  If the previous character was a \\r and the next
            //     character isn't a \\n, restart. Otherwise, output the
            //     \\n.
            // 3.  If the next character is whitespace but not \\n or \\r,
            //     output it then repeat this step.
            // 4.  If the next character is a \\r and the peeked following
            //     character isn't an \\n, output the \\r and restart.
            //     Otherwise, output an encoded \\r.
            // 5.  If the peeked next character is a \\n, output an encoded
            //     \\n. Otherwise, restart.
            // 6.  If the next character is whitespace but not \\n or \\r,
            //     output it then repeat this step. Otherwise, restart.
            //
            // Replace instances of two or more newlines with a newline
            // followed by escaped newlines
            let mut result = String::with_capacity(s.len());
            let mut chars = s.chars().peekable();

            while let Some(c) = chars.next() {
                // Step 1.
                if c == '\r' || c == '\n' {
                    result.push(c);

                    // Step 2.
                    if c == '\r' {
                        if chars.peek() == Some(&'\n') {
                            result.push(chars.next().unwrap());
                        } else {
                            continue;
                        }
                    }

                    // Step 3: Skip any whitespace after the newline.
                    while let Some(&next) = chars.peek() {
                        if next.is_whitespace() && next != '\r' && next != '\n' {
                            result.push(next);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    // Step 4.
                    if let Some(c) = chars.next() {
                        if c == '\r' || c == '\n' {
                            if c == '\r' {
                                if chars.peek() == Some(&'\n') {
                                    chars.next();
                                    result.push_str("&#13;&#10;");
                                } else {
                                    // Step 6.
                                    result.push('\r');
                                    continue;
                                }
                            } else {
                                result.push_str("&#10;");
                            }

                            // Step 6.
                            while let Some(&next) = chars.peek() {
                                if next.is_whitespace() && next != '\r' && next != '\n' {
                                    result.push(next);
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                        } else {
                            result.push(c);
                        }
                    }
                } else {
                    result.push(c);
                }
            }
            Ok(concat_strings!("\n\n", result, "\n\n"))
        }
    };
    match f() {
        Ok(s) => s,
        Err(err) => err.to_string(),
    }
}

// When in faithful translation mode, return an HTML translation if this element
// has more than the allowed number of attributes.
#[macro_export]
macro_rules! serialize_if_faithful {
    (
        // The handlers to use for serialization.
        $handlers: expr,
        // The element to translate.
        $element: expr,
        // The maximum number of attributes allowed for this element. Supply
        // -1 to serialize in faithful mode, even with no attributes.
        $num_attrs_allowed: expr
    ) => {
        if $handlers.options().translation_mode == $crate::content_saver::markdown_converter::htmd::options::TranslationMode::Faithful
            && $element.attrs.len() as i64 > $num_attrs_allowed
        {
            return Some($crate::content_saver::markdown_converter::htmd::element_handler::HandlerResult {
                content: $crate::content_saver::markdown_converter::htmd::element_handler::element_util::serialize_element(
                    $handlers, &$element,
                ),
                // This was translated using HTML, not Markdown.
                markdown_translated: false,
            });
        }
    };
}


/// Check if character is in CJK Unicode ranges (Chinese, Japanese, Korean)
/// 
/// Covers: CJK Unified Ideographs, Extensions A-D, Compatibility Ideographs,
/// Hiragana, Katakana, Hangul Syllables and Jamo
#[inline]
fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'   |  // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}'   |  // CJK Unified Ideographs Extension A
        '\u{20000}'..='\u{2A6DF}' |  // CJK Unified Ideographs Extension B
        '\u{2A700}'..='\u{2B73F}' |  // CJK Unified Ideographs Extension C
        '\u{2B740}'..='\u{2B81F}' |  // CJK Unified Ideographs Extension D
        '\u{F900}'..='\u{FAFF}'   |  // CJK Compatibility Ideographs
        '\u{3040}'..='\u{309F}'   |  // Hiragana
        '\u{30A0}'..='\u{30FF}'   |  // Katakana
        '\u{31F0}'..='\u{31FF}'   |  // Katakana Phonetic Extensions
        '\u{AC00}'..='\u{D7AF}'   |  // Hangul Syllables
        '\u{1100}'..='\u{11FF}'   |  // Hangul Jamo
        '\u{3130}'..='\u{318F}'      // Hangul Compatibility Jamo
    )
}

/// Check if character is CJK punctuation
/// 
/// Covers: CJK Symbols and Punctuation block (U+3000-U+303F) and
/// Halfwidth/Fullwidth Forms (U+FF00-U+FFEF)
#[inline]
fn is_cjk_punctuation(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{303F}' |  // CJK Symbols and Punctuation
        '\u{FF00}'..='\u{FFEF}'    // Halfwidth and Fullwidth Forms
    )
}

/// Check if character is Latin alphanumeric (ASCII + extended Latin)
#[inline]
fn is_latin_alphanum(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, 'À'..='ÿ')
}

/// Determine if a space is needed between two adjacent characters
/// 
/// Returns false (no space needed) for:
/// - Either character is whitespace
/// - Both characters are CJK
/// - CJK character adjacent to CJK punctuation
/// - Punctuation pairs (handled by is_punctuation_pair)
/// - Latin adjacent to CJK (code extraction context - no space preferred)
/// 
/// Returns true (space needed) for:
/// - Latin word boundaries (both chars are Latin alphanumeric)
#[inline]
fn needs_space_between(prev: char, next: char) -> bool {
    // No space if either side is whitespace
    if prev.is_whitespace() || next.is_whitespace() {
        return false;
    }
    
    // No space between CJK characters
    if is_cjk_char(prev) && is_cjk_char(next) {
        return false;
    }
    
    // No space between CJK and CJK punctuation
    if (is_cjk_char(prev) && is_cjk_punctuation(next)) ||
       (is_cjk_punctuation(prev) && is_cjk_char(next)) {
        return false;
    }
    
    // No space for standard punctuation pairs
    if is_punctuation_pair(prev, next) {
        return false;
    }
    
    // No space between Latin and CJK (code extraction context)
    // This prevents "Hello世界" from becoming "Hello 世界"
    if (is_latin_alphanum(prev) && is_cjk_char(next)) ||
       (is_cjk_char(prev) && is_latin_alphanum(next)) {
        return false;
    }
    
    // Space needed between Latin word characters
    if is_latin_alphanum(prev) && is_latin_alphanum(next) {
        return true;
    }
    
    // Default: no space for other cases (symbols, numbers, etc.)
    false
}

/// Extract raw text content from a node tree, preserving all whitespace
/// and adding intelligent spacing between inline elements
pub fn extract_raw_text(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> String {
    use markup5ever_rcdom::NodeData;

    let mut text = String::new();

    match &node.data {
        NodeData::Text { contents } => {
            let content = contents.borrow();
            // Debug logging for troubleshooting
            if !content.is_empty() {
                tracing::trace!("extract_raw_text: Found text node: {:?}", &content[..content.len().min(50)]);
            }
            // Preserve text exactly as-is (including angle brackets from decoded entities)
            text.push_str(&content);
        }
        NodeData::Element { name, .. } => {
            // Debug logging for element traversal
            tracing::trace!("extract_raw_text: Processing element: {}", &name.local);
            
            // Recursively process all children with intelligent spacing
            for (i, child) in node.children.borrow().iter().enumerate() {
                let child_text = extract_raw_text(child);
                
                // Add appropriate separator between siblings
                if i > 0 && !child_text.is_empty() && !text.is_empty() {
                    // Check if current child is a block element (div, p, etc.)
                    let curr_is_block = matches!(&child.data, 
                        NodeData::Element { name, .. } if is_block_element(&name.local));
                    
                    if curr_is_block {
                        // Block elements should be on new lines
                        // Only add newline if text doesn't already end with one
                        if !text.ends_with('\n') {
                            text.push('\n');
                        }
                    } else {
                        // Inline elements: add space if needed
                        let last_char = text.chars().last();
                        let first_char = child_text.chars().next();
                        
                        if let (Some(last), Some(first)) = (last_char, first_char)
                            && needs_space_between(last, first)
                        {
                            text.push(' ');
                        }
                    }
                }
                
                text.push_str(&child_text);
            }
        }
        NodeData::Document | NodeData::Doctype { .. } => {
            // Recursively process all children with intelligent spacing
            for (i, child) in node.children.borrow().iter().enumerate() {
                let child_text = extract_raw_text(child);
                
                // Add appropriate separator between siblings
                if i > 0 && !child_text.is_empty() && !text.is_empty() {
                    // Check if current child is a block element (div, p, etc.)
                    let curr_is_block = matches!(&child.data, 
                        NodeData::Element { name, .. } if is_block_element(&name.local));
                    
                    if curr_is_block {
                        // Block elements should be on new lines
                        // Only add newline if text doesn't already end with one
                        if !text.ends_with('\n') {
                            text.push('\n');
                        }
                    } else {
                        // Inline elements: add space if needed
                        let last_char = text.chars().last();
                        let first_char = child_text.chars().next();
                        
                        if let (Some(last), Some(first)) = (last_char, first_char)
                            && needs_space_between(last, first)
                        {
                            text.push(' ');
                        }
                    }
                }
                
                text.push_str(&child_text);
            }
        }
        NodeData::Comment { .. } | NodeData::ProcessingInstruction { .. } => {
            // Skip comments and processing instructions
        }
    }

    text
}

/// Check if two characters form a punctuation pair that shouldn't have space between them
#[inline]
fn is_punctuation_pair(prev: char, next: char) -> bool {
    match (prev, next) {
        // Path separators
        ('/', _) | (_, '/') => true,
        // Brackets and parens - no space inside
        ('(', _) | (_, ')') => true,
        ('[', _) | (_, ']') => true,
        ('{', _) | (_, '}') => true,
        ('<', _) | (_, '>') => true,
        // Quotes - no space inside
        ('"', _) | (_, '"') => true,
        ('\'', _) | (_, '\'') => true,
        ('`', _) | (_, '`') => true,
        // Operators that should be adjacent in code
        ('=', _) | (_, '=') => true,
        ('.', _) | (_, '.') => true,
        (',', _) => true,  // Comma followed by anything
        (_, ':') => true,  // No space before colon
        (_, ';') => true,  // No space before semicolon
        // Dash with alphanumeric or dash (handles -fsSL, --release, build-prod, -5)
        ('-', c) | (c, '-') if c.is_alphanumeric() || c == '-' => true,
        // Underscore in identifiers
        ('_', c) | (c, '_') if c.is_alphanumeric() || c == '_' => true,
        // Hash for anchors/preprocessor
        ('#', c) if c.is_alphanumeric() => true,
        // At sign for decorators/mentions
        ('@', c) if c.is_alphanumeric() => true,
        // Ampersand in references
        ('&', c) if c.is_alphanumeric() => true,
        // Asterisk in pointers/wildcards
        ('*', c) | (c, '*') if c.is_alphanumeric() => true,
        // Plus sign adjacent to numbers or identifiers (e.g., +5, C++)
        ('+', c) | (c, '+') if c.is_alphanumeric() || c == '+' => true,
        _ => false,
    }
}

/// Get attribute value from element, filtering empty values
pub(super) fn get_attr(attrs: &[Attribute], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|attr| &*attr.name.local == name)
        .map(|attr| attr.value.to_string())
        .filter(|v| !v.trim().is_empty())
}

/// Check if element has widget-like class or id that should be skipped
///
/// This filters out common widget elements like:
/// - Social media sharing buttons (class contains "social", "share", "follow")
/// - Cookie consent notices (class/id contains "cookie", "popup", "modal", "overlay")
/// - Advertisement containers (class/id contains "ad-", "ads-", "advertisement")
///
/// Returns `true` if the element should be skipped (not converted to markdown).
pub(super) fn is_widget_element(attrs: &[Attribute]) -> bool {
    // Check class attribute
    if let Some(class) = get_attr(attrs, "class") {
        let class_lower = class.to_lowercase();
        if class_lower.contains("social")
            || class_lower.contains("share")
            || class_lower.contains("follow")
            || class_lower.contains("cookie")
            || class_lower.contains("popup")
            || class_lower.contains("modal")
            || class_lower.contains("overlay")
            || class_lower.contains("ad-")
            || class_lower.contains("ads-")
            || class_lower.contains("advertisement")
        {
            return true;
        }
    }

    // Check id attribute
    if let Some(id) = get_attr(attrs, "id") {
        let id_lower = id.to_lowercase();
        if id_lower.contains("cookie")
            || id_lower.contains("popup")
            || id_lower.contains("modal")
            || id_lower.contains("overlay")
            || id_lower.contains("ad-")
            || id_lower.contains("ads-")
        {
            return true;
        }
    }

    false
}
