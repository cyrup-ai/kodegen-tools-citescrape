// Category HTTP Server: Citescrape Tools
//
// This binary serves web crawling and search tools over HTTP/HTTPS transport.
// Managed by kodegend daemon, typically running on port 30445.

use anyhow::Result;
use kodegen_server_http::{run_http_server, Managers, RouterSet, ShutdownHook, register_tool};
use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};
use std::sync::Arc;

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
    run_http_server("citescrape", |_config, _tracker| {
        Box::pin(async move {
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

        // CRITICAL: Start cleanup tasks after all tools are registered
        engine_cache.start_cleanup_task();

        Ok(RouterSet::new(tool_router, prompt_router, managers))
        })
    })
    .await
}
