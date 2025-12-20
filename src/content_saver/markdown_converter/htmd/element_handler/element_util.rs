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
        NodeData::Element { name, attrs, .. } => {
            // Debug logging for element traversal
            tracing::trace!("extract_raw_text: Processing element: {}", &name.local);
            
            // CRITICAL FIX: Skip widget elements (including sr-only spans)
            // This prevents accessibility text from appearing in markdown output
            if is_widget_element(&attrs.borrow()) {
                return String::new();
            }
            
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

/// Check if element should be skipped as widget/UI chrome with table context awareness
///
/// CRITICAL: In table contexts (th/td parents), accessibility classes
/// (sr-only, visually-hidden) must be PRESERVED, not filtered.
/// These represent semantic table structure, not UI widgets.
///
/// # Arguments
/// * `attrs` - Element attributes to check
/// * `parent_context` - Optional parent tag name (e.g., Some("th"), Some("td"))
///
/// # Returns
/// `true` if element should be skipped, `false` if it should be preserved
pub(super) fn is_widget_element_with_context(
    attrs: &[Attribute],
    parent_context: Option<&str>,
) -> bool {
    // In table cell contexts, preserve accessibility content
    if matches!(parent_context, Some("th") | Some("td")) {
        // Only filter actual interactive widgets, NOT accessibility classes
        return is_interactive_widget(attrs);
    }
    
    // Outside tables, use full widget filtering
    is_widget_element(attrs)
}

/// Check for interactive widgets only (no accessibility filtering)
///
/// This function filters ONLY interactive UI elements like buttons, toolbars, etc.
/// It does NOT filter accessibility classes (sr-only, visually-hidden).
///
/// Used in table contexts where accessibility content must be preserved.
fn is_interactive_widget(attrs: &[Attribute]) -> bool {
    // Check class attribute for interactive patterns only
    if let Some(class) = get_attr(attrs, "class") {
        let class_lower = class.to_lowercase();
        
        // Filter ONLY interactive elements, preserve sr-only/visually-hidden
        if class_lower.contains("copy")
            || class_lower.contains("clipboard")
            || class_lower.contains("toolbar")
            || class_lower.contains("button")
            || class_lower.contains("menu")
            || class_lower.contains("social")
            || class_lower.contains("share")
            || class_lower.contains("follow")
            || class_lower.contains("cookie")
            || class_lower.contains("popup")
            || class_lower.contains("modal")
            || class_lower.contains("overlay")
            || class_lower.contains("ad-")
            || class_lower.contains("ads-")
            || class_lower.contains("advertisement")
            || class_lower.contains("theme-toggle")
            || class_lower.contains("mobile-menu")
            || class_lower.contains("hamburger")
            || class_lower.contains("menu-toggle")
            || class_lower.contains("search-button")
        {
            return true;
        }
    }
    
    // Check ARIA role for interactive elements
    if let Some(role) = get_attr(attrs, "role") {
        let role_lower = role.to_lowercase();
        if role_lower == "button"
            || role_lower == "menuitem"
            || role_lower == "tab"
            || role_lower == "switch"
        {
            return true;
        }
    }
    
    // Check data-* attributes for interactive patterns
    for attr in attrs {
        let name = attr.name.local.as_ref();
        if let Some(data_name) = name.strip_prefix("data-")
            && (data_name.contains("clipboard")
                || data_name.contains("copy")
                || data_name == "action"
                || data_name == "command")
        {
            return true;
        }
    }
    
    false
}

/// Check if element has widget-like class, id, or data attributes that should be skipped
///
/// This filters out common widget/interactive elements including:
/// - Social media sharing buttons
/// - Cookie consent notices  
/// - Advertisement containers
/// - Copy buttons and clipboard widgets
/// - Code block toolbars and action buttons
/// - Theme toggles and UI chrome
/// - Documentation framework action buttons
/// - Screen reader only elements (contain UI instructions, not content)
/// - AI assistance buttons
///
/// Patterns sourced from html_cleaning.rs remove_interactive_elements_from_dom()
pub(super) fn is_widget_element(attrs: &[Attribute]) -> bool {
    // ========================================================================
    // ARIA-HIDDEN (HIGHEST PRIORITY - W3C SEMANTIC SIGNAL)
    // ========================================================================
    // aria-hidden="true" is a W3C ARIA specification attribute that explicitly
    // marks elements as not being part of the accessible content tree.
    // 
    // When authors set aria-hidden="true", they are explicitly stating:
    // - This element is decorative/UI chrome only
    // - Screen readers should ignore this content
    // - This should not be part of semantic document content
    // 
    // Common uses:
    // - Tooltips (hover text that duplicates aria-label)
    // - Icon labels (visual labels for SVG icons)
    // - Decorative spacers and dividers
    // - Duplicate content for visual styling
    // - Loading spinners and status indicators
    // 
    // This is THE most authoritative semantic signal for UI chrome.
    // If aria-hidden="true" is present, skip the element regardless of
    // any other attributes or content.
    if let Some(aria_hidden) = get_attr(attrs, "aria-hidden") {
        // Check for "true" value (case-insensitive for robustness)
        if aria_hidden.trim().eq_ignore_ascii_case("true") {
            return true;
        }
    }

    // Check class attribute
    if let Some(class) = get_attr(attrs, "class") {
        let class_lower = class.to_lowercase();
        
        // ========================================================================
        // ORIGINAL WIDGET PATTERNS (from existing is_widget_element)
        // ========================================================================
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
        
        // ========================================================================
        // CLIPBOARD/COPY PATTERNS (from remove_interactive_elements_from_dom)
        // ========================================================================
        if class_lower.contains("copy")
            || class_lower.contains("clipboard")
        {
            return true;
        }
        
        // ========================================================================
        // TOOLBAR PATTERNS
        // ========================================================================
        if class_lower.contains("toolbar")
            || class_lower.contains("code-actions")
            || class_lower.contains("code-header")
        {
            return true;
        }
        
        // ========================================================================
        // UI CHROME PATTERNS
        // ========================================================================
        if class_lower.contains("theme-toggle")
            || class_lower.contains("mobile-menu")
            || class_lower.contains("hamburger")
            || class_lower.contains("menu-toggle")
            || class_lower.contains("search-button")
        {
            return true;
        }
        
        // ========================================================================
        // DOCUMENTATION FRAMEWORK PATTERNS
        // ========================================================================
        if class_lower.contains("sl-copy")           // Starlight (Astro)
            || class_lower.contains("vp-copy")       // VitePress
            || class_lower.contains("nextra-copy")   // Nextra
            || class_lower.contains("docusaurus")    // Docusaurus
            || class_lower.contains("edit-page")
            || class_lower.contains("share-page")
            || class_lower.contains("print-button")
        {
            return true;
        }
        
        // ========================================================================
        // SCREEN READER / VISUALLY HIDDEN (contains UI instructions, not content)
        // ========================================================================
        if class_lower.contains("sr-only")
            || class_lower.contains("screen-reader-only")
            || class_lower.contains("visually-hidden")
        {
            return true;
        }
        
        // ========================================================================
        // SKIP NAVIGATION LINK PATTERNS (WCAG 2.4.1: Bypass Blocks)
        // ========================================================================
        // Skip links allow keyboard/screen reader users to bypass repetitive navigation.
        // Common implementations use explicit skip-link classes.
        if class_lower.contains("skip-link")
            || class_lower.contains("skip-to-content")
            || class_lower.contains("skip-to-main")
            || class_lower.contains("skip-nav")
            || class_lower.contains("skiplink")       // Single word variant
            || class_lower.contains("skip-to-main-content")  // Verbose variant
        {
            return true;
        }
        
        // ========================================================================
        // AI ASSISTANCE PATTERNS
        // ========================================================================
        if class_lower.contains("ai-assist")
            || class_lower.contains("ai-button")
            || class_lower.contains("ask-ai")
        {
            return true;
        }
        
        // ========================================================================
        // SYNTAX HIGHLIGHTER TOOLBARS
        // ========================================================================
        if class_lower.contains("shiki-toolbar")
            || class_lower.contains("prism-toolbar")
            || class_lower.contains("hljs-toolbar")
        {
            return true;
        }
        
        // ========================================================================
        // FOOTER UI CHROME PATTERNS (disclaimers, legal, copyright)
        // ========================================================================
        if class_lower.contains("footer-chrome")
            || class_lower.contains("page-footer")
            || class_lower.contains("site-footer")
            || class_lower.contains("disclaimer")
            || class_lower.contains("legal")
            || class_lower.contains("copyright")
            || class_lower.contains("page-meta")
            || class_lower.contains("document-meta")
        {
            return true;
        }
        
        // ========================================================================
        // KEYBOARD SHORTCUT PATTERNS (keyboard indicators, hotkeys)
        // ========================================================================
        if class_lower.contains("keyboard")
            || class_lower.contains("shortcut")
            || class_lower.contains("hotkey")
            || class_lower.contains("keybinding")
            || class_lower.contains("key-combo")
            || class_lower.contains("kbd-indicator")
        {
            return true;
        }
        
        // ========================================================================
        // AI ASSISTANT/DISCLAIMER PATTERNS (chatbot indicators, AI disclaimers)
        // ========================================================================
        if class_lower.contains("assistant")
            || class_lower.contains("ai-disclaimer")
            || class_lower.contains("chatbot-disclaimer")
            || class_lower.contains("ai-notice")
            || class_lower.contains("ai-indicator")
            || class_lower.contains("bot-indicator")
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
            || id_lower.contains("toolbar")
            || id_lower.contains("actions")
            || id_lower.contains("controls")
        {
            return true;
        }
    }
    
    // ========================================================================
    // DATA-* ATTRIBUTE PATTERNS
    // ========================================================================
    for attr in attrs {
        let name = attr.name.local.as_ref();
        if let Some(data_name) = name.strip_prefix("data-")
            && (data_name.contains("clipboard")
                || data_name.contains("copy")
                || data_name == "action"
                || data_name == "command"
                || data_name.contains("theme"))
        {
            return true;
        }
    }
    
    // ========================================================================
    // ARIA-LABEL PATTERNS (interactive element labels)
    // ========================================================================
    if let Some(aria_label) = get_attr(attrs, "aria-label") {
        let label_lower = aria_label.to_lowercase();
        if label_lower.contains("copy")
            || label_lower.contains("print")
            || label_lower.contains("ai")
            || label_lower.contains("skip")
            || label_lower.contains("jump to")
        {
            return true;
        }
    }
    
    // ========================================================================
    // ARIA ROLE PATTERNS (interactive UI elements + semantic footer content)
    // ========================================================================
    // role="button|menuitem|tab|switch" = interactive UI elements
    // role="contentinfo" = footer/legal/copyright content
    // role="complementary" = supplementary content (often sidebars/disclaimers)
    if let Some(role) = get_attr(attrs, "role") {
        let role_lower = role.to_lowercase();
        if role_lower == "button"
            || role_lower == "menuitem"
            || role_lower == "tab"
            || role_lower == "switch"
            || role_lower.contains("contentinfo")
            || role_lower.contains("complementary")
        {
            return true;
        }
    }

    false
}

/// Check if image element is a theme variant that should be hidden
///
/// Detects theme-switching images using semantic HTML signals:
/// - CSS classes with theme keywords (dark:*, light:*, theme-*, *-dark, *-light)
/// - ARIA hidden attribute (aria-hidden="true")
/// - Inline visibility styles (display:none, visibility:hidden)
/// - Data attributes for theme/mode/color-scheme
///
/// Returns true if the image should be SKIPPED (hidden variant).
pub(super) fn is_theme_variant_image(attrs: &[Attribute]) -> bool {
    // Check aria-hidden="true"
    if let Some(aria_hidden) = get_attr(attrs, "aria-hidden")
        && aria_hidden == "true"
    {
        return true;
    }
    
    // Check inline style for display:none or visibility:hidden
    if let Some(style) = get_attr(attrs, "style") {
        let style_lower = style.to_lowercase().replace(" ", "");
        if style_lower.contains("display:none") || style_lower.contains("visibility:hidden") {
            return true;
        }
    }
    
    // Check CSS classes for theme indicators
    if let Some(class) = get_attr(attrs, "class") {
        let class_lower = class.to_lowercase();
        
        // Tailwind dark mode utilities: "dark:block", "dark:hidden", "light:hidden"
        if class_lower.contains("dark:") || class_lower.contains("light:") {
            // If class contains "hidden" with theme prefix, it's a hidden variant
            if class_lower.contains("hidden") {
                return true;
            }
        }
        
        // Theme class patterns: "theme-dark", "theme-light", "logo-dark", "logo-light"
        if class_lower.contains("-dark") 
            || class_lower.contains("-light")
            || class_lower.contains("dark-")
            || class_lower.contains("light-")
            || class_lower.contains("theme-") 
        {
            // Additional heuristic: if "hidden" class also present, definitely skip
            if class_lower.contains("hidden") {
                return true;
            }
            
            // Check if this is the "dark" variant (skip dark, keep light as default)
            // This is a heuristic: prefer light mode images when both exist
            if class_lower.contains("dark") && !class_lower.contains("light") {
                return true;
            }
        }
    }
    
    // Check data-* attributes for theme indicators
    for attr in attrs {
        let name = attr.name.local.as_ref();
        if let Some(data_name) = name.strip_prefix("data-")
            && (data_name == "theme" || data_name == "mode" || data_name == "color-scheme")
        {
            let value = attr.value.to_string().to_lowercase();
            // Skip dark variants, keep light as default
            if value.contains("dark") {
                return true;
            }
        }
    }
    
    false
}


// ============================================================================
// ADMONITION/CALLOUT DETECTION AND FORMATTING
// ============================================================================

use markup5ever_rcdom::Node;
use std::rc::Rc;

/// Known admonition type keywords (case-insensitive matching)
const ADMONITION_TYPES: &[&str] = &[
    "note", "tip", "hint", "warning", "caution", "important", "info",
    "danger", "error", "success", "example", "quote", "abstract",
    "summary", "tldr", "todo", "bug", "question", "faq", "help",
    "see also", "seealso",
];

/// Check if a class attribute contains admonition-related keywords
/// 
/// Detects common patterns from various documentation frameworks:
/// - Material for MkDocs: "admonition note"
/// - Docusaurus: "admonition admonition-tip"  
/// - VuePress: "custom-block warning"
/// - GitHub: "markdown-alert markdown-alert-note"
/// - Bootstrap: "alert alert-info"
/// - Obsidian/Quarto: "callout callout-note"
pub(super) fn is_admonition_class(class: &str) -> bool {
    let class_lower = class.to_lowercase();
    
    class_lower.contains("admonition")
        || class_lower.contains("callout")
        || (class_lower.contains("alert") && !class_lower.contains("markdown-alert"))
        || class_lower.contains("custom-block")
        || class_lower.contains("markdown-alert")
}

/// Extract admonition type from class attribute
/// 
/// Tries multiple extraction strategies:
/// 1. Look for "admonition-{type}" or "callout-{type}" pattern
/// 2. Check second token in space-separated classes
/// 3. Map common class names (alert-info → info, alert-warning → warning)
pub(super) fn extract_admonition_type_from_class(class: &str) -> Option<String> {
    let class_lower = class.to_lowercase();
    let tokens: Vec<&str> = class_lower.split_whitespace().collect();
    
    // Strategy 1: Look for hyphenated type (admonition-tip, callout-note, alert-warning)
    for token in &tokens {
        if let Some(type_part) = token.strip_prefix("admonition-")
            .or_else(|| token.strip_prefix("callout-"))
            .or_else(|| token.strip_prefix("alert-"))
            .or_else(|| token.strip_prefix("markdown-alert-"))
        {
            // Validate it's a known type
            if ADMONITION_TYPES.contains(&type_part) {
                return Some(capitalize_first(type_part));
            }
        }
    }
    
    // Strategy 2: Check if second token is an admonition type
    if tokens.len() >= 2 {
        let potential_type = tokens[1];
        if ADMONITION_TYPES.contains(&potential_type) {
            return Some(capitalize_first(potential_type));
        }
    }
    
    // Strategy 3: Map common alert classes
    if class_lower.contains("alert-info") {
        return Some("Info".to_string());
    } else if class_lower.contains("alert-warning") {
        return Some("Warning".to_string());
    } else if class_lower.contains("alert-danger") {
        return Some("Danger".to_string());
    } else if class_lower.contains("alert-success") {
        return Some("Success".to_string());
    }
    
    None
}

/// Extract admonition type from first child element's text content
/// 
/// Checks if the first child element contains ONLY an admonition keyword.
/// This handles the structural pattern where the label is a separate element.
pub(super) fn extract_admonition_type_from_first_child(node: &Rc<Node>) -> Option<String> {
    // Get first child element
    let children = node.children.borrow();
    let first_child = children.iter().find(|child| {
        matches!(child.data, NodeData::Element { .. })
    })?;
    
    // Extract text from first child
    let text = extract_raw_text(first_child);
    let trimmed = text.trim();
    
    // Must be short (< 30 chars) and non-empty
    if trimmed.is_empty() || trimmed.len() > 30 {
        return None;
    }
    
    // Check if it matches a known admonition type (case-insensitive)
    let trimmed_lower = trimmed.to_lowercase();
    if ADMONITION_TYPES.contains(&trimmed_lower.as_str()) {
        return Some(capitalize_first(&trimmed_lower));
    }
    
    None
}

/// Capitalize first letter of string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

/// Detect and format admonition blocks as markdown blockquotes
/// 
/// Returns Some(formatted_blockquote) if this element is detected as an admonition,
/// None otherwise.
/// 
/// Detection uses two-phase approach:
/// 1. Class-based detection (most frameworks)
/// 2. Structural detection (fallback for minimal HTML)
pub(super) fn detect_and_format_admonition(
    handlers: &dyn super::Handlers,
    element: &super::super::Element,
) -> Option<super::HandlerResult> {
    use super::super::text_util::TrimDocumentWhitespace;
    
    let mut admonition_type: Option<String> = None;
    let mut skip_first_child = false;
    
    // Phase 1: Class-based detection
    if let Some(class) = get_attr(element.attrs, "class")
        && is_admonition_class(&class)
    {
        // Try to extract type from class
        admonition_type = extract_admonition_type_from_class(&class);
        
        // If type not in class, check first child
        if admonition_type.is_none() {
            admonition_type = extract_admonition_type_from_first_child(element.node);
            if admonition_type.is_some() {
                skip_first_child = true; // Don't include label in content
            }
        }
        
        // Fallback to generic "Note" if we know it's an admonition but can't determine type
        if admonition_type.is_none() {
            admonition_type = Some("Note".to_string());
        }
    }
    
    // Phase 2: Structural detection (fallback)
    if admonition_type.is_none()
        && let Some(detected_type) = extract_admonition_type_from_first_child(element.node)
    {
        admonition_type = Some(detected_type);
        skip_first_child = true;
    }
    
    // If no admonition detected, return None
    let admonition_type = admonition_type?;
    
    // Extract content, optionally skipping first child
    let content = if skip_first_child {
        extract_content_skipping_first_child(handlers, element)
    } else {
        handlers.walk_children(element.node, element.is_pre).content
    };
    
    let content = content.trim_document_whitespace();
    
    if content.is_empty() {
        return None;
    }
    
    // Format as blockquote with bold label
    // Each line gets "> " prefix
    let lines: Vec<&str> = content.lines().collect();
    let mut result = String::new();
    
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            // First line includes the label
            result.push_str(&format!("> **{}:** {}\n", admonition_type, line));
        } else if line.trim().is_empty() {
            // Blank lines become blockquote continuation
            result.push_str("> \n");
        } else {
            // Regular content lines
            result.push_str(&format!("> {}\n", line));
        }
    }
    
    Some(format!("\n\n{}\n\n", result.trim_end()).into())
}

/// Extract content from element's children, skipping the first child element
/// 
/// Used when the first child is the admonition label that should not be included
/// in the content.
fn extract_content_skipping_first_child(
    _handlers: &dyn super::Handlers,
    element: &super::super::Element,
) -> String {
    let mut content = String::new();
    let children = element.node.children.borrow();
    let mut first_element_skipped = false;
    
    for child in children.iter() {
        // Skip first element child (the label)
        if matches!(child.data, NodeData::Element { .. }) && !first_element_skipped {
            first_element_skipped = true;
            continue;
        }
        
        // Process remaining children using extract_raw_text
        let child_content = extract_raw_text(child);
        content.push_str(&child_content);
    }
    
    content
}
