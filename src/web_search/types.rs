//! Data structures and constants for web search functionality

use serde::{Deserialize, Serialize};

// =============================================================================
// Constants
// =============================================================================

/// `DuckDuckGo` search URL base
pub const SEARCH_URL: &str = "https://duckduckgo.com";

/// CSS selector for individual search results
/// `DuckDuckGo` uses article elements with data-testid="result"
pub const SEARCH_RESULT_SELECTOR: &str = "article[data-testid='result']";

/// CSS selector for the title/link element in DuckDuckGo search results
///
/// In DuckDuckGo's DOM structure, title and URL share the same element:
/// ```html
/// <h2><a href="https://example.com">Page Title</a></h2>
/// ```
///
/// Extract title via `.inner_text()` and URL via `.attribute("href")`.
pub const TITLE_LINK_SELECTOR: &str = "h2 > a";

/// CSS selector for result snippets/descriptions
/// `DuckDuckGo` uses div with data-result="snippet" attribute
pub const SNIPPET_SELECTOR: &str = "div[data-result='snippet']";

/// Maximum number of retry attempts
pub const MAX_RETRIES: u32 = 3;

/// Maximum number of results to extract
pub const MAX_RESULTS: usize = 10;

/// Polling interval in milliseconds for waiting on DOM elements
///
/// 100ms provides good responsiveness without excessive CDP overhead.
/// Used by both perform_search and wait_for_results polling loops.
pub const POLL_INTERVAL_MS: u64 = 100;

/// Maximum query length for DuckDuckGo searches
///
/// DuckDuckGo's practical query limit is approximately 1000 characters.
/// Longer queries may be truncated or cause navigation errors.
///
/// Conservative limit based on:
/// - Browser URL length limits (~2048 chars total)
/// - DuckDuckGo URL structure overhead (~50 chars)
/// - URL encoding expansion (up to 3x for special characters)
/// - Safe buffer for reliable operation
pub const MAX_QUERY_LENGTH: usize = 1000;

// =============================================================================
// Data Structures
// =============================================================================

/// A single search result with rank, title, URL, and snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Result ranking (1-indexed)
    pub rank: usize,

    /// Page title
    pub title: String,

    /// Page URL
    pub url: String,

    /// Description snippet from search results
    pub snippet: String,
}



/// Collection of search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// Search query that produced these results
    pub query: String,

    /// List of search results
    pub results: Vec<SearchResult>,
}

impl SearchResults {
    /// Create new `SearchResults`
    #[must_use]
    pub fn new(query: String, results: Vec<SearchResult>) -> Self {
        Self { query, results }
    }


}
