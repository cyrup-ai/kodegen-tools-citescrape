//! Content validation for detecting error pages and invalid extractions
//!
//! This module provides comprehensive validation to prevent saving error pages,
//! placeholder content, or incompletely loaded pages during web crawling.

use log::{debug, warn};
use regex::Regex;
use std::sync::LazyLock;

/// Result of content validation
#[derive(Debug, Clone)]
pub struct ContentValidationResult {
    /// Whether the content passed validation
    pub is_valid: bool,
    /// Reason for validation failure (if any)
    pub reason: Option<String>,
    /// Confidence score (0.0 = definitely invalid, 1.0 = definitely valid)
    pub confidence: f32,
}

impl ContentValidationResult {
    /// Create a valid result
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            reason: None,
            confidence: 1.0,
        }
    }

    /// Create an invalid result with reason
    pub fn invalid(reason: String, confidence: f32) -> Self {
        Self {
            is_valid: false,
            reason: Some(reason),
            confidence,
        }
    }
}

/// Comprehensive content validation checking multiple error indicators
///
/// # Arguments
/// * `html` - Raw HTML content from page extraction
/// * `markdown` - Converted markdown content
/// * `url` - Page URL (for logging)
/// * `http_status` - HTTP status code from network layer (if available)
///
/// # Returns
/// `ContentValidationResult` indicating validity and reason for any failures
pub fn validate_page_content(
    html: &str,
    markdown: &str,
    url: &str,
    http_status: Option<u16>,
) -> ContentValidationResult {
    // 1. PRIMARY CHECK: HTTP Status Code (if available)
    if let Some(status) = http_status {
        if status >= 400 {
            warn!("HTTP error status {} for {}", status, url);
            return ContentValidationResult::invalid(
                format!("HTTP error: {}", status),
                0.95, // High confidence - actual server error
            );
        }
        // If status is 2xx/3xx, skip text pattern matching entirely
        debug!("HTTP status {} OK for {}, skipping text pattern checks", status, url);
    } else {
        // HTTP status unavailable - proceed to secondary checks
        debug!("HTTP status unavailable for {}, using secondary validation", url);
        
        // 2. SECONDARY CHECK: Framework-Specific Error Indicators
        if let Some(error_pattern) = detect_client_side_errors(html, markdown) {
            warn!("Client-side error detected in {}: {}", url, error_pattern);
            return ContentValidationResult::invalid(
                format!("Client-side error: {}", error_pattern),
                0.85, // High confidence but lower than HTTP status
            );
        }
    }

    // 2. Check content length (suspiciously short content)
    let markdown_stripped = strip_markdown_formatting(markdown);
    if markdown_stripped.len() < 100 {
        warn!("Suspiciously short content for {}: {} bytes", url, markdown_stripped.len());
        return ContentValidationResult::invalid(
            format!("Content too short: {} bytes", markdown_stripped.len()),
            0.8,
        );
    }

    // 3. Check for minimal text content (mostly HTML tags, little actual text)
    let text_ratio = calculate_text_to_html_ratio(html);
    if text_ratio < 0.05 {
        warn!("Low text-to-HTML ratio for {}: {:.2}%", url, text_ratio * 100.0);
        return ContentValidationResult::invalid(
            format!("Low text ratio: {:.2}%", text_ratio * 100.0),
            0.7,
        );
    }

    // 4. Check for empty or whitespace-only content
    if markdown.trim().is_empty() {
        warn!("Empty content after markdown conversion for {}", url);
        return ContentValidationResult::invalid(
            "Empty content".to_string(),
            1.0, // Definitely invalid
        );
    }

    debug!("Content validation passed for {}", url);
    ContentValidationResult::valid()
}

/// Detect client-side errors that don't have HTTP status codes
///
/// Only checks for framework-specific error indicators, NOT generic HTTP status phrases.
/// This is a secondary check used when HTTP status is unavailable.
fn detect_client_side_errors(html: &str, markdown: &str) -> Option<String> {
    // Framework-specific error patterns (high precision)
    const CLIENT_ERROR_PATTERNS: &[&str] = &[
        // Next.js client-side errors
        "Application error: a client-side exception has occurred",
        "Application error: a server-side exception has occurred",
        
        // React error boundaries
        "Unhandled Runtime Error",
        "Runtime Error",
        
        // Generic but specific enough
        "This page could not be found", // Next.js 404 page text
    ];

    // Check text content
    for pattern in CLIENT_ERROR_PATTERNS {
        if html.contains(pattern) || markdown.contains(pattern) {
            return Some(pattern.to_string());
        }
    }

    // Check for Next.js error object in page data
    if html.contains("__NEXT_DATA__") && html.contains("\"err\"") {
        return Some("Next.js error object detected".to_string());
    }

    // Check for error-specific HTML structure
    const ERROR_HTML_INDICATORS: &[&str] = &[
        "class=\"error-page\"",
        "id=\"error-boundary\"",
        "data-testid=\"error-page\"",
    ];

    for indicator in ERROR_HTML_INDICATORS {
        if html.contains(indicator) {
            return Some(format!("Error HTML structure found: {}", indicator));
        }
    }

    None
}

/// Calculate ratio of actual text content to HTML markup
fn calculate_text_to_html_ratio(html: &str) -> f32 {
    let total_len = html.len();
    if total_len == 0 {
        return 0.0;
    }

    // Strip HTML tags and count remaining text
    let text_len = strip_html_tags(html).len();
    
    text_len as f32 / total_len as f32
}

/// Strip HTML tags from content (simple tag removal)
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    
    result
}

/// Matches fenced code blocks (```...```) to remove them entirely from content measurement
/// Code blocks shouldn't count toward readable content length
static CODE_BLOCK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"```[\s\S]*?```")
        .expect("CODE_BLOCK_REGEX: hardcoded regex is valid")
});

/// Matches markdown images ![alt text](url) and extracts only the alt text
/// Images contribute alt text to content, but not the URL
static IMAGE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\[([^\]]*)\]\([^)]*\)")
        .expect("IMAGE_REGEX: hardcoded regex is valid")
});

/// Matches markdown links [text](url) and extracts only the link text
/// Links contribute link text to content, but not the URL
static LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[([^\]]*)\]\([^)]*\)")
        .expect("LINK_REGEX: hardcoded regex is valid")
});

/// Strip markdown formatting to get raw text length for content validation
/// 
/// Removes all markdown syntax elements to measure actual readable content:
/// - Code blocks (```code```) - removed entirely
/// - Images (![alt](url)) - alt text only
/// - Links ([text](url)) - link text only (URL completely removed)
/// - Headers (#), bold (*), italic (_), inline code (`)
///
/// Order matters: code blocks removed first to avoid processing markdown-like syntax inside code
fn strip_markdown_formatting(markdown: &str) -> String {
    // Step 1: Remove code blocks entirely (they shouldn't count as readable content)
    let text = CODE_BLOCK_REGEX.replace_all(markdown, "");
    
    // Step 2: Replace images with alt text only (remove URL portion)
    let text = IMAGE_REGEX.replace_all(&text, "$1");
    
    // Step 3: Replace links with link text only (remove URL portion)
    let text = LINK_REGEX.replace_all(&text, "$1");
    
    // Step 4: Remove remaining markdown formatting symbols line by line
    text.lines()
        .map(|line| {
            line.trim_start_matches('#')  // Remove heading markers
                .trim()
                .replace(['*', '_', '`'], "")  // Remove bold/italic/code markers
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_status_400_rejects() {
        let html = "<html><body>Any content</body></html>";
        let markdown = "Any content";
        
        let result = validate_page_content(html, markdown, "https://example.com", Some(404));
        assert!(!result.is_valid);
        assert_eq!(result.confidence, 0.95);
        assert!(result.reason.unwrap().contains("HTTP error: 404"));
    }

    #[test]
    fn test_http_status_200_with_text_404_passes() {
        let html = r#"
            <html><body>
                <h1>Documentation</h1>
                <p>When your API returns a 404 Not Found error, the client should handle it gracefully.</p>
                <p>Example: "Page not found" is displayed when the route doesn't exist.</p>
                <p>This documentation page contains substantial content explaining error handling patterns and best practices for web applications.</p>
            </body></html>
        "#;
        let markdown = "# Documentation\n\nWhen your API returns a 404 Not Found error, the client should handle it gracefully.\n\nExample: Page not found is displayed when the route doesn't exist.\n\nThis documentation page contains substantial content explaining error handling patterns and best practices for web applications.";
        
        // HTTP 200 + text containing "404 Not Found" = VALID (no false positive!)
        let result = validate_page_content(html, markdown, "https://example.com", Some(200));
        assert!(result.is_valid); // ✅ Passes despite containing "404 Not Found" text
    }

    #[test]
    fn test_no_http_status_with_nextjs_error_rejects() {
        let html = r#"Application error: a client-side exception has occurred</html>"#;
        let markdown = "Application error: a client-side exception has occurred";
        
        let result = validate_page_content(html, markdown, "https://example.com", None);
        assert!(!result.is_valid);
        assert!(result.reason.unwrap().contains("Client-side error"));
    }

    #[test]
    fn test_no_http_status_with_valid_content_passes() {
        let html = "<html><body><h1>Welcome</h1><p>This is legitimate content without errors. It contains substantial information about the topic and provides valuable insights for readers seeking to understand the subject matter.</p></body></html>";
        let markdown = "# Welcome\n\nThis is legitimate content without errors. It contains substantial information about the topic and provides valuable insights for readers seeking to understand the subject matter.";
        
        let result = validate_page_content(html, markdown, "https://example.com", None);
        assert!(result.is_valid);
    }

    #[test]
    fn test_http_status_500_rejects() {
        let html = "<html><body>Server Error</body></html>";
        let markdown = "Server Error";
        
        let result = validate_page_content(html, markdown, "https://example.com", Some(500));
        assert!(!result.is_valid);
        assert!(result.reason.unwrap().contains("HTTP error: 500"));
    }

    #[test]
    fn test_detects_short_content() {
        let html = "<html><body>Hi</body></html>";
        let markdown = "Hi";
        
        let result = validate_page_content(html, markdown, "https://example.com", None);
        assert!(!result.is_valid);
        assert!(result.reason.unwrap().contains("too short"));
    }

    #[test]
    fn test_accepts_valid_content() {
        let html = "<html><body><h1>Welcome</h1><p>This is a real page with substantial content that should pass validation. It has multiple sentences and enough text to be considered valid content.</p></body></html>";
        let markdown = "# Welcome\n\nThis is a real page with substantial content that should pass validation. It has multiple sentences and enough text to be considered valid content.";
        
        let result = validate_page_content(html, markdown, "https://example.com", Some(200));
        assert!(result.is_valid);
    }
}
