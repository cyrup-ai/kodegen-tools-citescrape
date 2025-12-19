//! Main content extraction from HTML documents.
//!
//! This module extracts the primary content container from HTML pages by:
//! 1. Looking for semantic containers in priority order: `<main>`, `<article>`, content-specific divs
//! 2. Falling back to `<body>` tag if no semantic containers are found
//!
//! Element filtering (nav, header, footer, sidebars, etc.) is handled by htmd handlers.

use anyhow::Result;
use scraper::{Html, Selector};
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
// Main Content Extraction
// ============================================================================

/// Extract main content container from HTML by identifying semantic containers
///
/// This function extracts the primary content container from an HTML page by:
/// 1. Looking for semantic containers in priority order: `<main>`, `<article>`, content-specific divs
/// 2. Falling back to `<body>` tag if no semantic containers are found
/// 3. Returning the raw HTML of the container (element filtering is handled by htmd handlers)
///
/// # Arguments
/// * `html` - Raw HTML string to process
///
/// # Returns
/// * `Ok(String)` - HTML of the main content container
/// * `Err` - If HTML input exceeds size limit
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
            // Return the container's HTML - element filtering is handled by htmd handlers
            return Ok(element.html());
        }
    }

    // If no main content container found, try to extract body
    if let Some(body) = document.select(&BODY_SELECTOR).next() {
        return Ok(body.html());
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
    fn test_extracts_main_element() -> Result<()> {
        let html = r"
            <html>
                <body>
                    <nav>Navigation</nav>
                    <main><p>Main content</p></main>
                    <footer>Footer</footer>
                </body>
            </html>
        ";
        let result = extract_main_content(html)?;
        // Returns the <main> element HTML (element filtering done by htmd handlers)
        assert!(result.contains("<main>"));
        assert!(result.contains("Main content"));
        Ok(())
    }

    #[test]
    fn test_extracts_article_element() -> Result<()> {
        let html = r"
            <html>
                <body>
                    <nav>Navigation</nav>
                    <article><p>Article content</p></article>
                </body>
            </html>
        ";
        let result = extract_main_content(html)?;
        assert!(result.contains("<article>"));
        assert!(result.contains("Article content"));
        Ok(())
    }

    #[test]
    fn test_main_takes_priority_over_article() -> Result<()> {
        let html = r"
            <html>
                <body>
                    <article><p>Article</p></article>
                    <main><p>Main</p></main>
                </body>
            </html>
        ";
        let result = extract_main_content(html)?;
        // <main> should be selected, not <article>
        assert!(result.contains("<main>"));
        assert!(result.contains("Main"));
        Ok(())
    }

    #[test]
    fn test_body_fallback() -> Result<()> {
        let html = r"
            <html>
                <body>
                    <div>No semantic container</div>
                    <p>Just body content</p>
                </body>
            </html>
        ";
        let result = extract_main_content(html)?;
        assert!(result.contains("<body>"));
        assert!(result.contains("Just body content"));
        Ok(())
    }

    #[test]
    fn test_raw_html_fallback() -> Result<()> {
        let html = "<p>Malformed HTML without body</p>";
        let result = extract_main_content(html)?;
        assert_eq!(result, html);
        Ok(())
    }

    #[test]
    fn test_content_class_selector() -> Result<()> {
        let html = r#"
            <html>
                <body>
                    <div class="content"><p>Content div</p></div>
                </body>
            </html>
        "#;
        let result = extract_main_content(html)?;
        assert!(result.contains("Content div"));
        Ok(())
    }

    #[test]
    fn test_role_main_selector() -> Result<()> {
        let html = r#"
            <html>
                <body>
                    <div role="main"><p>Role main content</p></div>
                </body>
            </html>
        "#;
        let result = extract_main_content(html)?;
        assert!(result.contains("Role main content"));
        Ok(())
    }

    #[test]
    fn test_preserves_all_elements_in_container() -> Result<()> {
        // This module no longer filters elements - htmd handlers do that
        let html = r"
            <article>
                <nav>Navigation</nav>
                <p>Content</p>
                <footer>Footer</footer>
            </article>
        ";
        let result = extract_main_content(html)?;
        // All elements are preserved - filtering is done by htmd handlers
        assert!(result.contains("Navigation"));
        assert!(result.contains("Content"));
        assert!(result.contains("Footer"));
        Ok(())
    }
}
