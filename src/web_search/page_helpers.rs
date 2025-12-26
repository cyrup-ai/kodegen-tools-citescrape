//! Helper functions for consistent chromiumoxide Page error handling
//!
//! These functions provide standardized error handling for common Page operations
//! that may fail (browser communication errors) or return None (value not yet available).

use chromiumoxide::page::Page;
use tracing::trace;

/// Get page URL with diagnostic fallback
///
/// Handles two failure modes:
/// 1. `Err(e)` - Browser communication failure (logs at trace level)
/// 2. `Ok(None)` - Page has no URL yet (treated as empty)
///
/// Returns `"about:blank"` on any failure for clear diagnostics.
///
/// # Why "about:blank" instead of ""?
/// - More descriptive than empty string in logs
/// - Valid URL format for debugging
/// - Clearly indicates "no real URL" vs empty string bugs
///
/// # Example
/// ```rust
/// let url = get_page_url_with_fallback(&page).await;
/// if url.contains("/captcha") {
///     // Handle CAPTCHA...
/// }
/// ```
pub async fn get_page_url_with_fallback(page: &Page) -> String {
    match page.url().await {
        Ok(Some(url)) => url,
        Ok(None) => {
            trace!("Page URL is None (page not yet navigated)");
            "about:blank".to_string()
        }
        Err(e) => {
            trace!("Failed to get page URL (browser communication error): {}", e);
            "about:blank".to_string()
        }
    }
}

/// Get page content HTML for debugging purposes
///
/// This is explicitly for diagnostic/debugging use cases where content
/// may not be available. Returns `None` on any failure and logs diagnostic info.
///
/// # Use Cases
/// - Saving debug snapshots on timeout
/// - Logging page state for error analysis
/// - CAPTCHA detection diagnostics
///
/// # Example
/// ```rust
/// if let Some(html) = get_page_content_for_debug(&page).await {
///     tokio::fs::write("/tmp/debug.html", &html).await?;
/// }
/// ```
#[allow(dead_code)]
pub async fn get_page_content_for_debug(page: &Page) -> Option<String> {
    match page.content().await {
        Ok(html) => Some(html),
        Err(e) => {
            trace!("Failed to get page content for debugging: {}", e);
            None
        }
    }
}

/// Get element inner text with fallback
///
/// For extracting text from elements where absence is acceptable
/// (e.g., optional snippets, descriptions).
///
/// Returns provided fallback string on any failure.
///
/// # Example
/// ```rust
/// let snippet = get_element_text_with_fallback(
///     &element,
///     "No description available"
/// ).await;
/// ```
#[allow(dead_code)]
pub async fn get_element_text_with_fallback(
    element: &chromiumoxide::element::Element,
    fallback: &str,
) -> String {
    element
        .inner_text()
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| fallback.to_string())
}
