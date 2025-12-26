//! Web search functionality using browser automation
//!
//! Performs DuckDuckGo searches using pre-warmed browsers from the pool.
//! Returns structured results with titles, URLs, and snippets.

mod search;
mod types;
mod page_helpers;

// Re-export public types
pub use types::{MAX_QUERY_LENGTH, MAX_RESULTS, MAX_RETRIES, SearchResult, SearchResults};

use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::info;

/// Perform web search using BrowserPool
///
/// Acquires a pre-warmed browser from the pool, performs the search,
/// and automatically returns the browser to the pool when done.
///
/// # Arguments
/// * `pool` - Shared browser pool reference
/// * `query` - Search query string
///
/// # Returns
/// `SearchResults` with up to 10 results containing titles, URLs, and snippets
///
/// # Example
/// ```no_run
/// use kodegen_tools_citescrape::{BrowserPool, BrowserPoolConfig};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = BrowserPool::new(BrowserPoolConfig::default());
///     pool.start().await?;
///     
///     let results = kodegen_tools_citescrape::web_search::search_with_pool(
///         &pool,
///         "rust async programming"
///     ).await?;
///     
///     println!("Found {} results", results.results.len());
///     pool.shutdown().await?;
///     Ok(())
/// }
/// ```
pub async fn search_with_pool(
    pool: &Arc<crate::browser_pool::BrowserPool>,
    query: impl Into<String>,
) -> Result<SearchResults> {
    let query = query.into();
    
    // Validate query before acquiring browser resources
    let trimmed_query = query.trim();
    
    // Check for empty query
    if trimmed_query.is_empty() {
        anyhow::bail!(
            "Search query cannot be empty or whitespace-only. \
             Please provide a valid search term."
        );
    }
    
    // Check query length
    if trimmed_query.len() > types::MAX_QUERY_LENGTH {
        anyhow::bail!(
            "Search query is too long ({} characters). \
             Maximum allowed: {} characters. \
             Please shorten your query.",
            trimmed_query.len(),
            types::MAX_QUERY_LENGTH
        );
    }
    
    // Convert to owned String for use in closure
    let query = trimmed_query.to_string();
    
    info!("Starting web search for query: '{}' ({} chars)", query, query.len());

    // Acquire pre-warmed browser from pool
    let guard = pool.acquire().await
        .context("Failed to acquire browser from pool")?;
    
    info!("Acquired pre-warmed browser from pool (id={})", guard.id());

    // Get browser reference for use in retry closure
    let browser = guard.browser();

    // Perform search with retry logic - fresh page per attempt
    // Each retry creates a new page to ensure clean stealth injection state
    let results = search::retry_with_backoff(
        || {
            let query = query.clone();
            async move {
                // Create fresh page for each attempt (critical for stealth retry)
                // PageGuard ensures page.close() is called on ANY exit path
                let page_guard = crate::crawl_engine::page_processor::PageGuard::new(
                    browser.new_page("about:blank").await
                        .context("Failed to create blank page")?,
                    format!("web_search:{}", query),
                );
                
                // Execute search operations
                search::perform_search(&page_guard, &query).await?;
                search::extract_results(&page_guard).await
                // PageGuard dropped here - spawns async page.close()
            }
        },
        MAX_RETRIES,
    )
    .await?;

    info!("Search completed successfully with {} results", results.len());

    // page_guard dropped here on success path
    // Drop::drop spawns async page.close() - guaranteed cleanup
    Ok(SearchResults::new(query, results))
    // Browser automatically returns to pool when guard drops
}
