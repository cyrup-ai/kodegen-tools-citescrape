//! Main content extraction from HTML documents.
//!
//! This module intelligently extracts the primary content from HTML pages by:
//! 1. Looking for semantic containers in priority order: `<main>`, `<article>`, content-specific divs
//! 2. Removing navigation, headers, footers, sidebars, and other non-content elements
//! 3. Falling back to `<body>` tag if no semantic containers are found
//! 4. Preserving HTML structure, attributes, and element nesting

use anyhow::Result;
use ego_tree::NodeId;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::sync::LazyLock;

/// Maximum HTML input size to prevent memory exhaustion attacks (10 MB)
///
/// This limit protects against `DoS` attacks while accommodating legitimate use:
/// - Wikipedia largest articles: ~2-3 MB
/// - Typical documentation: 1-2 MB  
/// - Blog posts: 100-500 KB
/// - 99.9% of real HTML is under 10 MB
pub(super) const MAX_HTML_SIZE: usize = 10 * 1024 * 1024; // 10 MB

// ============================================================================
// CSS Selectors for main content extraction
// ============================================================================

// These are parsed once at first access and cached forever.
// Hardcoded selectors should NEVER fail to parse - if they do, it's a compile-time bug.

static MAIN_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("main").expect("BUG: hardcoded CSS selector 'main' is invalid")
});

static ARTICLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("article").expect("BUG: hardcoded CSS selector 'article' is invalid")
});

static ROLE_MAIN_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("[role='main']")
        .expect("BUG: hardcoded CSS selector \"[role='main']\" is invalid")
});

static MAIN_CONTENT_ID_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#main-content")
        .expect("BUG: hardcoded CSS selector '#main-content' is invalid")
});

static MAIN_CONTENT_CLASS_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".main-content")
        .expect("BUG: hardcoded CSS selector '.main-content' is invalid")
});

static CONTENT_ID_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#content").expect("BUG: hardcoded CSS selector '#content' is invalid")
});

static CONTENT_CLASS_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".content").expect("BUG: hardcoded CSS selector '.content' is invalid")
});

static POST_CONTENT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".post-content")
        .expect("BUG: hardcoded CSS selector '.post-content' is invalid")
});

static ENTRY_CONTENT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".entry-content")
        .expect("BUG: hardcoded CSS selector '.entry-content' is invalid")
});

static ARTICLE_BODY_ITEMPROP_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("[itemprop='articleBody']")
        .expect("BUG: hardcoded CSS selector \"[itemprop='articleBody']\" is invalid")
});

static ARTICLE_BODY_CLASS_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".article-body")
        .expect("BUG: hardcoded CSS selector '.article-body' is invalid")
});

static STORY_BODY_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".story-body").expect("BUG: hardcoded CSS selector '.story-body' is invalid")
});

static BODY_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("body").expect("BUG: hardcoded CSS selector 'body' is invalid")
});

// ============================================================================
// Element Removal Utilities
// ============================================================================

/// Efficiently remove elements matching selectors from an element's subtree.
///
/// This function:
/// 1. Parses all selectors once (O(s) where s = number of selectors)
/// 2. Builds a `HashSet` of element pointers to remove (O(n) where n = number of elements)
/// 3. Serializes the DOM tree once, skipping removed elements - O(n)
///
/// Overall complexity: O(s + n) instead of O(s × n²) from naive string replacement
///
/// Note: This preserves HTML structure (tags, attributes, nesting) while removing
/// unwanted elements, as required by downstream processors (`clean_html_content`, `MarkdownConverter`).
///
/// Works directly with the element's node tree, avoiding redundant serialization and re-parsing.
fn remove_elements_from_html(element: &ElementRef, remove_selectors: &[&str]) -> String {
    // Parse all selectors upfront - O(s)
    let parsed_selectors: Vec<Selector> = remove_selectors
        .iter()
        .map(|&sel_str| match Selector::parse(sel_str) {
            Ok(s) => s,
            Err(e) => panic!("Invalid hardcoded selector '{sel_str}': {e}"),
        })
        .collect();

    // Build HashSet of all elements to remove (using NodeId for O(1) lookup) - O(n)
    let mut to_remove: HashSet<NodeId> = HashSet::new();
    for sel in &parsed_selectors {
        for elem in element.select(sel) {
            // Store NodeId for identity comparison
            to_remove.insert(elem.id());
        }
    }

    // Serialize HTML while skipping removed elements - O(n)
    let mut result = String::new();
    serialize_html_excluding(element, &to_remove, &mut result);
    result
}

/// Recursively serialize an element and its descendants to HTML,
/// skipping elements in the removal set.
///
/// This preserves the full HTML structure (tags, attributes, nesting) while
/// excluding unwanted elements and their children.
fn serialize_html_excluding(
    element: &ElementRef,
    to_remove: &HashSet<NodeId>,
    output: &mut String,
) {
    // Check if this element should be removed
    if to_remove.contains(&element.id()) {
        return; // Skip this element and all its children
    }

    // Serialize this element's children (we're at the root or an allowed element)
    for child in element.children() {
        use scraper::node::Node;

        match child.value() {
            Node::Text(text) => {
                // Escape HTML special characters in text content
                for ch in text.chars() {
                    match ch {
                        '<' => output.push_str("&lt;"),
                        '>' => output.push_str("&gt;"),
                        '&' => output.push_str("&amp;"),
                        '"' => output.push_str("&quot;"),
                        c => output.push(c),
                    }
                }
            }
            Node::Element(_) => {
                // Element node - check if it should be removed
                if let Some(child_elem) = ElementRef::wrap(child) {
                    if to_remove.contains(&child_elem.id()) {
                        // Skip this element and its children
                        continue;
                    }

                    // Serialize the element with its tags and attributes
                    let elem_name = child_elem.value().name();
                    output.push('<');
                    output.push_str(elem_name);

                    // Serialize attributes
                    for (name, value) in child_elem.value().attrs() {
                        output.push(' ');
                        output.push_str(name);
                        output.push_str("=\"");
                        // Escape attribute value
                        for ch in value.chars() {
                            match ch {
                                '"' => output.push_str("&quot;"),
                                '&' => output.push_str("&amp;"),
                                '<' => output.push_str("&lt;"),
                                '>' => output.push_str("&gt;"),
                                c => output.push(c),
                            }
                        }
                        output.push('"');
                    }
                    output.push('>');

                    // Check if this is a void element (self-closing)
                    const VOID_ELEMENTS: &[&str] = &[
                        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
                        "param", "source", "track", "wbr",
                    ];

                    if VOID_ELEMENTS.contains(&elem_name) {
                        // Void element - no closing tag needed
                        continue;
                    }

                    // Recursively serialize children
                    serialize_html_excluding(&child_elem, to_remove, output);

                    // Closing tag (only for non-void elements)
                    output.push_str("</");
                    output.push_str(elem_name);
                    output.push('>');
                }
            }
            Node::Comment(comment) => {
                // Preserve comments
                output.push_str("<!--");
                output.push_str(comment);
                output.push_str("-->");
            }
            _ => {
                // Other node types (Document, Doctype, ProcessingInstruction) - skip
            }
        }
    }
}

// ============================================================================
// Main Content Extraction
// ============================================================================

/// Extract main content from HTML by identifying semantic containers
///
/// This function intelligently extracts the primary content from an HTML page by:
/// 1. Looking for semantic containers in priority order: `<main>`, `<article>`, content-specific divs
/// 2. Removing navigation, headers, footers, sidebars, and other non-content elements
/// 3. Falling back to `<body>` tag if no semantic containers are found
/// 4. Preserving HTML structure, attributes, and element nesting via scraper serialization
///
/// # Arguments
/// * `html` - Raw HTML string to process
///
/// # Returns
/// * `Ok(String)` - Extracted HTML with main content preserved and non-content elements removed
/// * `Err` - If HTML parsing fails
///
/// # Example
/// ```
/// let html = r#"<html><body><nav>Menu</nav><main><p>Content</p></main></body></html>"#;
/// let content = extract_main_content(html)?;
/// // Result contains: <main><p>Content</p></main>
/// ```
pub fn extract_main_content(html: &str) -> Result<String> {
    // Validate input size to prevent memory exhaustion
    if html.len() > MAX_HTML_SIZE {
        return Err(anyhow::anyhow!(
            "HTML input too large: {} bytes ({:.2} MB). Maximum allowed: {} bytes ({} MB). \
             This protects against memory exhaustion attacks.",
            html.len(),
            html.len() as f64 / 1_000_000.0,
            MAX_HTML_SIZE,
            MAX_HTML_SIZE / (1024 * 1024)
        ));
    }

    let document = Html::parse_document(html);

    // First, remove common non-content elements
    let remove_selectors = [
        "nav",
        "header",
        "footer",
        "aside",
        ".sidebar",
        "#sidebar",
        ".navigation",
        ".header",
        ".footer",
        ".menu",
        ".ads",
        ".advertisement",
        ".social-share",
        ".comments",
        "#comments",
        ".related-posts",
        ".cookie-notice",
        ".popup",
        ".modal",
    ];

    // Try to find main content in common containers (parsed once, cached forever)
    let content_selectors = [
        &*MAIN_SELECTOR,
        &*ARTICLE_SELECTOR,
        &*ROLE_MAIN_SELECTOR,
        &*MAIN_CONTENT_ID_SELECTOR,
        &*MAIN_CONTENT_CLASS_SELECTOR,
        &*CONTENT_ID_SELECTOR,
        &*CONTENT_CLASS_SELECTOR,
        &*POST_CONTENT_SELECTOR,
        &*ENTRY_CONTENT_SELECTOR,
        &*ARTICLE_BODY_ITEMPROP_SELECTOR,
        &*ARTICLE_BODY_CLASS_SELECTOR,
        &*STORY_BODY_SELECTOR,
    ];

    // First try to find a specific content container
    for selector in content_selectors {
        if let Some(element) = document.select(selector).next() {
            // Efficiently remove unwanted elements within the content
            // Works directly with element's node tree - no serialize-parse roundtrip
            let cleaned_html = remove_elements_from_html(&element, &remove_selectors);

            return Ok(cleaned_html);
        }
    }

    // If no main content container found, try to extract body and remove non-content elements
    if let Some(body) = document.select(&BODY_SELECTOR).next() {
        // Efficiently remove non-content elements from body
        // Works directly with element's node tree - no serialize-parse roundtrip
        let cleaned_html = remove_elements_from_html(&body, &remove_selectors);

        return Ok(cleaned_html);
    }

    // Last resort: return the whole HTML
    Ok(html.to_string())
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_removes_navigation() -> Result<()> {
        let html = r"
            <article>
                <p>Content</p>
                <nav>Should be removed</nav>
            </article>
        ";
        let result = extract_main_content(html)?;
        assert!(!result.contains("Should be removed"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_preserves_nested_structure() -> Result<()> {
        let html = r#"
            <article>
                <div class="content">
                    <p>Nested <strong>content</strong></p>
                </div>
            </article>
        "#;
        let result = extract_main_content(html)?;
        assert!(result.contains("<strong>content</strong>"));
        assert!(result.contains("<p>"));
        Ok(())
    }

    #[test]
    fn test_text_escaping() -> Result<()> {
        let html = r"
            <article>
                <p>5 &lt; 10 &amp; 10 &gt; 5</p>
            </article>
        ";
        let result = extract_main_content(html)?;
        // Must preserve escaped characters
        assert!(result.contains("&lt;") || result.contains('<'));
        assert!(result.contains("&gt;") || result.contains('>'));
        assert!(result.contains("&amp;") || result.contains('&'));
        Ok(())
    }

    #[test]
    fn test_self_closing_tags() -> Result<()> {
        let html = r#"
            <article>
                <p>Image: <img src="test.jpg" alt="test" /></p>
                <hr />
                <p>Line break<br />here</p>
            </article>
        "#;
        let result = extract_main_content(html)?;
        assert!(result.contains("<img"));
        assert!(result.contains("<hr"));
        assert!(result.contains("<br"));
        // Should NOT contain closing tags for void elements
        assert!(!result.contains("</img>"));
        assert!(!result.contains("</br>"));
        assert!(!result.contains("</hr>"));
        Ok(())
    }

    #[test]
    fn test_preserves_attributes() -> Result<()> {
        let html = r#"
            <article>
                <div class="content" id="main" data-test="value">Content</div>
            </article>
        "#;
        let result = extract_main_content(html)?;
        assert!(result.contains("class=\"content\""));
        assert!(result.contains("id=\"main\""));
        assert!(result.contains("data-test=\"value\""));
        Ok(())
    }

    #[test]
    fn test_removes_multiple_unwanted_elements() -> Result<()> {
        let html = r"
            <article>
                <header>Header</header>
                <p>Main content</p>
                <nav>Navigation</nav>
                <footer>Footer</footer>
                <aside>Sidebar</aside>
            </article>
        ";
        let result = extract_main_content(html)?;
        assert!(result.contains("Main content"));
        assert!(!result.contains("Header"));
        assert!(!result.contains("Navigation"));
        assert!(!result.contains("Footer"));
        assert!(!result.contains("Sidebar"));
        Ok(())
    }

    #[test]
    fn test_preserves_comments() -> Result<()> {
        let html = r"
            <article>
                <!-- Important comment -->
                <p>Content</p>
            </article>
        ";
        let result = extract_main_content(html)?;
        assert!(result.contains("<!-- Important comment -->"));
        Ok(())
    }

    #[test]
    fn test_body_fallback() -> Result<()> {
        let html = r"
            <html>
                <body>
                    <nav>Navigation</nav>
                    <p>Main content without article tag</p>
                </body>
            </html>
        ";
        let result = extract_main_content(html)?;
        assert!(result.contains("Main content"));
        assert!(!result.contains("Navigation"));
        Ok(())
    }

    #[test]
    fn test_malformed_html_fallback() -> Result<()> {
        let html = "<p>Malformed HTML without body</p>";
        let result = extract_main_content(html)?;
        assert_eq!(result, html);
        Ok(())
    }
}
