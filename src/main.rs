// Category HTTP Server: Citescrape Tools
//
// This binary serves web crawling and search tools over HTTP/HTTPS transport.
// Managed by kodegend daemon, typically running on port kodegen_config::PORT_CITESCRAPE (30439).

use anyhow::Result;
use kodegen_config::CATEGORY_CITESCRAPE;
use kodegen_server_http::{ServerBuilder, Managers, RouterSet, ShutdownHook, register_tool, ConnectionCleanupFn};
use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;

// Wrapper to impl ShutdownHook for Arc<BrowserPool>
struct BrowserPoolWrapper(Arc<kodegen_tools_citescrape::BrowserPool>);

impl ShutdownHook for BrowserPoolWrapper {
    fn shutdown(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let pool = self.0.clone();
        Box::pin(async move {
            pool.shutdown().await
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    ServerBuilder::new()
        .category(CATEGORY_CITESCRAPE)
        .register_tools(|| async {
            let mut tool_router = ToolRouter::new();
            let mut prompt_router = PromptRouter::new();
            let managers = Managers::new();

            // Create managers
            let engine_cache = Arc::new(kodegen_tools_citescrape::SearchEngineCache::new());

            // Create browser pool for pre-warmed Chrome instances
            let pool_config = kodegen_tools_citescrape::BrowserPoolConfig::default();
            let browser_pool = kodegen_tools_citescrape::BrowserPool::new(pool_config);
            if let Err(e) = browser_pool.start().await {
                log::error!("Failed to start browser pool: {}", e);
                return Err(anyhow::anyhow!("Failed to start browser pool: {}", e));
            }

            // Create crawl registry (NEW - replaces CrawlSessionManager)
            let crawl_registry = Arc::new(kodegen_tools_citescrape::CrawlRegistry::new(
                engine_cache.clone(),
                browser_pool.clone(),
            ));

            // Register browser pool for graceful shutdown
            managers.register(BrowserPoolWrapper(browser_pool.clone())).await;

            // Register tools
            use kodegen_tools_citescrape::*;

            // Register unified scrape_url tool with registry
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                ScrapeUrlTool::new(crawl_registry.clone()),
            );

            // web_search tool now uses shared browser pool
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                WebSearchTool::new(browser_pool.clone()),
            );

            // Register fetch tool (simplified single-page fetcher)
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                FetchTool::new(crawl_registry.clone()),
            );

            // CRITICAL: Start cleanup tasks after all tools are registered
            engine_cache.start_cleanup_task();

            // Create cleanup callback for connection dropped notification
            let cleanup: ConnectionCleanupFn = Arc::new(move |connection_id: String| {
                let registry = crawl_registry.clone();
                Box::pin(async move {
                    let cleaned = registry.cleanup_connection(&connection_id).await;
                    log::debug!(
                        "Connection {}: cleaned up {} crawl session(s) (output directories preserved)",
                        connection_id,
                        cleaned
                    );
                }) as Pin<Box<dyn Future<Output = ()> + Send + 'static>>
            });

            let mut router_set = RouterSet::new(tool_router, prompt_router, managers);
            router_set.connection_cleanup = Some(cleanup);
            Ok(router_set)
        })
        .run()
        .await
}
