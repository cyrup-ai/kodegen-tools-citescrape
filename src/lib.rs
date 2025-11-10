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
    // Managers
    CrawlSessionManager,
    CrawlStatus,
    GetCrawlResultsTool,
    ManifestManager,
    SearchCrawlResultsTool,
    SearchEngineCache,
    // Tools
    StartCrawlTool,
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
                eprintln!("Chunk error: {:?}", e);
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
                eprintln!("Unknown error occurred");
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

/// Start the citescrape tools HTTP server programmatically.
///
/// This function is designed to be called from kodegend for embedded server mode.
/// It replicates the logic from main.rs but as a library function.
///
/// # Arguments
/// * `addr` - The socket address to bind to
/// * `tls_cert` - Optional path to TLS certificate file
/// * `tls_key` - Optional path to TLS private key file
///
/// # Returns
/// Returns `Ok(())` when the server shuts down gracefully, or an error if startup/shutdown fails.
pub async fn start_server(
    addr: std::net::SocketAddr,
    tls_cert: Option<std::path::PathBuf>,
    tls_key: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    use kodegen_server_http::{Managers, RouterSet, register_tool};
    use kodegen_tools_config::ConfigManager;
    use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};
    use std::sync::Arc;

    let _ = env_logger::try_init();
    
    let config = ConfigManager::new();
    config.init().await?;
    
    let timestamp = chrono::Utc::now();
    let pid = std::process::id();
    let instance_id = format!("{}-{}", timestamp.format("%Y%m%d-%H%M%S-%9f"), pid);
    kodegen_mcp_tool::tool_history::init_global_history(instance_id.clone()).await;
    
    let mut tool_router = ToolRouter::new();
    let mut prompt_router = PromptRouter::new();
    let managers = Managers::new();
    
    // Create three managers (all local instances)
    let session_manager = Arc::new(crate::CrawlSessionManager::new());
    let engine_cache = Arc::new(crate::SearchEngineCache::new());
    let browser_manager = Arc::new(crate::BrowserManager::new());
    
    // Register browser manager for shutdown (closes Chrome)
    managers.register(BrowserManagerWrapper(browser_manager.clone())).await;
    
    // Register all 4 citescrape tools
    (tool_router, prompt_router) = register_tool(
        tool_router,
        prompt_router,
        crate::StartCrawlTool::new(session_manager.clone(), engine_cache.clone()),
    );

    (tool_router, prompt_router) = register_tool(
        tool_router,
        prompt_router,
        crate::GetCrawlResultsTool::new(session_manager.clone()),
    );

    (tool_router, prompt_router) = register_tool(
        tool_router,
        prompt_router,
        crate::SearchCrawlResultsTool::new(session_manager.clone(), engine_cache.clone()),
    );

    (tool_router, prompt_router) = register_tool(
        tool_router,
        prompt_router,
        crate::WebSearchTool::new(browser_manager.clone()),
    );

    // CRITICAL: Start cleanup tasks after all tools are registered
    session_manager.start_cleanup_task();
    engine_cache.start_cleanup_task();
    
    let router_set = RouterSet::new(tool_router, prompt_router, managers);
    
    let session_config = rmcp::transport::streamable_http_server::session::local::SessionConfig {
        channel_capacity: 16,
        keep_alive: Some(std::time::Duration::from_secs(3600)),
    };
    let session_manager_http = Arc::new(
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager {
            sessions: Default::default(),
            session_config,
        }
    );
    
    let usage_tracker = kodegen_utils::usage_tracker::UsageTracker::new(
        format!("citescrape-{}", instance_id)
    );
    
    let server = kodegen_server_http::HttpServer::new(
        router_set.tool_router,
        router_set.prompt_router,
        usage_tracker,
        config,
        router_set.managers,
        session_manager_http,
    );
    
    let shutdown_timeout = std::time::Duration::from_secs(30);
    let tls_config = tls_cert.zip(tls_key);
    let handle = server.serve_with_tls(addr, tls_config, shutdown_timeout).await?;
    
    handle.wait_for_completion(shutdown_timeout).await
        .map_err(|e| anyhow::anyhow!("Server shutdown error: {}", e))?;
    
    Ok(())
}
