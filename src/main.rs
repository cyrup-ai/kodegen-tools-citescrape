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

// Wrapper to impl ShutdownHook for Arc<BrowserManager>
struct BrowserManagerWrapper(Arc<kodegen_tools_citescrape::BrowserManager>);

impl ShutdownHook for BrowserManagerWrapper {
    fn shutdown(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let manager = self.0.clone();
        Box::pin(async move {
            manager.shutdown().await
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
            let browser_manager = Arc::new(kodegen_tools_citescrape::BrowserManager::new());

            // Create crawl registry (NEW - replaces CrawlSessionManager)
            let crawl_registry = Arc::new(kodegen_tools_citescrape::CrawlRegistry::new(engine_cache.clone()));

            // Register browser manager for shutdown (closes Chrome)
            managers.register(BrowserManagerWrapper(browser_manager.clone())).await;

            // Register tools
            use kodegen_tools_citescrape::*;

            // Register unified scrape_url tool with registry
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                ScrapeUrlTool::new(crawl_registry.clone()),
            );

            // Keep web_search tool (unchanged)
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                WebSearchTool::new(browser_manager.clone()),
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
