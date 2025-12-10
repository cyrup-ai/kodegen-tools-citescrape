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
/// - Code blocks (`<pre>`, `<code>`): Language inference from content when HTML lacks hints
/// - Links (`<a>`): Fallback text extraction from aria-label, title, or cleaned href
pub fn create_converter() -> HtmlToMarkdown {
    HtmlToMarkdown::builder()
        // Custom pre handler with language inference
        .add_handler(vec!["pre"], pre_handler)
        // Custom code handler with language inference
        .add_handler(vec!["code"], code_handler)
        // Custom link handler with aria-label fallback
        .add_handler(vec!["a"], link_handler)
        .build()
}

/// Handle `<pre>` elements - code blocks with fences
fn pre_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Walk children to get content (likely contains <code>)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim_matches('\n');

    // If the child was a code block that already has fences, just return it
    if content.starts_with("```") {
        return Some(HandlerResult::from(format!("\n\n{}\n\n", content)));
    }

    // Otherwise, try to infer language and wrap in fences
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
fn code_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Check if inside a <pre> - if so, extract content with language detection
    let is_in_pre = is_inside_pre(element.node);

    // Use raw text extraction to preserve angle brackets (like <Left>, <Right>)
    // that would otherwise be stripped by htmd's default handler pipeline
    let content = extract_raw_text(element.node);

    if is_in_pre {
        // Inside <pre>: return raw content, language detection happens in pre_handler
        // But also try to detect language from this element
        let language = get_language_from_element(&element);

        // Validate HTML language hint against content
        let language = match language {
            Some(lang) if validate_html_language(&lang, &content) => Some(lang),
            Some(_) => infer_language_from_content(&content), // HTML hint was wrong
            None => infer_language_from_content(&content),
        };

        // Return with fence markers for pre_handler to use
        let fence = match &language {
            Some(lang) => format!("```{}", lang),
            None => "```".to_string(),
        };

        Some(HandlerResult::from(format!("{}\n{}\n```", fence, content)))
    } else {
        // Inline code: wrap with backticks
        let trimmed = content.trim();

        // Handle backticks in content
        if trimmed.contains('`') {
            if trimmed.starts_with('`') {
                Some(HandlerResult::from(format!("`` {} ``", trimmed)))
            } else {
                Some(HandlerResult::from(format!("``{}``", trimmed)))
            }
        } else {
            Some(HandlerResult::from(format!("`{}`", trimmed)))
        }
    }
}

/// Handle `<a>` elements with fallback text extraction
fn link_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Get href
    let href = get_attr(element.attrs, "href").unwrap_or_default();

    // Get link text with fallback hierarchy
    let text = handlers.walk_children(element.node).content;
    let text = text.trim();

    let link_text = if text.is_empty() {
        // Fallback 1: aria-label
        get_attr(element.attrs, "aria-label")
            // Fallback 2: title
            .or_else(|| get_attr(element.attrs, "title"))
            // Fallback 3: alt (for image links)
            .or_else(|| get_attr(element.attrs, "alt"))
            // Fallback 4: cleaned href
            .unwrap_or_else(|| clean_url_for_display(&href))
    } else {
        text.to_string()
    };

    // Get optional title for markdown link
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

    // Title case the first word
    if let Some(first_char) = cleaned.chars().next() {
        format!(
            "{}{}",
            first_char.to_uppercase(),
            &cleaned[first_char.len_utf8()..]
        )
    } else {
        cleaned.to_string()
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
}
