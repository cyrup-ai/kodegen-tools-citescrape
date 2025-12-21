//! Web search functionality using browser automation
//!
//! Performs DuckDuckGo searches using pre-warmed browsers from the pool.
//! Returns structured results with titles, URLs, and snippets.

mod search;
mod types;

// Re-export public types
pub use types::{MAX_RESULTS, MAX_RETRIES, SearchResult, SearchResults};

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
    info!("Starting web search for query: {}", query);

    // Acquire pre-warmed browser from pool
    let guard = pool.acquire().await
        .context("Failed to acquire browser from pool")?;
    let browser_arc = guard.browser_arc();
    
    info!("Acquired pre-warmed browser from pool (id={})", guard.id());

    // Create blank page for stealth injection
    let page = browser_arc
        .new_page("about:blank")
        .await
        .context("Failed to create blank page")?;

    // Perform search with retry logic
    let results = search::retry_with_backoff(
        || async {
            search::perform_search(&page, &query).await?;
            search::wait_for_results(&page).await?;
            search::extract_results(&page).await
        },
        MAX_RETRIES,
    )
    .await?;

    info!("Search completed successfully with {} results", results.len());

    // Clean up page
    if let Err(e) = page.close().await {
        tracing::warn!("Failed to close search page: {}", e);
    }

    Ok(SearchResults::new(query, results))
    // Browser automatically returns to pool when guard drops
}
