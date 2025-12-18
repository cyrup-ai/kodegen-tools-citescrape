//! MCP (Model Context Protocol) Tools for Web Crawling
//!
//! This module provides TWO production-ready MCP tools that enable LLMs to autonomously
//! crawl websites and search crawled documentation using Tantivy full-text search.
//!
//! ## Tools
//!
//! ### `scrape_url` (Long-Running)
//! Executes a complete web crawl that blocks until finished, returning comprehensive
//! results. This replaces the old polling pattern (start_crawl + get_crawl_results).
//!
//! **Features:**
//! - Long-running execution (blocks until completion or timeout)
//! - Automatic Tantivy search indexing
//! - Rate limiting (default 2 req/sec)
//! - Multiple output formats: Markdown, HTML, JSON
//! - Screenshot capture support
//! - Configurable timeout (default 600s = 10 minutes)
//! - Partial results on timeout
//!
//! ### `scrape_search_results`
//! Full-text search across crawled documentation with advanced query syntax.
//!
//! **Query Types:**
//! - Text: `layout components` (searches all fields)
//! - Phrase: `"exact phrase"` (exact match)
//! - Boolean: `layout AND (components OR widgets)`
//! - Field: `title:layout` (search specific field)
//! - Fuzzy: `layot~2` (allows 2 character differences)
//!
//! **Features:**
//! - Pagination support
//! - Result highlighting
//! - Relevance scoring
//!
//! ## Architecture
//!
//! The MCP layer uses direct tokio async/await patterns with three core managers:
//!
//! - **`CrawlSessionManager`**: Tracks active crawls in memory using `tokio::sync::Mutex`.
//!   Sessions have a 30-minute TTL and are automatically cleaned up.
//!
//! - **`SearchEngineCache`**: Caches `SearchEngine` instances per `output_dir` to avoid
//!   expensive re-initialization. Uses double-checked locking for thread-safe caching.
//!
//! - **`ManifestManager`**: Persists crawl metadata to manifest.json files for historical
//!   queries and status tracking after crawl completion.
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use kodegen_tools_citescrape::{
//!     ScrapeUrlTool, FetchTool, WebSearchTool, CrawlRegistry, SearchEngineCache,
//!     BrowserPool, BrowserPoolConfig,
//! };
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create shared managers
//! let engine_cache = Arc::new(SearchEngineCache::new());
//! let browser_pool = BrowserPool::new(BrowserPoolConfig::default());
//! browser_pool.start().await?;
//! let registry = Arc::new(CrawlRegistry::new(engine_cache.clone(), browser_pool.clone()));
//!
//! // Create tools
//! let scrape_tool = ScrapeUrlTool::new(registry.clone());
//! let fetch_tool = FetchTool::new(registry.clone());
//! let browser_manager = Arc::new(kodegen_tools_citescrape::web_search::BrowserManager::new());
//! let search_tool = WebSearchTool::new(browser_manager);
//!
//! // Tools are ready to use
//! # Ok(())
//! # }
//! ```
//!
//! ## Workflow
//!
//! 1. **Crawl Website**: Call `scrape_url` with target URL and options → blocks until complete, returns full results
//! 2. **Search Results**: Call `scrape_search_results` with `crawl_id` or `output_dir` → returns ranked results
//!
//! ## Output Directory Structure
//!
//! Crawled content is organized under `.kodegen/citescrape/` (in git repo) or
//! `~/.local/share/kodegen/citescrape/` (fallback):
//! ```text
//! ${git_root}/.kodegen/citescrape/
//! └── ratatui.rs/
//!     ├── manifest.json          # Crawl metadata
//!     ├── .search_index/         # Tantivy search index
//!     │   ├── meta.json
//!     │   └── ...
//!     ├── index.md               # Crawled markdown files
//!     ├── tutorials/
//!     │   └── hello-world.md
//!     └── ...
//! ```
//!
//! ## Manager Lifecycle
//!
//! Managers are designed to be long-lived and shared:
//! - Create once at application startup
//! - Clone when passing to tool constructors
//! - Arc-based internally for efficient sharing
//!
//! ## Error Handling
//!
//! Tools return `Result<Value, McpError>`:
//! - Invalid URL: "Invalid URL 'xyz': ..."
//! - Missing index: "Search index not found at ..."
//! - Not found: "Crawl not found. Check `crawl_id` or `output_dir`."
//!
//! Handle errors appropriately in your MCP server implementation.

pub mod fetch;
pub mod manager;
pub mod registry;        // NEW
pub mod session;         // NEW
pub mod start_crawl;     // REFACTORED
pub mod types;
pub mod validation;
pub mod web_search;

// Re-export main types for convenience
pub use types::{ActiveCrawlSession, ConfigSummary, CrawlManifest, CrawlStatus};

// Re-export managers and utilities
pub use manager::{CrawlSessionManager, ManifestManager, SearchEngineCache, url_to_output_dir};
pub use registry::CrawlRegistry;   // NEW
pub use session::CrawlSession;     // NEW
pub use validation::ErrorContext;

// Re-export tools
pub use fetch::FetchTool;
pub use start_crawl::ScrapeUrlTool;
pub use web_search::WebSearchTool;
