//! Core search execution logic
//!
//! Handles performing searches, waiting for results, and extracting data
//! from search result pages.

use anyhow::{Context, Result, anyhow};
use chromiumoxide::page::Page;
use once_cell::sync::Lazy;
use rand::Rng;
use regex::Regex;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use url::Url;

use super::types::{
    MAX_RESULTS, POLL_INTERVAL_MS, SEARCH_RESULT_SELECTOR, SEARCH_URL, SNIPPET_SELECTOR,
    SearchResult, TITLE_LINK_SELECTOR,
};
use super::page_helpers::get_page_url_with_fallback;

/// Perform a search with kromekover stealth injection
///
/// Applies kromekover stealth features to the page before navigating to `DuckDuckGo`,
/// then navigates directly to the search results URL. `DuckDuckGo` uses React-based
/// rendering, so we wait for results to appear after navigation.
///
/// # Arguments
/// * `page` - Blank page instance to enhance and use for search
/// * `query` - Search query string
///
/// # Based on
/// - packages/citescrape/src/crawl_engine/core.rs:231-259 (stealth pattern)
/// - `DuckDuckGo` DOM analysis from `tasks/DUCK_DUCK_GO_DOM_PARSING.md`
pub async fn perform_search(page: &Page, query: &str) -> Result<()> {
    // Apply kromekover stealth injection BEFORE navigation - FAIL on error
    // Stealth is CRITICAL for web search (DuckDuckGo will CAPTCHA without it)
    info!("Applying kromekover stealth injection");
    
    tokio::time::timeout(
        Duration::from_secs(5),
        crate::kromekover::inject(page),
    )
    .await
    .context("Stealth injection timeout after 5s")?
    .context("Stealth injection failed")?;
    
    info!("Stealth injection complete");

    // Set viewport for consistent desktop rendering (extracted from page_enhancer)
    use chromiumoxide::cdp;
    page.execute(
        cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams::builder()
            .width(1920)
            .height(1080)
            .device_scale_factor(1.0)
            .mobile(false)
            .build()
            .map_err(anyhow::Error::msg)?,
    )
    .await
    .context("Failed to set viewport dimensions")?;

    // Navigate directly to DuckDuckGo search results with proper URL encoding
    let mut search_url = Url::parse(SEARCH_URL).context("Failed to parse DuckDuckGo base URL")?;
    search_url
        .query_pairs_mut()
        .append_pair("q", query)
        .append_pair("ia", "web");

    info!("Navigating to DuckDuckGo search: {}", search_url);
    page.goto(search_url.as_str())
        .await
        .context("Failed to navigate to DuckDuckGo")?;

    // DuckDuckGo uses React for rendering - wait for initial load
    page.wait_for_navigation()
        .await
        .context("Failed to wait for initial page load")?;

    // Smart wait: Poll for results instead of fixed 3s delay
    // This is faster when results load quickly, but waits up to 5s if needed
    let poll_start = Instant::now();
    let max_wait = Duration::from_secs(5);
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

    info!("Waiting for React to render search results...");
    loop {
        // Check if results are present
        if page.find_element(SEARCH_RESULT_SELECTOR).await.is_ok() {
            let elapsed = poll_start.elapsed();
            debug!(
                "Search results appeared after {:.2}s",
                elapsed.as_secs_f64()
            );
            break;
        }

        // Check timeout
        if poll_start.elapsed() >= max_wait {
            // Check if we got a CAPTCHA or error page
            let url = get_page_url_with_fallback(page).await;
            if url.contains("/sorry/") || url.contains("captcha") {
                return Err(anyhow!(
                    "DuckDuckGo presented a CAPTCHA page. Try again later or use a different network."
                ));
            }

            return Err(anyhow!(
                "Timeout waiting for DuckDuckGo results to render. \
                 React took longer than {}s to load results. \
                 This may indicate network issues or DuckDuckGo changes.",
                max_wait.as_secs()
            ));
        }

        // Wait before next poll
        tokio::time::sleep(poll_interval).await;
    }

    Ok(())
}

/// Extract search results from the page
///
/// Extracts title, URL, and snippet for each result up to `MAX_RESULTS`.
/// Uses fail-fast approach: URLs must exist (critical), titles use fallback text,
/// snippets gracefully default to "No description available".
///
/// # Arguments
/// * `page` - Page containing search results
///
/// # Returns
/// Vector of `SearchResult` structs
///
/// # Based on
/// - packages/citescrape/src/google_search.rs:208-263
///
/// # Note
/// Extraction logic is inlined because `chromiumoxide::Element` doesn't implement
/// Clone, making it difficult to reuse elements across multiple extraction calls.
pub async fn extract_results(page: &Page) -> Result<Vec<SearchResult>> {
    let search_results = page
        .find_elements(SEARCH_RESULT_SELECTOR)
        .await
        .context("Failed to find search results")?;

    info!("Found {} search results", search_results.len());

    // Fail fast if no results (likely CAPTCHA, error page, or DuckDuckGo DOM change)
    if search_results.is_empty() {
        // Get current URL for diagnostics
        let url = get_page_url_with_fallback(page).await;

        // Check for known error conditions
        if url.contains("/sorry/") || url.contains("captcha") {
            return Err(anyhow!(
                "DuckDuckGo CAPTCHA detected. No search results available. \
                 Try again later or use a different network connection."
            ));
        }

        // Check if this might be a "no results" page from DDG
        if let Ok(body) = page.find_element("body").await
            && let Ok(Some(text)) = body.inner_text().await
            && text.to_lowercase().contains("no results")
        {
            return Err(anyhow!(
                "DuckDuckGo returned zero results for this query. \
                 Try a different search term."
            ));
        }

        return Err(anyhow!(
            "No search results found on DuckDuckGo. This may indicate:\n\
             • DuckDuckGo DOM structure changed (selector '{SEARCH_RESULT_SELECTOR}' not found)\n\
             • Network/connection issues\n\
             • DuckDuckGo is temporarily unavailable\n\
             Current URL: {url}"
        ));
    }

    let mut results = Vec::new();

    for (index, result) in search_results.into_iter().enumerate().take(MAX_RESULTS) {
        // Find title/link element ONCE (contains both title text and href)
        let title_link = result
            .find_element(TITLE_LINK_SELECTOR)
            .await
            .with_context(|| {
                format!(
                    "DuckDuckGo result {}: Title/link element not found with selector '{}'. \
                     DOM structure may have changed.",
                    index + 1,
                    TITLE_LINK_SELECTOR
                )
            })?;

        // Extract title from the element
        let title = title_link
            .inner_text()
            .await
            .with_context(|| format!("Failed to get title text for result {}", index + 1))?
            .unwrap_or_else(|| format!("Untitled Result {}", index + 1));

        // Extract URL from the SAME element (no additional DOM query)
        let url = title_link
            .attribute("href")
            .await
            .with_context(|| format!("Failed to get href attribute for result {}", index + 1))?
            .ok_or_else(|| {
                anyhow!(
                    "DuckDuckGo result {}: Link href attribute is empty. \
                     This shouldn't happen - may indicate a DuckDuckGo UI change.",
                    index + 1
                )
            })?;

        // Extract snippet - separate element, unchanged
        let snippet = match result.find_element(SNIPPET_SELECTOR).await {
            Ok(el) => el
                .inner_text()
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "No description available".to_string()),
            Err(_) => "No description available".to_string(),
        };

        results.push(SearchResult {
            rank: index + 1,
            title,
            url,
            snippet,
        });
    }

    Ok(results)
}

/// Regex patterns for permanent browser errors (non-retryable)
///
/// These indicate the browser/page/session state is broken and cannot recover.
/// Patterns are compiled once at first use via `once_cell::sync::Lazy`.
static PERMANENT_ERROR_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Browser lifecycle errors
        Regex::new(r"browser (closed|disconnected|crashed)").unwrap(),
        Regex::new(r"page (closed|crashed)").unwrap(),
        Regex::new(r"target (closed|crashed|destroyed)").unwrap(),
        // Session/connection errors
        Regex::new(r"session (not found|closed|disconnected)").unwrap(),
        Regex::new(r"no response from.*chromium").unwrap(),
        Regex::new(r"channel (closed|disconnected|error)").unwrap(),
        // CDP/WebSocket specific
        Regex::new(r"websocket (closed|error|disconnected)").unwrap(),
        Regex::new(r"cdp.*(disconnect|closed)").unwrap(),
        // Frame/DOM errors
        Regex::new(r"frame.*not found").unwrap(),
        // Anti-bot detection
        Regex::new(r"captcha").unwrap(),
    ]
});

/// Regex patterns for transient errors (retryable)
///
/// These indicate temporary failures that may succeed on retry.
/// Patterns are compiled once at first use via `once_cell::sync::Lazy`.
static RETRYABLE_ERROR_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Stealth injection failures - RETRYABLE (fresh page can succeed)
        // These errors indicate transient CDP or script loading issues
        Regex::new(r"stealth injection").unwrap(),
        // Timeout errors
        Regex::new(r"timeout|timed out").unwrap(),
        // Network errors (specific, not just "network")
        Regex::new(r"network (error|timeout|unreachable)").unwrap(),
        Regex::new(r"connection (refused|reset|closed)").unwrap(),
        Regex::new(r"dns.*(fail|error|timeout)").unwrap(),
        // Rate limiting
        Regex::new(r"rate limit|too many requests").unwrap(),
        Regex::new(r"(http[: ])?429").unwrap(),
        // Temporary resource issues
        Regex::new(r"temporarily unavailable").unwrap(),
    ]
});

/// Classify errors into retryable vs permanent failures
///
/// Uses regex pattern matching for precise error classification.
/// Logs classification decisions for debugging.
///
/// # Permanent Errors (return false)
/// - Browser/page closed or disconnected
/// - Session terminated
/// - Frame/target not found
/// - CAPTCHA detected
/// - WebSocket/CDP connection lost
///
/// # Transient Errors (return true)
/// - Network timeouts
/// - Connection refused (temporary)
/// - Rate limiting (429)
/// - DNS failures
///
/// # Unknown Errors (return false)
/// - Fail fast on unknown errors (conservative approach)
/// - Logs warning to help identify patterns to add
fn is_retryable_error(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();

    // Check permanent patterns first (non-retryable)
    for pattern in PERMANENT_ERROR_PATTERNS.iter() {
        if pattern.is_match(&error_str) {
            tracing::debug!(
                error = %error_str,
                pattern = %pattern.as_str(),
                "Classified as permanent (non-retryable)"
            );
            return false;
        }
    }

    // Check retryable patterns
    for pattern in RETRYABLE_ERROR_PATTERNS.iter() {
        if pattern.is_match(&error_str) {
            tracing::debug!(
                error = %error_str,
                pattern = %pattern.as_str(),
                "Classified as transient (retryable)"
            );
            return true;
        }
    }

    // Unknown errors - fail fast (truly conservative approach)
    // Log at warn level so developers can identify patterns to add
    tracing::warn!(
        error = %error_str,
        "Unknown error classification - failing fast. \
         If this should be retryable, add pattern to RETRYABLE_ERROR_PATTERNS in search.rs"
    );
    false
}

/// Retry a search operation with exponential backoff and error classification
///
/// Only retries errors that are likely transient (timeouts, network issues).
/// Fails fast on permanent errors (browser crashes, page closed).
///
/// # Arguments
/// * `f` - Async function to retry
/// * `max_retries` - Maximum number of retry attempts
///
/// # Returns
/// Result of the operation or final error after all retries
///
/// # Based on
/// - packages/citescrape/src/google_search.rs:275-299
pub async fn retry_with_backoff<F, Fut, T>(f: F, max_retries: u32) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut retries = 0;
    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                // Check if error is retryable BEFORE checking retry limit
                if !is_retryable_error(&e) {
                    warn!("Non-retryable error encountered, failing fast: {:?}", e);
                    return Err(e);
                }

                if retries >= max_retries {
                    warn!("Max retries ({}) exceeded: {:?}", max_retries, e);
                    return Err(e);
                }

                let delay = 2u64.pow(retries) * 1000 + rand::rng().random_range(0..1000);
                warn!(
                    "Retryable error, attempt {}/{}, retrying in {}ms: {:?}",
                    retries + 1,
                    max_retries,
                    delay,
                    e
                );
                tokio::time::sleep(Duration::from_millis(delay)).await;
                retries += 1;
            }
        }
    }
}
