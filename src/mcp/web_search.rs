//! `web_search` MCP tool implementation
//!
//! Performs web searches and returns structured results with titles, URLs, and snippets.

use kodegen_mcp_schema::citescrape::{WebSearchArgs, WebSearchOutput, WebSearchPrompts, WebSearchResultItem};
use kodegen_mcp_schema::{Tool, ToolExecutionContext, ToolResponse};
use kodegen_mcp_schema::McpError;
use std::sync::Arc;

// =============================================================================
// ANSI Color Constants
// =============================================================================

const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_RESET: &str = "\x1b[0m";

// =============================================================================
// Tool Struct
// =============================================================================

#[derive(Clone)]
pub struct WebSearchTool {
    browser_pool: Arc<crate::browser_pool::BrowserPool>,
}

impl WebSearchTool {
    #[must_use]
    pub fn new(browser_pool: Arc<crate::browser_pool::BrowserPool>) -> Self {
        Self { browser_pool }
    }
}

// =============================================================================
// Tool Trait Implementation
// =============================================================================

impl Tool for WebSearchTool {
    type Args = WebSearchArgs;
    type Prompts = WebSearchPrompts;

    fn name() -> &'static str {
        kodegen_mcp_schema::citescrape::WEB_SEARCH
    }

    fn description() -> &'static str {
        "Perform a web search using DuckDuckGo and return structured results with titles, URLs, and snippets.\\n\\n\
         Returns up to 10 search results with:\\n\
         - rank: Result position (1-10)\\n\
         - title: Page title\\n\
         - url: Page URL\\n\
         - snippet: Description excerpt\\n\\n\
         Uses DuckDuckGo to avoid CAPTCHA issues. First search takes ~5-6s (browser launch), \
         subsequent searches take ~3-4s.\\n\\n\
         Example: web_search({\\\"query\\\": \\\"rust async programming\\\"})"
    }

    fn read_only() -> bool {
        true
    }

    fn destructive() -> bool {
        false
    }

    fn open_world() -> bool {
        true
    }

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<ToolResponse<<Self::Args as kodegen_mcp_schema::ToolArgs>::Output>, McpError> {
        // Validate query is not empty
        if args.query.trim().is_empty() {
            return Err(McpError::invalid_arguments("Search query cannot be empty"));
        }

        // Perform search using browser pool
        let results = crate::web_search::search_with_pool(&self.browser_pool, args.query)
            .await
            .map_err(McpError::Other)?;

        // Build summary
        let count = results.results.len();
        let first_title = if results.results.is_empty() {
            "No results"
        } else {
            &results.results[0].title
        };

        let line1 = format!("{}Web Search: {}{}", ANSI_CYAN, results.query, ANSI_RESET);
        let line2 = format!("  Results: {} · Top: {}", count, first_title);
        let summary = format!("{}\n{}", line1, line2);

        // Build typed output
        let output = WebSearchOutput {
            success: true,
            query: results.query,
            results_count: results.results.len(),
            results: results.results.into_iter().map(|r| WebSearchResultItem {
                rank: r.rank as u32,
                title: r.title,
                url: r.url,
                snippet: Some(r.snippet),
            }).collect(),
        };

        Ok(ToolResponse::new(summary, output))
    }
}
