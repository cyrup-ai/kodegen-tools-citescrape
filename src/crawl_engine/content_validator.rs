//! Content validation for detecting error pages
//!
//! Simple validation based on HTTP status codes. All heuristic checks
//! (short content, text ratio, pattern matching) have been removed as
//! they caused false positives on legitimate pages.

use log::{debug, warn};

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
    #[must_use]
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            reason: None,
            confidence: 1.0,
        }
    }

    /// Create an invalid result with reason
    #[must_use]
    pub fn invalid(reason: String, confidence: f32) -> Self {
        Self {
            is_valid: false,
            reason: Some(reason),
            confidence,
        }
    }
}

/// Validate page content based on HTTP status code
///
/// Simple validation: HTTP 4xx/5xx = invalid, everything else = valid.
/// All heuristic checks (short content, text ratio, pattern matching)
/// have been removed as they caused false positives on legitimate pages.
///
/// # Arguments
/// * `_html` - Raw HTML content (unused, kept for API compatibility)
/// * `_markdown` - Converted markdown content (unused, kept for API compatibility)
/// * `url` - Page URL (for logging)
/// * `http_status` - HTTP status code from network layer
///
/// # Returns
/// `ContentValidationResult` indicating validity based on HTTP status
#[must_use]
pub fn validate_page_content(
    _html: &str,
    _markdown: &str,
    url: &str,
    http_status: Option<u16>,
) -> ContentValidationResult {
    // Only check: HTTP status code
    if let Some(status) = http_status {
        if status >= 400 {
            warn!("HTTP error status {} for {}", status, url);
            return ContentValidationResult::invalid(
                format!("HTTP error: {status}"),
                0.95,
            );
        }
        debug!("HTTP {} OK for {}", status, url);
    } else {
        debug!("No HTTP status for {}, assuming valid", url);
    }

    ContentValidationResult::valid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_404_rejects() {
        let result = validate_page_content("", "", "https://example.com", Some(404));
        assert!(!result.is_valid);
        assert!(result.reason.unwrap().contains("HTTP error: 404"));
    }

    #[test]
    fn test_http_500_rejects() {
        let result = validate_page_content("", "", "https://example.com", Some(500));
        assert!(!result.is_valid);
        assert!(result.reason.unwrap().contains("HTTP error: 500"));
    }

    #[test]
    fn test_http_200_accepts() {
        let result = validate_page_content("", "", "https://example.com", Some(200));
        assert!(result.is_valid);
    }

    #[test]
    fn test_http_200_accepts_short_content() {
        let result = validate_page_content("<html>Hi</html>", "Hi", "https://example.com", Some(200));
        assert!(result.is_valid);
    }

    #[test]
    fn test_http_200_accepts_empty_content() {
        let result = validate_page_content("", "", "https://example.com", Some(200));
        assert!(result.is_valid);
    }

    #[test]
    fn test_no_status_assumes_valid() {
        let result = validate_page_content("", "", "https://example.com", None);
        assert!(result.is_valid);
    }

    #[test]
    fn test_http_301_accepts() {
        let result = validate_page_content("", "", "https://example.com", Some(301));
        assert!(result.is_valid);
    }
}
