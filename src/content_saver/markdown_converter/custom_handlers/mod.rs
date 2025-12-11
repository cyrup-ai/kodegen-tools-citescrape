//! Custom handlers for htmd HTML-to-Markdown conversion
//!
//! This module provides custom element handlers for htmd that extend
//! the default behavior with language inference and improved link handling.

pub mod language_inference;

use htmd::{
    element_handler::{HandlerResult, Handlers},
    Element, HtmlToMarkdown,
};

use language_inference::{
    extract_language_from_class, infer_language_from_content, validate_html_language,
};

/// Create an htmd converter with custom handlers for citescrape
///
/// Custom handlers:
/// - Block elements (`<p>`, `<h1>`-`<h6>`): Proper spacing to prevent text concatenation
/// - Code blocks (`<pre>`, `<code>`): Language inference from content when HTML lacks hints
/// - Links (`<a>`): Fallback text extraction from aria-label, title, or cleaned href
/// - Text formatting (`<strong>`, `<em>`): Preserve bold and italic formatting
/// - Generic inline elements: Ensure text is never lost from unsupported HTML elements
pub fn create_converter() -> HtmlToMarkdown {
    // Define all inline elements that should preserve text
    // Note: "span" has a dedicated handler for CSS style detection
    let inline_elements = vec![
        "div", "kbd", "samp", "var", "mark", "time",
        "abbr", "cite", "q", "dfn", "data", "s", "del", "ins", 
        "u", "ruby", "rt", "rp", "bdi", "bdo",
    ];
    
    HtmlToMarkdown::builder()
        // Block-level element handlers (MUST come first for priority)
        .add_handler(vec!["p"], p_handler)
        .add_handler(vec!["h1", "h2", "h3", "h4", "h5", "h6"], heading_handler)
        
        // Special formatting elements
        .add_handler(vec!["strong", "b"], strong_handler)
        .add_handler(vec!["em", "i"], em_handler)
        
        // Span handler for CSS inline styles (bold/italic detection)
        .add_handler(vec!["span"], span_handler)
        
        // Code and links
        .add_handler(vec!["pre"], pre_handler)
        .add_handler(vec!["code"], code_handler)
        .add_handler(vec!["a"], link_handler)
        
        // Custom video handler - convert to markdown link instead of fallback text
        .add_handler(vec!["video"], video_handler)
        // Custom audio handler - convert to markdown link instead of fallback text
        .add_handler(vec!["audio"], audio_handler)
        
        // Custom list handlers for proper nested content extraction
        .add_handler(vec!["ol"], ol_handler)
        .add_handler(vec!["ul"], ul_handler)
        .add_handler(vec!["li"], li_handler)
        
        // Fallback for all other inline elements
        // This ensures text is NEVER lost
        .add_handler(inline_elements, inline_element_handler)
        .build()
}

/// Handle `<p>` elements - paragraphs with proper spacing
///
/// Paragraphs are block-level elements that require blank lines before and after
/// to prevent concatenation with adjacent content (headings, other paragraphs, lists).
fn p_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Extract paragraph content (handles nested inline elements)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    if content.is_empty() {
        // Skip empty paragraphs
        return None;
    }
    
    // Wrap with blank lines: "\n\n{content}\n\n"
    // This ensures separation from adjacent block elements
    Some(HandlerResult::from(format!("\n\n{}\n\n", content)))
}

/// Handle heading elements (`<h1>` through `<h6>`) with proper spacing
///
/// Headings are block-level elements requiring blank lines before and after.
/// Markdown headings use ATX style: `# H1`, `## H2`, etc.
fn heading_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    use markup5ever_rcdom::NodeData;
    
    // Determine heading level from tag name
    let level = if let NodeData::Element { ref name, .. } = element.node.data {
        match &*name.local {
            "h1" => 1,
            "h2" => 2,
            "h3" => 3,
            "h4" => 4,
            "h5" => 5,
            "h6" => 6,
            _ => return None, // Should never happen
        }
    } else {
        return None;
    };
    
    // Extract heading text (handles nested inline elements like <strong>, <a>, etc.)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    if content.is_empty() {
        // Skip empty headings
        return None;
    }
    
    // Generate ATX-style markdown heading
    let heading_prefix = "#".repeat(level);
    
    // Wrap with blank lines: "\n\n{## Heading}\n\n"
    // This ensures separation from adjacent block elements
    Some(HandlerResult::from(format!("\n\n{} {}\n\n", heading_prefix, content)))
}

/// Handle `<pre>` elements - code blocks with fences
fn pre_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Walk children (which processes <code> via code_handler)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim_matches('\n');

    // If code_handler already added fences, just wrap with newlines
    if content.starts_with("```") && content.ends_with("```") {
        return Some(HandlerResult::from(format!("\n\n{}\n\n", content)));
    }

    // Otherwise, infer language and add fences (fallback for non-code <pre>)
    let language = get_language_from_element(&element)
        .or_else(|| infer_language_from_content(content));

    let fence = match &language {
        Some(lang) => format!("```{}", lang),
        None => "```".to_string(),
    };

    Some(HandlerResult::from(format!(
        "\n\n{}\n{}\n```\n\n",
        fence, content
    )))
}

/// Handle `<code>` elements - inline code or code block content
fn code_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Check if inside a <pre> - if so, extract content with language detection
    let is_in_pre = is_inside_pre(element.node);

    if is_in_pre {
        // ✅ FIX: Use handler pipeline instead of raw text extraction
        let result = handlers.walk_children(element.node);
        let raw_content = result.content.trim();
        
        // ✅ FIX: Normalize excessive blank lines (3+ newlines → 2 newlines)
        let content = normalize_code_whitespace(raw_content);

        // Language inference (keep existing logic)
        let language = get_language_from_element(&element)
            .filter(|lang| validate_html_language(lang, &content))
            .or_else(|| infer_language_from_content(&content));

        let fence = match &language {
            Some(lang) => format!("```{}", lang),
            None => "```".to_string(),
        };

        // Return fenced code for pre_handler to wrap
        Some(HandlerResult::from(format!("{}\n{}\n```", fence, content)))
    } else {
        // Inline code: also use handler pipeline
        let result = handlers.walk_children(element.node);
        let raw_content = result.content.trim();
        
        // ✅ FIX: Normalize excessive blank lines in inline code too
        let content = normalize_code_whitespace(raw_content);

        // Handle backticks in content (keep existing logic)
        if content.contains('`') {
            if content.starts_with('`') {
                Some(HandlerResult::from(format!("`` {} ``", content)))
            } else {
                Some(HandlerResult::from(format!("``{}``", content)))
            }
        } else {
            Some(HandlerResult::from(format!("`{}`", content)))
        }
    }
}

/// Handle `<a>` elements with fallback text extraction and validation
fn link_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Get href
    let href = get_attr(element.attrs, "href").unwrap_or_default();
    
    // Skip links with completely empty or meaningless hrefs
    if href.is_empty() || href == "#" || href == "javascript:void(0)" {
        // Return just the text content without the link wrapper
        // Use aggressive text extraction to ensure we get ALL text from nested elements
        let text = extract_raw_text(element.node);
        let text = text.trim();
        if !text.is_empty() {
            return Some(HandlerResult::from(text.to_string()));
        } else {
            return Some(HandlerResult::from(String::new()));
        }
    }

    // Extract link text with fallback hierarchy
    // Use aggressive text extraction instead of walk_children to ensure
    // we get text from nested inline elements like <span>, <kbd>, etc.
    let text = extract_raw_text(element.node);
    let text = text.trim();

    // Determine final link text with robust fallback chain
    let link_text = if is_meaningful_link_text(text) {
        // Text is meaningful, use it
        text.to_string()
    } else {
        // Text is empty or meaningless, try fallbacks
        get_attr(element.attrs, "aria-label")
            .filter(|s| is_meaningful_link_text(s))
            .or_else(|| get_attr(element.attrs, "title").filter(|s| is_meaningful_link_text(s)))
            .or_else(|| get_attr(element.attrs, "alt").filter(|s| is_meaningful_link_text(s)))
            .unwrap_or_else(|| clean_url_for_display(&href))
    };

    // Final validation: if link_text is STILL not meaningful, skip the link
    if !is_meaningful_link_text(&link_text) {
        // This should rarely happen with the improved clean_url_for_display,
        // but if it does, return empty to skip the link entirely
        tracing::warn!(
            "Skipping link with no meaningful text: href={}, extracted_text={:?}",
            href,
            text
        );
        return Some(HandlerResult::from(String::new()));
    }

    // Get optional title for markdown link (only if different from link text)
    let title = get_attr(element.attrs, "title");

    // Build markdown link
    let result = if let Some(title_text) = title {
        if !title_text.is_empty() && title_text != link_text {
            format!("[{}]({} \"{}\")", link_text, href, title_text)
        } else {
            format!("[{}]({})", link_text, href)
        }
    } else {
        format!("[{}]({})", link_text, href)
    };

    Some(HandlerResult::from(result))
}

/// Handle `<video>` elements - convert to markdown link with video indicator
fn video_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Try to get video source URL
    // Priority: 1. Direct src attribute, 2. First <source> child element
    let src = get_attr(element.attrs, "src")
        .or_else(|| find_source_url(element.node))
        .filter(|s| !s.is_empty())?;
    
    // Extract poster image if present
    let poster = get_attr(element.attrs, "poster");
    
    // Create display name from URL
    let filename = extract_filename_from_url(&src);
    
    // Build markdown output
    let mut result = format!("[Video: {}]({})", filename, src);
    
    // If there's a poster image, include it as well
    if let Some(poster_url) = poster
        && !poster_url.is_empty()
    {
        result.push_str(&format!("\n\n![Video thumbnail]({})", poster_url));
    }
    
    Some(HandlerResult::from(result))
}

/// Handle `<audio>` elements - convert to markdown link with audio indicator
fn audio_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Try to get audio source URL
    // Priority: 1. Direct src attribute, 2. First <source> child element
    let src = get_attr(element.attrs, "src")
        .or_else(|| find_source_url(element.node))
        .filter(|s| !s.is_empty())?;
    
    // Create display name from URL
    let filename = extract_filename_from_url(&src);
    
    // Build markdown output
    let result = format!("[Audio: {}]({})", filename, src);
    
    Some(HandlerResult::from(result))
}

/// Handle generic inline elements by extracting their text content
/// This ensures text is never lost, even from unsupported HTML elements
fn inline_element_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Recursively extract text from all children
    let text = extract_raw_text(element.node);
    
    // Return the text as-is (no markdown formatting)
    // The parent element's handler will apply formatting if needed
    Some(HandlerResult::from(text))
}

/// Handle `<span>` elements - detect inline styles and apply markdown formatting
fn span_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Get the inner content first
    let text = extract_raw_text(element.node);
    let content = text.trim();
    
    // If no content, return nothing
    if content.is_empty() {
        return None;
    }
    
    // Check for style attribute
    let style = get_attr(element.attrs, "style");
    
    if let Some(style_str) = style {
        let (is_bold, is_italic) = parse_style_formatting(&style_str);
        
        // Apply markdown formatting based on detected styles
        let formatted = match (is_bold, is_italic) {
            (true, true) => {
                // Both bold and italic: ***text***
                format!("***{}***", content)
            }
            (true, false) => {
                // Bold only: **text**
                format!("**{}**", content)
            }
            (false, true) => {
                // Italic only: *text*
                format!("*{}*", content)
            }
            (false, false) => {
                // No recognized formatting, return plain content
                // This preserves color, background, and other non-markdown styles as plain text
                content.to_string()
            }
        };
        
        return Some(HandlerResult::from(formatted));
    }
    
    // No style attribute, return plain content
    Some(HandlerResult::from(content.to_string()))
}

/// Handle <strong> and <b> tags - bold text
fn strong_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let text = extract_raw_text(element.node).trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(HandlerResult::from(format!("**{}**", text)))
}

/// Handle <em> and <i> tags - italic text
fn em_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let text = extract_raw_text(element.node).trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(HandlerResult::from(format!("*{}*", text)))
}

/// Handle `<li>` elements - list items with deep content extraction
///
/// Ensures all nested content (divs, spans, etc.) is properly extracted
/// by using walk_children to recursively traverse the entire element tree.
fn li_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Walk all children to extract nested content (divs, spans, etc.)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    // Return content with proper spacing
    // The list marker (number or bullet) will be added by the parent ol/ul handler
    if !content.is_empty() {
        Some(HandlerResult::from(format!("{}\n", content)))
    } else {
        // Empty list item - preserve it
        Some(HandlerResult::from("\n".to_string()))
    }
}

/// Handle `<ol>` elements - ordered lists with proper numbering
///
/// Processes list items and adds markdown-style numbering (1. 2. 3. etc.)
fn ol_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    use markup5ever_rcdom::NodeData;
    
    // Get the starting number from the 'start' attribute (defaults to 1)
    let start_num = get_attr(element.attrs, "start")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    
    let mut output = String::from("\n");
    let mut item_number = start_num;
    
    // Iterate through children looking for <li> elements
    for child in element.node.children.borrow().iter() {
        if let NodeData::Element { ref name, .. } = child.data
            && &*name.local == "li"
        {
            // Process the list item
            let item_result = handlers.walk_children(child);
            let content = item_result.content.trim();
            
            if !content.is_empty() {
                // Add numbered list item: "1. content\n"
                output.push_str(&format!("{}. {}\n", item_number, content));
                item_number += 1;
            } else {
                // Empty list item - still increment number
                output.push_str(&format!("{}. \n", item_number));
                item_number += 1;
            }
        }
    }
    
    output.push('\n');
    Some(HandlerResult::from(output))
}

/// Handle `<ul>` elements - unordered lists with bullet points
///
/// Processes list items and adds markdown-style bullets (- or *)
fn ul_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    use markup5ever_rcdom::NodeData;
    
    let mut output = String::from("\n");
    
    // Iterate through children looking for <li> elements
    for child in element.node.children.borrow().iter() {
        if let NodeData::Element { ref name, .. } = child.data
            && &*name.local == "li"
        {
            // Process the list item
            let item_result = handlers.walk_children(child);
            let content = item_result.content.trim();
            
            if !content.is_empty() {
                // Add bullet list item: "- content\n"
                output.push_str(&format!("- {}\n", content));
            } else {
                // Empty list item
                output.push_str("- \n");
            }
        }
    }
    
    output.push('\n');
    Some(HandlerResult::from(output))
}

// === Helper Functions ===

/// Extract raw text content from a node tree, preserving all whitespace
/// This bypasses htmd's handler pipeline which would strip unknown HTML-like content
fn extract_raw_text(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> String {
    use markup5ever_rcdom::NodeData;

    let mut text = String::new();

    match &node.data {
        NodeData::Text { contents } => {
            // Preserve text exactly as-is (including angle brackets from decoded entities)
            text.push_str(&contents.borrow());
        }
        NodeData::Element { .. } | NodeData::Document | NodeData::Doctype { .. } => {
            // Recursively process all children
            for child in node.children.borrow().iter() {
                text.push_str(&extract_raw_text(child));
            }
        }
        NodeData::Comment { .. } | NodeData::ProcessingInstruction { .. } => {
            // Skip comments and processing instructions
        }
    }

    text
}

/// Normalize excessive blank lines in code content
///
/// Collapses sequences of 3+ consecutive newlines into exactly 2 newlines,
/// preserving intentional single blank lines while removing excessive spacing
/// from HTML-sourced code blocks.
///
/// # Newline Semantics
///
/// - **1 newline** (`\n`) = no blank line (consecutive statements)
/// - **2 newlines** (`\n\n`) = 1 blank line (intentional spacing, **preserve**)
/// - **3+ newlines** (`\n\n\n+`) = excessive spacing (**collapse to 2**)
///
/// # Rationale
///
/// Standard code style guides (Rust, Python, JavaScript) allow at most
/// 1 blank line between logical sections. Multiple blank lines are never
/// idiomatic and indicate HTML formatting artifacts.
///
/// # Performance
///
/// - Regex compiled once via `LazyLock` (zero overhead on subsequent calls)
/// - Single-pass replacement (O(n) time complexity)
/// - Only applied to `<code>` element content (typically < 1KB strings)
///
/// # Examples
///
/// ```rust
/// let input = "fn main() {\n\n\n    println!(\"Hello\");\n}";
/// let output = normalize_code_whitespace(input);
/// assert_eq!(output, "fn main() {\n\n    println!(\"Hello\");\n}");
/// ```
fn normalize_code_whitespace(content: &str) -> String {
    use std::sync::LazyLock;
    use regex::Regex;
    
    // Match 3 or more consecutive newlines
    static EXCESSIVE_NEWLINES: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\n{3,}").expect("EXCESSIVE_NEWLINES: hardcoded regex is valid")
    });
    
    // Replace with exactly 2 newlines (= 1 blank line)
    EXCESSIVE_NEWLINES.replace_all(content, "\n\n").to_string()
}

/// Check if a node is inside a <pre> element
fn is_inside_pre(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> bool {
    use markup5ever_rcdom::NodeData;

    let mut current = node.parent.take();
    node.parent.set(current.clone());

    while let Some(weak_parent) = current {
        if let Some(parent) = weak_parent.upgrade() {
            if let NodeData::Element { ref name, .. } = parent.data
                && &*name.local == "pre"
            {
                return true;
            }
            current = parent.parent.take();
            parent.parent.set(current.clone());
        } else {
            break;
        }
    }
    false
}

/// Find the first `<source>` element's src attribute in child nodes
fn find_source_url(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> Option<String> {
    use markup5ever_rcdom::NodeData;

    // Check direct children only (not recursive - source elements are direct children)
    for child in node.children.borrow().iter() {
        if let NodeData::Element { ref name, ref attrs, .. } = child.data {
            // Check if this is a <source> element
            if &*name.local == "source" {
                // Extract src attribute
                for attr in attrs.borrow().iter() {
                    if &*attr.name.local == "src" {
                        let src = attr.value.to_string().trim().to_string();
                        if !src.is_empty() {
                            return Some(src);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract a human-readable filename from a URL for display
fn extract_filename_from_url(url: &str) -> String {
    // Split by / and get the last segment
    let path = url.split('/').next_back().unwrap_or(url);
    
    // Remove query parameters and fragments
    let filename = path
        .split('?').next().unwrap_or(path)
        .split('#').next().unwrap_or(path);
    
    // URL decode if needed (e.g., %20 -> space)
    let decoded = urlencoding::decode(filename).unwrap_or(std::borrow::Cow::Borrowed(filename));
    
    // If we got a meaningful filename, use it
    if !decoded.is_empty() && decoded != "/" {
        decoded.to_string()
    } else {
        "media file".to_string()
    }
}

/// Get language from element attributes (class or data-language)
fn get_language_from_element(element: &Element) -> Option<String> {
    // Try data-language first
    if let Some(lang) = get_attr(element.attrs, "data-language")
        && !lang.is_empty()
    {
        return Some(lang);
    }

    // Try class attribute
    if let Some(class) = get_attr(element.attrs, "class") {
        return extract_language_from_class(&class);
    }

    None
}

/// Get attribute value from element
fn get_attr(attrs: &[html5ever::Attribute], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|a| &*a.name.local == name)
        .map(|a| a.value.to_string())
        .filter(|v| !v.trim().is_empty())
}

/// Parse inline CSS style attribute and detect bold/italic formatting
/// 
/// Returns (is_bold, is_italic) tuple based on CSS properties:
/// - font-weight: bold, bolder, or numeric >= 600
/// - font-style: italic or oblique
fn parse_style_formatting(style: &str) -> (bool, bool) {
    let mut is_bold = false;
    let mut is_italic = false;
    
    // Parse semicolon-separated CSS properties
    for property in style.split(';') {
        let parts: Vec<&str> = property.split(':').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            continue;
        }
        
        let (key, value) = (parts[0].to_lowercase(), parts[1].to_lowercase());
        
        match key.as_str() {
            "font-weight" => {
                // Check for bold keyword values
                if matches!(value.as_str(), "bold" | "bolder") {
                    is_bold = true;
                } else if let Ok(weight) = value.parse::<u16>() {
                    // Numeric font-weight: 600+ is bold
                    is_bold = weight >= 600;
                }
            }
            "font-style" => {
                // Check for italic values
                is_italic = matches!(value.as_str(), "italic" | "oblique");
            }
            _ => {}
        }
    }
    
    (is_bold, is_italic)
}

/// Check if text is meaningful for a link (not just whitespace/punctuation)
fn is_meaningful_link_text(text: &str) -> bool {
    let trimmed = text.trim();
    
    // Empty is not meaningful
    if trimmed.is_empty() {
        return false;
    }
    
    // Single character that's only punctuation is not meaningful
    if trimmed.len() == 1
        && let Some(ch) = trimmed.chars().next()
        && ch.is_ascii_punctuation()
    {
        return false;
    }
    
    // Check if it contains at least one alphanumeric character
    // This filters out pure punctuation like ".", "...", "---", etc.
    trimmed.chars().any(|c| c.is_alphanumeric())
}

/// Clean URL for display when used as link text fallback
fn clean_url_for_display(url: &str) -> String {
    // Remove leading slash for relative URLs
    let cleaned = url.trim_start_matches('/');

    // Remove query parameters and fragments
    let cleaned = cleaned.split('?').next().unwrap_or(cleaned);
    let cleaned = cleaned.split('#').next().unwrap_or(cleaned);

    // Remove file extensions for cleaner display
    let cleaned = cleaned.trim_end_matches(".html");
    let cleaned = cleaned.trim_end_matches(".htm");
    let cleaned = cleaned.trim_end_matches("/index");

    // Replace hyphens/underscores with spaces
    let cleaned = cleaned.replace(['-', '_'], " ");

    // If cleaned is empty or just whitespace, return a sensible fallback
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return "link".to_string();  // Generic but meaningful fallback
    }

    // Title case the first word
    if let Some(first_char) = cleaned.chars().next() {
        format!(
            "{}{}",
            first_char.to_uppercase(),
            &cleaned[first_char.len_utf8()..]
        )
    } else {
        "link".to_string()  // Should never reach here, but safety fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_url_for_display() {
        assert_eq!(clean_url_for_display("/guide"), "Guide");
        assert_eq!(
            clean_url_for_display("/installation-guide"),
            "Installation guide"
        );
        assert_eq!(
            clean_url_for_display("/docs/api.html"),
            "Docs/api"
        );
    }

    #[test]
    fn test_code_block_basic() {
        let converter = create_converter();
        let html = r#"<pre><code>fn main() {
    println!("Hello");
}</code></pre>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("```rust"), "Should detect Rust language");
        assert!(md.contains("fn main()"), "Should preserve code content");
        assert!(md.contains("```"), "Should have closing fence");
    }

    #[test]
    fn test_code_block_with_language_class() {
        let converter = create_converter();
        let html = r#"<pre class="language-python"><code>def hello():
    print("world")</code></pre>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("```python"), "Should use HTML language hint");
    }

    #[test]
    fn test_inline_code() {
        let converter = create_converter();
        let html = r#"<p>Use the <code>println!</code> macro</p>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("`println!`"), "Should wrap inline code with backticks");
    }

    #[test]
    fn test_link_with_text() {
        let converter = create_converter();
        let html = r#"<a href="/guide">Installation Guide</a>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("[Installation Guide](/guide)"));
    }

    #[test]
    fn test_link_with_aria_label_fallback() {
        let converter = create_converter();
        let html = r#"<a href="/guide" aria-label="Installation Guide"></a>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("[Installation Guide](/guide)"));
    }

    #[test]
    fn test_link_href_fallback() {
        let converter = create_converter();
        let html = r#"<a href="/installation-guide"></a>"#;
        let md = converter.convert(html).unwrap();

        // Should clean up href for display
        assert!(md.contains("[Installation guide](/installation-guide)"));
    }

    #[test]
    fn test_code_block_with_angle_brackets() {
        let converter = create_converter();
        // Test HTML-escaped angle brackets that should be preserved in code
        let html = r#"<pre><code>"&lt;Left&gt;".blue().bold()</code></pre>"#;
        let md = converter.convert(html).unwrap();

        // Should preserve <Left> after HTML entity decoding
        assert!(
            md.contains("<Left>"),
            "Should preserve angle brackets in code. Got: {}",
            md
        );
    }

    #[test]
    fn test_inline_code_with_angle_brackets() {
        let converter = create_converter();
        let html = r#"<p>Press <code>&lt;Left&gt;</code> to go back</p>"#;
        let md = converter.convert(html).unwrap();

        // Should preserve <Left> in inline code
        assert!(
            md.contains("`<Left>`"),
            "Should preserve angle brackets in inline code. Got: {}",
            md
        );
    }

    #[test]
    fn test_list_with_bold() {
        let converter = create_converter();
        let html = r#"<ul><li><strong>Homebrew (macOS, Linux):</strong> Install via brew</li></ul>"#;
        let md = converter.convert(html).unwrap();

        // Should have proper bold formatting in list
        assert!(
            md.contains("**Homebrew (macOS, Linux):**"),
            "Should have proper bold in list. Got: {}",
            md
        );
    }

    #[test]
    fn test_span_with_bold() {
        let converter = create_converter();
        // This is the pattern from Claude Code docs: <span data-as="p"><strong>text:</strong></span>
        let html = r#"<span data-as="p"><strong>Homebrew (macOS, Linux):</strong></span>"#;
        let md = converter.convert(html).unwrap();

        // Should have proper bold formatting
        assert!(
            md.contains("**Homebrew (macOS, Linux):**"),
            "Should have proper bold. Got: '{}'",
            md
        );
    }

    #[test]
    fn test_link_with_empty_text() {
        let converter = create_converter();
        let html = r#"<a href="/guide"></a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should use href fallback
        assert!(md.contains("[Guide](/guide)"), "Empty link should use href fallback. Got: {}", md);
    }

    #[test]
    fn test_link_with_only_period() {
        let converter = create_converter();
        let html = r#"<a href="/troubleshooting">.</a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should NOT output [.](url), should use fallback
        assert!(!md.contains("[.]"), "Should not output period as link text");
        assert!(md.contains("[Troubleshooting](/troubleshooting)"), "Should use href fallback. Got: {}", md);
    }

    #[test]
    fn test_link_with_only_whitespace() {
        let converter = create_converter();
        let html = r#"<a href="/docs">   </a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should use href fallback, not empty text
        assert!(md.contains("[Docs](/docs)"), "Whitespace-only link should use href fallback. Got: {}", md);
    }

    #[test]
    fn test_link_with_empty_href() {
        let converter = create_converter();
        let html = r#"<a href="">Click here</a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should return just the text, not a link
        assert!(!md.contains("[Click here]"), "Empty href should not create link");
        assert!(md.contains("Click here"), "Should preserve text content");
    }

    #[test]
    fn test_link_with_hash_only() {
        let converter = create_converter();
        let html = r##"<a href="#">Section link</a>"##;
        let md = converter.convert(html).unwrap();
        
        // Should return just the text for # links (page anchors without context)
        assert!(md.contains("Section link"), "Should preserve text");
    }

    #[test]
    fn test_link_with_nested_span() {
        let converter = create_converter();
        let html = r#"<a href="/guide"><span>Installation Guide</span></a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should extract text from nested span
        assert!(md.contains("[Installation Guide](/guide)"), "Should extract nested span text. Got: {}", md);
    }

    #[test]
    fn test_link_with_aria_label_meaningful() {
        let converter = create_converter();
        let html = r#"<a href="/help" aria-label="Get Help">.</a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should use aria-label instead of period
        assert!(md.contains("[Get Help](/help)"), "Should use aria-label over meaningless text. Got: {}", md);
    }

    #[test]
    fn test_is_meaningful_link_text_helper() {
        // Meaningful
        assert!(is_meaningful_link_text("Guide"));
        assert!(is_meaningful_link_text("Installation guide"));
        assert!(is_meaningful_link_text("API"));
        assert!(is_meaningful_link_text("v1.0"));
        
        // Not meaningful
        assert!(!is_meaningful_link_text(""));
        assert!(!is_meaningful_link_text("   "));
        assert!(!is_meaningful_link_text("."));
        assert!(!is_meaningful_link_text(","));
        assert!(!is_meaningful_link_text("!"));
        assert!(!is_meaningful_link_text("..."));
        assert!(!is_meaningful_link_text("---"));
        
        // Edge cases
        assert!(is_meaningful_link_text("1"));  // Single digit is meaningful
        assert!(is_meaningful_link_text("a"));  // Single letter is meaningful
        assert!(is_meaningful_link_text("v.1.0"));  // Has alphanumeric
    }

    #[test]
    fn test_inline_span_preserves_text() {
        let converter = create_converter();
        let html = r#"<p>There was <span>a breaking change</span> in the code</p>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("a breaking change"),
            "Span text should be preserved. Got: {}",
            md
        );
    }

    #[test]
    fn test_nested_inline_elements() {
        let converter = create_converter();
        let html = r#"<p>Click <a href="/docs"><span class="icon">here</span></a> to continue</p>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("[here](/docs)"),
            "Nested span in link should preserve text. Got: {}",
            md
        );
    }

    #[test]
    fn test_kbd_element() {
        let converter = create_converter();
        let html = r#"<p>Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to copy</p>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("Ctrl") && md.contains("C"),
            "Kbd elements should preserve text. Got: {}",
            md
        );
    }

    #[test]
    fn test_multiple_inline_elements() {
        let converter = create_converter();
        let html = r#"<p><strong>Software</strong>: <span>Node.js 18+</span> (only required for npm installation)</p>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("**Software**") && md.contains("Node.js 18+"),
            "Multiple inline elements should preserve all text. Got: {}",
            md
        );
    }

    #[test]
    fn test_empty_inline_elements() {
        let converter = create_converter();
        let html = r#"<p>Text <span></span> more text</p>"#;
        let md = converter.convert(html).unwrap();
        
        // Should not crash, empty span should be skipped
        assert!(md.contains("Text") && md.contains("more text"));
    }

    #[test]
    fn test_real_world_missing_words_patterns() {
        let converter = create_converter();
        
        // Pattern 1: Link with styled text
        let html1 = r#"<p>There was <a href="/change"><span>a breaking change</span></a> in version 0.1.14</p>"#;
        let md1 = converter.convert(html1).unwrap();
        assert!(md1.contains("a breaking change"));
        
        // Pattern 2: Bold label with inline value
        let html2 = r#"<p><strong>Software</strong>: <code>Node.js 18+</code> (only required...)</p>"#;
        let md2 = converter.convert(html2).unwrap();
        assert!(md2.contains("Node.js 18+"));
        
        // Pattern 3: Text with inline code
        let html3 = r#"<p>If you have <kbd>nvm</kbd> installed:</p>"#;
        let md3 = converter.convert(html3).unwrap();
        assert!(md3.contains("nvm"));
    }
}
#[test]
fn test_paragraph_heading_separation() {
    use crate::content_saver::markdown_converter::custom_handlers::create_converter;
    
    let converter = create_converter();
    
    // Test from task: Paragraph -> Heading -> Paragraph
    let html = r#"<p>rust TUIs.</p><h2>Why Ratatui?</h2><p>Ratatui is designed for developers...</p>"#;
    let md = converter.convert(html).unwrap();
    
    // Should have the heading marker
    assert!(md.contains("## Why Ratatui?"), "Should have heading with ##. Got: {}", md);
    
    // Should NOT have concatenated text (the original bug)
    assert!(!md.contains("TUIs.Why Ratatui?"), "Should not concatenate paragraph and heading. Got: {}", md);
    
    // Should have blank lines (multiple newlines)
    assert!(md.contains("\n\n"), "Should have blank lines between elements. Got: {}", md);
}

#[test]
fn test_multiple_paragraphs_separation() {
    use crate::content_saver::markdown_converter::custom_handlers::create_converter;
    
    let converter = create_converter();
    let html = r#"<p>First paragraph.</p><p>Second paragraph.</p>"#;
    let md = converter.convert(html).unwrap();
    
    // Should have both paragraphs
    assert!(md.contains("First paragraph"), "Should have first paragraph");
    assert!(md.contains("Second paragraph"), "Should have second paragraph");
    
    // Should have blank lines
    assert!(md.contains("\n\n"), "Should have blank lines between paragraphs. Got: {}", md);
}

#[test]
fn test_heading_with_nested_formatting() {
    use crate::content_saver::markdown_converter::custom_handlers::create_converter;
    
    let converter = create_converter();
    let html = r#"<h1>Main <strong>Title</strong> with Link to <a href="/guide">Guide</a></h1>"#;
    let md = converter.convert(html).unwrap();
    
    // Should be H1
    assert!(md.contains("# Main"), "Should have H1 marker. Got: {}", md);
    
    // Should preserve inline formatting
    assert!(md.contains("**Title**"), "Should preserve bold. Got: {}", md);
    assert!(md.contains("[Guide](/guide)"), "Should preserve link. Got: {}", md);
}

#[test]
fn test_empty_paragraphs_skipped() {
    use crate::content_saver::markdown_converter::custom_handlers::create_converter;
    
    let converter = create_converter();
    let html = r#"<p></p><h2>Valid Heading</h2><p>   </p><p>Valid paragraph</p>"#;
    let md = converter.convert(html).unwrap();
    
    // Should have valid content
    assert!(md.contains("## Valid Heading"), "Should have heading");
    assert!(md.contains("Valid paragraph"), "Should have paragraph");
    
    // Empty paragraphs should be skipped (no excessive blank lines)
    let newline_count = md.matches('\n').count();
    // Should have some newlines but not excessive
    assert!(newline_count < 15, "Should not have excessive newlines from empty elements. Got: {}", md);
}
