//! `scrape_url` MCP tool - Elite Terminal Pattern
//!
//! Unified tool with 5 actions: CRAWL, READ, LIST, KILL, SEARCH
//! Pattern based on: packages/kodegen-tools-terminal/src/tool.rs

use kodegen_mcp_schema::citescrape::{ScrapeAction, ScrapeUrlArgs, ScrapeUrlOutput, ScrapeUrlPrompts, SCRAPE_URL};
use kodegen_mcp_schema::{Tool, ToolExecutionContext, ToolResponse};
use kodegen_mcp_schema::McpError;
use std::path::PathBuf;
use std::sync::Arc;

use super::registry::CrawlRegistry;
use super::manager::url_to_output_dir;

/// Unified scrape_url tool with action-based dispatch
#[derive(Clone)]
pub struct ScrapeUrlTool {
    registry: Arc<CrawlRegistry>,
}

impl ScrapeUrlTool {
    pub fn new(registry: Arc<CrawlRegistry>) -> Self {
        Self { registry }
    }

    /// Resolve output directory from args using client PWD
    fn resolve_output_dir(
        args: &ScrapeUrlArgs,
        client_pwd: Option<&std::path::Path>,
    ) -> Result<PathBuf, McpError> {
        if let Some(ref dir) = args.output_dir {
            // User provided explicit output_dir - still needs pwd resolution if relative
            if PathBuf::from(dir).is_absolute() {
                Ok(PathBuf::from(dir))
            } else {
                // Resolve relative path using client PWD
                let base_path = if let Some(pwd) = client_pwd {
                    pwd.to_path_buf()
                } else {
                    std::env::current_dir()
                        .map_err(|e| McpError::InvalidUrl(format!("Failed to get current directory: {e}")))?
                };
                Ok(base_path.join(dir))
            }
        } else if let Some(ref url) = args.url {
            url_to_output_dir(url, None, client_pwd)
        } else {
            // For READ/SEARCH without url, use a default relative path
            let base_path = if let Some(pwd) = client_pwd {
                pwd.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|e| McpError::InvalidUrl(format!("Failed to get current directory: {e}")))?
            };
            Ok(base_path.join("docs"))
        }
    }
}

impl Tool for ScrapeUrlTool {
    type Args = ScrapeUrlArgs;
    type Prompts = ScrapeUrlPrompts;

    fn name() -> &'static str {
        SCRAPE_URL
    }

    fn description() -> &'static str {
        "Web crawling with unified action-based interface (CRAWL/READ/LIST/KILL/SEARCH). \
         Supports connection isolation, instance numbering (crawl_id:0, crawl_id:1...), \
         timeout with background continuation, and intelligent auto-crawl search. \
         \n\n\
         **Actions:**\n\
         - SEARCH: Search with auto-crawl if needed (RECOMMENDED - one-step operation)\n\
         - CRAWL: Explicit crawl with timeout support\n\
         - READ: Check current progress without blocking\n\
         - LIST: Show all active crawls for connection\n\
         - KILL: Cancel crawl and cleanup resources\n\n\
         **One-Step Search (auto-crawls if index missing):**\n\
         scrape_url({action: 'SEARCH', url: 'https://ratatui.rs', crawl_id: 0, query: 'layout'})\n\n\
         **Explicit Crawl:**\n\
         scrape_url({action: 'CRAWL', crawl_id: 0, url: 'https://ratatui.rs'})"
    }

    fn read_only() -> bool {
        false
    }

    fn destructive() -> bool {
        false
    }

    fn open_world() -> bool {
        true
    }

    async fn execute(
        &self,
        args: Self::Args,
        ctx: ToolExecutionContext,
    ) -> Result<ToolResponse<ScrapeUrlOutput>, McpError> {
        let connection_id = ctx.connection_id().unwrap_or("default");

        // Dispatch based on action (pattern from terminal/tool.rs:72-120)
        let result: ScrapeUrlOutput = match args.action {
            ScrapeAction::List => {
                // List all crawls for connection
                self.registry
                    .list_all_crawls(connection_id)
                    .await
                    .map_err(McpError::Other)?
            }

            ScrapeAction::Kill => {
                // Kill specific crawl
                self.registry
                    .kill_crawl(connection_id, args.crawl_id)
                    .await
                    .map_err(McpError::Other)?
            }

            ScrapeAction::Read => {
                // Read current state without executing
                let output_dir = Self::resolve_output_dir(&args, ctx.pwd())?;
                let session = self
                    .registry
                    .find_or_create_crawl(connection_id, args.crawl_id, output_dir)
                    .await
                    .map_err(McpError::Other)?;
                
                session
                    .read_current_state()
                    .await
                    .map_err(McpError::Other)?
            }

            ScrapeAction::Search => {
                // Search indexed content with intelligent auto-crawl
                let query = args.query.as_ref().ok_or_else(|| {
                    McpError::InvalidArguments("query required for SEARCH action".to_string())
                })?;
                
                let output_dir = Self::resolve_output_dir(&args, ctx.pwd())?;
                let session = self
                    .registry
                    .find_or_create_crawl(connection_id, args.crawl_id, output_dir)
                    .await
                    .map_err(McpError::Other)?;
                
                // Pass url for auto-crawl if index doesn't exist
                session
                    .search_indexed_content(
                        args.url.clone(),  // Auto-crawl if needed
                        query.clone(),
                        args.search_limit,
                        args.search_offset,
                        args.search_highlight,
                        args.clone(),  // Pass full args for auto-crawl config
                    )
                    .await
                    .map_err(McpError::Other)?
            }

            ScrapeAction::Crawl => {
                // Execute crawl with timeout
                let url = args.url.as_ref().ok_or_else(|| {
                    McpError::InvalidArguments("url required for CRAWL action".to_string())
                })?;
                
                // Validate URL
                url::Url::parse(url).map_err(|e| {
                    McpError::InvalidUrl(format!("Invalid URL '{}': {}", url, e))
                })?;
                
                let output_dir = Self::resolve_output_dir(&args, ctx.pwd())?;
                let session = self
                    .registry
                    .find_or_create_crawl(connection_id, args.crawl_id, output_dir)
                    .await
                    .map_err(McpError::Other)?;
                
                session
                    .execute_crawl_with_timeout(args.clone(), args.await_completion_ms)
                    .await
                    .map_err(McpError::Other)?
            }
        };

        // Build summary from result
        let summary = format!(
            "Crawl {} - Status: {} · Pages: {}",
            result.crawl_id,
            result.status,
            result.pages_crawled
        );

        Ok(ToolResponse::new(summary, result))
    }
}
