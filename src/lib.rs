#![feature(integer_atomics)]

pub mod browser_setup;
pub mod config;
pub mod content_saver;
pub mod crawl_engine;
pub mod crawl_events;
pub mod inline_css;
pub mod kromekover;
pub mod link_index;
pub mod link_rewriter;
pub mod mcp;
pub mod page_extractor;
pub mod runtime;
pub mod search;
pub mod utils;
pub mod web_search;
pub mod imurl;
pub mod browser_pool;
pub mod browser_profile;

pub use browser_setup::{
    apply_stealth_measures, download_managed_browser, find_browser_executable, launch_browser,
};
pub use config::CrawlConfig;
pub use content_saver::{CacheMetadata, save_json_data};
pub use crawl_engine::{
    ChromiumoxideCrawler, CrawlError, CrawlProgress, CrawlQueue, CrawlResult, Crawler,
};
pub use page_extractor::schema::*;
pub use runtime::{AsyncJsonSave, AsyncStream, BrowserAction, CrawlRequest};
pub use utils::{get_mirror_path, get_uri_from_path};
pub use web_search::BrowserManager;
pub use imurl::ImUrl;
pub use browser_pool::{BrowserPool, BrowserPoolConfig, PooledBrowserGuard};
pub use browser_profile::{
    BrowserProfile,
    create_unique_profile,
    create_unique_profile_with_prefix,
    is_singleton_lock_stale,
    cleanup_stale_lock,
    cleanup_stale_profiles,
};

// Test-accessible modules
pub use crawl_engine::rate_limiter as crawl_rate_limiter;

// New event-driven link rewriting (SQLite-backed)
pub use link_index::LinkIndex;
pub use link_rewriter::LinkRewriter;

// MCP Tools and Managers
pub use mcp::{
    // Types
    ActiveCrawlSession,
    ConfigSummary,
    CrawlManifest,
    CrawlStatus,
    // Managers
    CrawlSessionManager,
    ManifestManager,
    SearchEngineCache,
    // Registry (NEW)
    CrawlRegistry,
    CrawlSession,
    // Tools
    FetchTool,
    ScrapeUrlTool,
    WebSearchTool,
    // Utilities
    url_to_output_dir,
};

/// Macro for handling streaming data chunks with safe unwrapping
#[macro_export]
macro_rules! on_chunk {
    ($closure:expr) => {
        move |chunk| match chunk {
            Ok(data) => $closure(data),
            Err(e) => {
                tracing::warn!(error = ?e, "Chunk processing error");
            }
        }
    };
}

/// Macro for handling errors with safe unwrapping
#[macro_export]
macro_rules! on_error {
    ($closure:expr) => {
        move |error| match error {
            Some(e) => $closure(e),
            None => {
                tracing::error!("Unknown error occurred in event handler");
            }
        }
    };
}

pub async fn crawl(config: CrawlConfig) -> Result<(), CrawlError> {
    let crawler = ChromiumoxideCrawler::new(config);
    crawler.crawl().await
}

// Shutdown hook wrapper for BrowserManager
struct BrowserManagerWrapper(std::sync::Arc<crate::BrowserManager>);

impl kodegen_server_http::ShutdownHook for BrowserManagerWrapper {
    fn shutdown(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        let manager = self.0.clone();
        Box::pin(async move {
            manager.shutdown().await
        })
    }
}

// Shutdown hook wrapper for BrowserPool
struct BrowserPoolWrapper(std::sync::Arc<crate::BrowserPool>);

impl kodegen_server_http::ShutdownHook for BrowserPoolWrapper {
    fn shutdown(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        let pool = self.0.clone();
        Box::pin(async move {
            pool.shutdown().await
        })
    }
}

/// Start the citescrape tools HTTP server programmatically
///
/// Returns a ServerHandle for graceful shutdown control.
/// This function is non-blocking - the server runs in background tasks.
///
/// # Arguments
/// * `addr` - Socket address to bind to (e.g., "127.0.0.1:30439")
/// * `tls_cert` - Optional path to TLS certificate file
/// * `tls_key` - Optional path to TLS private key file
///
/// # Returns
/// ServerHandle for graceful shutdown, or error if startup fails
pub async fn start_server(
    addr: std::net::SocketAddr,
    tls_cert: Option<std::path::PathBuf>,
    tls_key: Option<std::path::PathBuf>,
) -> anyhow::Result<kodegen_server_http::ServerHandle> {
    // Bind to the address first
    let listener = tokio::net::TcpListener::bind(addr).await
        .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

    // Convert separate cert/key into Option<(cert, key)> tuple
    let tls_config = match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => Some((cert, key)),
        _ => None,
    };

    // Delegate to start_server_with_listener
    start_server_with_listener(listener, tls_config).await
}

/// Start citescrape tools HTTP server using pre-bound listener (TOCTOU-safe)
///
/// This variant is used by kodegend to eliminate TOCTOU race conditions
/// during port cleanup. The listener is already bound to a port.
///
/// # Arguments
/// * `listener` - Pre-bound TcpListener (port already reserved)
/// * `tls_config` - Optional (cert_path, key_path) for HTTPS
///
/// # Returns
/// ServerHandle for graceful shutdown, or error if startup fails
pub async fn start_server_with_listener(
    listener: tokio::net::TcpListener,
    tls_config: Option<(std::path::PathBuf, std::path::PathBuf)>,
) -> anyhow::Result<kodegen_server_http::ServerHandle> {
    use kodegen_server_http::{ServerBuilder, Managers, RouterSet, register_tool};
    use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};
    use std::sync::Arc;

    let mut builder = ServerBuilder::new()
        .category(kodegen_config::CATEGORY_CITESCRAPE)
        .register_tools(|| async {
            let mut tool_router = ToolRouter::new();
            let mut prompt_router = PromptRouter::new();
            let managers = Managers::new();

            // Create browser pool
            let pool_config = crate::BrowserPoolConfig::default();
            let browser_pool = crate::BrowserPool::new(pool_config);
            if let Err(e) = browser_pool.start().await {
                return Err(anyhow::anyhow!("Failed to start browser pool: {}", e));
            }

            // Create managers
            let engine_cache = Arc::new(crate::SearchEngineCache::new());
            let browser_manager = Arc::new(crate::BrowserManager::new());

            // Create crawl registry with browser pool
            let crawl_registry = Arc::new(crate::CrawlRegistry::new(
                engine_cache.clone(),
                browser_pool.clone(),
            ));

            // Register browser pool for shutdown
            managers.register(BrowserPoolWrapper(browser_pool.clone())).await;

            // Register browser manager for shutdown (still used by WebSearchTool)
            managers.register(BrowserManagerWrapper(browser_manager.clone())).await;

            // Register tools
            // Register unified scrape_url tool with registry
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                crate::ScrapeUrlTool::new(crawl_registry.clone()),
            );

            // Keep web_search tool (unchanged)
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                crate::WebSearchTool::new(browser_manager.clone()),
            );

            // Register fetch tool (simplified single-page fetcher)
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                crate::FetchTool::new(crawl_registry.clone()),
            );

            // CRITICAL: Start cleanup tasks after all tools are registered
            engine_cache.start_cleanup_task();

            Ok(RouterSet::new(tool_router, prompt_router, managers))
        })
        .with_listener(listener);

    if let Some((cert, key)) = tls_config {
        builder = builder.with_tls_config(cert, key);
    }

    builder.serve().await
}
