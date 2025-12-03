//! `web_search` MCP tool implementation
//!
//! Performs web searches and returns structured results with titles, URLs, and snippets.

use kodegen_mcp_schema::citescrape::{WebSearchArgs, WebSearchOutput, WebSearchPromptArgs, WebSearchResultItem};
use kodegen_mcp_tool::{Tool, ToolExecutionContext, ToolResponse};
use kodegen_mcp_tool::error::McpError;
use rmcp::model::{PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole};
use std::sync::Arc;

// =============================================================================
// ANSI Color Constants
// =============================================================================

const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_RESET: &str = "\x1b[0m";
const ICON_SEARCH: &str = "󰍉";
const ICON_LIST: &str = "󰮒";

// =============================================================================
// Tool Struct
// =============================================================================

#[derive(Clone)]
pub struct WebSearchTool {
    browser_manager: Arc<crate::web_search::BrowserManager>,
}

impl WebSearchTool {
    #[must_use]
    pub fn new(browser_manager: Arc<crate::web_search::BrowserManager>) -> Self {
        Self { browser_manager }
    }
}

// =============================================================================
// Tool Trait Implementation
// =============================================================================

impl Tool for WebSearchTool {
    type Args = WebSearchArgs;
    type PromptArgs = WebSearchPromptArgs;

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

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<ToolResponse<WebSearchOutput>, McpError> {
        // Validate query is not empty
        if args.query.trim().is_empty() {
            return Err(McpError::invalid_arguments("Search query cannot be empty"));
        }

        // Perform search
        let results = crate::web_search::search_with_manager(&self.browser_manager, args.query)
            .await
            .map_err(McpError::Other)?;

        // Build summary
        let count = results.results.len();
        let first_title = if results.results.is_empty() {
            "No results"
        } else {
            &results.results[0].title
        };

        let line1 = format!("{}{} Web Search: {}{}", ANSI_CYAN, ICON_SEARCH, results.query, ANSI_RESET);
        let line2 = format!("  {} Results: {} · Top: {}", ICON_LIST, count, first_title);
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

    fn prompt_arguments() -> Vec<PromptArgument> {
        vec![]
    }

    async fn prompt(&self, _args: Self::PromptArgs) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::text("How do I search the web?"),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "The web_search tool performs web searches and returns structured results:\\n\\n\
                     **Basic usage:**\\n\
                     ```json\\n\
                     web_search({\\\"query\\\": \\\"rust async programming\\\"})\\n\
                     ```\\n\\n\
                     **Response format:**\\n\
                     ```json\\n\
                     {\\n\
                       \\\"query\\\": \\\"rust async programming\\\",\\n\
                       \\\"result_count\\\": 10,\\n\
                       \\\"results\\\": [\\n\
                         {\\n\
                           \\\"rank\\\": 1,\\n\
                           \\\"title\\\": \\\"Async Programming in Rust\\\",\\n\
                           \\\"url\\\": \\\"https://example.com/rust-async\\\",\\n\
                           \\\"snippet\\\": \\\"Learn about async/await in Rust...\\\"\\n\
                         }\\n\
                       ]\\n\
                     }\\n\
                     ```\\n\\n\
                     **Key features:**\\n\
                     - Returns up to 10 results\\n\
                     - Includes title, URL, and description snippet\\n\
                     - Results ranked by relevance\\n\
                     - Automatic retry with exponential backoff\\n\
                     - Stealth browser configuration to avoid bot detection\\n\\n\
                     **Use cases:**\\n\
                     - Research technical topics\\n\
                     - Find documentation and tutorials\\n\
                     - Gather information for code generation\\n\
                     - Discover relevant libraries and tools",
                ),
            },
        ])
    }
}
