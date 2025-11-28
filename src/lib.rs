pub mod browser_setup;
pub mod config;
pub mod content_saver;
pub mod crawl_engine;
pub mod crawl_events;
pub mod inline_css;
pub mod kromekover;
pub mod mcp;
pub mod page_extractor;
pub mod runtime;
pub mod search;
pub mod utils;
pub mod web_search;
pub mod imurl;

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

// Test-accessible modules
pub use crawl_engine::rate_limiter as crawl_rate_limiter;
pub use page_extractor::link_rewriter;

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

/// Start the citescrape tools HTTP server programmatically
///
/// Returns a ServerHandle for graceful shutdown control.
/// This function is non-blocking - the server runs in background tasks.
///
/// # Arguments
/// * `addr` - Socket address to bind to (e.g., "127.0.0.1:30445")
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
    use kodegen_server_http::{create_http_server, Managers, RouterSet, register_tool};
    use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};
    use std::sync::Arc;
    use std::time::Duration;

    let tls_config = match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => Some((cert, key)),
        _ => None,
    };

    let shutdown_timeout = Duration::from_secs(30);
    let session_keep_alive = Duration::ZERO;  // Infinite keep-alive for MCP sessions

    create_http_server(
        "citescrape",
        addr,
        tls_config,
        shutdown_timeout,
        session_keep_alive,
        |_config, _tracker| {
        Box::pin(async move {
            let mut tool_router = ToolRouter::new();
            let mut prompt_router = PromptRouter::new();
            let managers = Managers::new();

            // Create managers
            let engine_cache = Arc::new(crate::SearchEngineCache::new());
            let browser_manager = Arc::new(crate::BrowserManager::new());

            // Create crawl registry (NEW - replaces CrawlSessionManager)
            let crawl_registry = Arc::new(crate::CrawlRegistry::new(engine_cache.clone()));

            // Register browser manager for shutdown (closes Chrome)
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

            // CRITICAL: Start cleanup tasks after all tools are registered
            engine_cache.start_cleanup_task();

            Ok(RouterSet::new(tool_router, prompt_router, managers))
        })
    }).await
}
