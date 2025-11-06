//! HTML cleaning utilities for removing scripts, styles, ads, and non-content elements.
//!
//! This module provides aggressive HTML sanitization by removing:
//! - `<script>` and `<style>` tags and their contents
//! - Inline event handlers (onclick, onload, etc.)
//! - HTML comments
//! - Forms, iframes, and tracking elements
//! - Social media widgets, cookie notices, and advertisements
//! - Hidden elements (display:none, visibility:hidden)
//! - HTML5 semantic elements without markdown equivalents

use anyhow::Result;
use html_escape::decode_html_entities;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

// Re-use the size limit from main_content_extraction
use super::main_content_extraction::MAX_HTML_SIZE;

// ============================================================================
// Regex Patterns for HTML Cleaning
// ============================================================================

// Compile regex patterns once at first use
// These are hardcoded patterns that will never fail to compile

static SCRIPT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<script[^>]*>.*?</script>").expect("SCRIPT_RE: hardcoded regex is valid")
});

static STYLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<style[^>]*>.*?</style>").expect("STYLE_RE: hardcoded regex is valid")
});

static EVENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"on\w+="[^"]*""#).expect("EVENT_RE: hardcoded regex is valid"));

static COMMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--.*?-->").expect("COMMENT_RE: hardcoded regex is valid"));

static FORM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<form[^>]*>.*?</form>").expect("FORM_RE: hardcoded regex is valid")
});

static IFRAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<iframe[^>]*>.*?</iframe>").expect("IFRAME_RE: hardcoded regex is valid")
});

static SOCIAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*class="[^"]*(?:social|share|follow)[^"]*"[^>]*>.*?</div>"#)
        .expect("SOCIAL_RE: hardcoded regex is valid")
});

static COOKIE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<div[^>]*(?:id|class)="[^"]*(?:cookie|popup|modal|overlay)[^"]*"[^>]*>.*?</div>"#,
    )
    .expect("COOKIE_RE: hardcoded regex is valid")
});

static AD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*(?:id|class)="[^"]*(?:ad-|ads-|advertisement)[^"]*"[^>]*>.*?</div>"#)
        .expect("AD_RE: hardcoded regex is valid")
});

// Matches elements with display:none in style attribute (supports single/double quotes)
static HIDDEN_DISPLAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<[^>]+style\s*=\s*["'][^"']*display\s*:\s*none[^"']*["'][^>]*>.*?</[^>]+>"#)
        .expect("HIDDEN_DISPLAY: hardcoded regex is valid")
});

// Matches elements with boolean hidden attribute
static HIDDEN_ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<[^>]+\bhidden(?:\s|>|/|=)[^>]*>.*?</[^>]+>")
        .expect("HIDDEN_ATTR: hardcoded regex is valid")
});

// Matches elements with visibility:hidden in style attribute
static HIDDEN_VISIBILITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<[^>]+style\s*=\s*["'][^"']*visibility\s*:\s*hidden[^"']*["'][^>]*>.*?</[^>]+>"#,
    )
    .expect("HIDDEN_VISIBILITY: hardcoded regex is valid")
});

static DETAILS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<details[^>]*>(.*?)</details>").expect("DETAILS_RE: hardcoded regex is valid")
});

static SEMANTIC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"<(/?)(?:article|section|aside|nav|header|footer|figure|figcaption|mark|time)[^>]*>",
    )
    .expect("SEMANTIC_RE: hardcoded regex is valid")
});

// Special case: needs to be compiled for closure captures in details processing
static SUMMARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<summary[^>]*>(.*?)</summary>").expect("SUMMARY_RE: hardcoded regex is valid")
});

// ============================================================================
// HTML Cleaning Function
// ============================================================================

/// Clean HTML content by removing scripts, styles, ads, tracking, and other non-content elements
///
/// This function performs aggressive cleaning including:
/// - Removing `<script>` and `<style>` tags and their contents
/// - Removing inline event handlers (onclick, onload, etc.)
/// - Removing HTML comments  
/// - Removing `<form>`, `<iframe>` elements
/// - Removing social media widgets, cookie notices, and ads
/// - Removing hidden elements (display:none, visibility:hidden)
/// - Converting `<details>`/`<summary>` to markdown-friendly format
/// - Removing semantic HTML5 elements without markdown equivalents
/// - **Decoding HTML entities** (&amp; → &, &lt; → <, etc.)
///
/// # Arguments
/// * `html` - HTML string to clean
///
/// # Returns
/// * `Ok(String)` - Cleaned HTML string
/// * `Err(anyhow::Error)` - If input exceeds size limit
///
/// # Example
/// ```
/// let html = r#"<div><script>alert('xss')</script><p>Hello &amp; goodbye</p></div>"#;
/// let clean = clean_html_content(html);
/// // Result: <div><p>Hello & goodbye</p></div>
/// ```
pub fn clean_html_content(html: &str) -> Result<String> {
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

    // Use Cow to avoid unnecessary allocations
    // Start with borrowed reference, only allocate when modifications occur
    let result = Cow::Borrowed(html);

    // Remove script tags and their contents
    let result = SCRIPT_RE.replace_all(&result, "");

    // Remove style tags and their contents
    let result = STYLE_RE.replace_all(&result, "");

    // Remove inline event handlers
    let result = EVENT_RE.replace_all(&result, "");

    // Remove comments
    let result = COMMENT_RE.replace_all(&result, "");

    // Remove forms
    let result = FORM_RE.replace_all(&result, "");

    // Remove iframes
    let result = IFRAME_RE.replace_all(&result, "");

    // Remove social media widgets and buttons
    let result = SOCIAL_RE.replace_all(&result, "");

    // Remove cookie notices and popups
    let result = COOKIE_RE.replace_all(&result, "");

    // Remove ads
    let result = AD_RE.replace_all(&result, "");

    // Remove hidden elements (multiple patterns for comprehensive matching)
    let result = HIDDEN_DISPLAY.replace_all(&result, "");
    let result = HIDDEN_ATTR.replace_all(&result, "");
    let result = HIDDEN_VISIBILITY.replace_all(&result, "");

    // Handle HTML5 details/summary elements by extracting their content
    // These don't convert well to markdown
    let result = Cow::Owned(
        DETAILS_RE
            .replace_all(&result, |caps: &regex::Captures| {
                let content = &caps[1];
                // Extract summary text if present
                if let Some(summary_match) = SUMMARY_RE.captures(content) {
                    let summary_text = &summary_match[1];
                    let remaining = SUMMARY_RE.replace(content, "");
                    format!(
                        "\n\n**{}**\n\n{}\n\n",
                        summary_text.trim(),
                        remaining.trim()
                    )
                } else {
                    format!("\n\n{}\n\n", content.trim())
                }
            })
            .into_owned(),
    );

    // Remove any remaining HTML5 semantic elements that don't have markdown equivalents
    let result = SEMANTIC_RE.replace_all(&result, "");

    // Decode HTML entities
    let result = decode_html_entities(&result);

    // Convert final Cow to owned String for return
    Ok(result.into_owned())
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_removes_scripts() -> Result<()> {
        let html = r#"<div><script>alert('xss')</script><p>Content</p></div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("script"));
        assert!(!result.contains("alert"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_removes_styles() -> Result<()> {
        let html = r#"<div><style>.test { color: red; }</style><p>Content</p></div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("style"));
        assert!(!result.contains("color: red"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_removes_event_handlers() -> Result<()> {
        let html = r#"<button onclick="alert('click')">Click me</button>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("onclick"));
        assert!(result.contains("Click me"));
        Ok(())
    }

    #[test]
    fn test_removes_comments() -> Result<()> {
        let html = r"<div><!-- This is a comment --><p>Content</p></div>";
        let result = clean_html_content(html)?;
        assert!(!result.contains("This is a comment"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_decodes_html_entities() -> Result<()> {
        let html = r"<p>Hello &amp; goodbye &lt;test&gt;</p>";
        let result = clean_html_content(html)?;
        assert!(result.contains("&"));
        assert!(result.contains("<test>"));
        Ok(())
    }

    #[test]
    fn test_removes_forms() -> Result<()> {
        let html = r#"<div><form><input type="text"></form><p>Content</p></div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("form"));
        assert!(!result.contains("input"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_removes_iframes() -> Result<()> {
        let html = r#"<div><iframe src="ads.html"></iframe><p>Content</p></div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("iframe"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_removes_hidden_elements() -> Result<()> {
        let html = r#"<div style="display:none">Hidden</div><p>Visible</p>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("Hidden"));
        assert!(result.contains("Visible"));
        Ok(())
    }

    #[test]
    fn test_converts_details_summary() -> Result<()> {
        let html = r"<details><summary>Click to expand</summary>Hidden content</details>";
        let result = clean_html_content(html)?;
        assert!(!result.contains("<details>"));
        assert!(!result.contains("<summary>"));
        assert!(result.contains("Click to expand"));
        assert!(result.contains("Hidden content"));
        Ok(())
    }

    #[test]
    fn test_size_limit_enforcement() {
        // Create HTML larger than MAX_HTML_SIZE
        let large_html = "x".repeat(MAX_HTML_SIZE + 1);
        let result = clean_html_content(&large_html);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("HTML input too large"));
    }
}
