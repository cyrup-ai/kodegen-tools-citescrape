//! `web_search` MCP tool implementation
//!
//! Performs web searches and returns structured results with titles, URLs, and snippets.

use kodegen_mcp_schema::citescrape::{WebSearchArgs, WebSearchPromptArgs};
use kodegen_mcp_tool::Tool;
use kodegen_mcp_tool::error::McpError;
use rmcp::model::{Content, PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole};
use serde_json::json;
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

    async fn execute(&self, args: Self::Args) -> Result<Vec<Content>, McpError> {
        // Validate query is not empty
        if args.query.trim().is_empty() {
            return Err(McpError::invalid_arguments("Search query cannot be empty"));
        }

        // Perform search
        let results = crate::web_search::search_with_manager(&self.browser_manager, args.query)
            .await
            .map_err(McpError::Other)?;

        // Build dual-content response
        let mut contents = Vec::new();

        // Content[0]: Human summary (two-line format with ANSI colors and Nerd Font icons)
        let count = results.results.len();
        let first_title = if results.results.is_empty() {
            "No results"
        } else {
            &results.results[0].title
        };

        let line1 = format!("{}{} Web Search: {}{}", ANSI_CYAN, ICON_SEARCH, results.query, ANSI_RESET);
        let line2 = format!("  {} Results: {} · Top: {}", ICON_LIST, count, first_title);
        let summary = format!("{}\n{}", line1, line2);

        contents.push(Content::text(summary));
        
        // Content[1]: Full machine-readable data
        let metadata = json!({
            "query": results.query,
            "result_count": results.results.len(),
            "results": results.results.iter().map(|r| json!({
                "rank": r.rank,
                "title": r.title,
                "url": r.url,
                "snippet": r.snippet,
            })).collect::<Vec<_>>(),
        });
        let json_str = serde_json::to_string_pretty(&metadata)
            .unwrap_or_else(|_| "{}".to_string());
        contents.push(Content::text(json_str));
        
        Ok(contents)
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
