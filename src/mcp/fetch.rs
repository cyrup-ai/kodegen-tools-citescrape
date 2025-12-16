//! `fetch` MCP tool - Simplified single-page fetcher with ANSI highlighting
//!
//! Wraps `scrape_url` with `max_depth: 0`, `limit: 1` for single-page retrieval.
//! Returns ANSI syntax-highlighted markdown for beautiful terminal display.

use kodegen_mcp_schema::citescrape::{
    FetchArgs, FetchOutput, FetchPrompts, FETCH,
    ScrapeAction, ScrapeUrlArgs,
};
use kodegen_mcp_schema::{McpError, Tool, ToolExecutionContext, ToolResponse};
use std::sync::{Arc, LazyLock};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

use super::registry::CrawlRegistry;
use super::start_crawl::ScrapeUrlTool;
use crate::utils::get_mirror_path;

/// Global syntax set for markdown highlighting (loaded once)
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Global theme set for highlighting (loaded once)
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Simplified fetch tool for single-page retrieval
#[derive(Clone)]
pub struct FetchTool {
    scrape_tool: ScrapeUrlTool,
}

impl FetchTool {
    pub fn new(registry: Arc<CrawlRegistry>) -> Self {
        Self {
            scrape_tool: ScrapeUrlTool::new(registry),
        }
    }

    /// Convert markdown to ANSI-highlighted string for terminal display
    fn highlight_markdown_to_ansi(markdown: &str) -> String {
        let syntax = SYNTAX_SET
            .find_syntax_by_extension("md")
            .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

        // Default themes always include "base16-eighties.dark" and others
        // Using or_else with values().next() ensures we always have a valid theme
        let theme = THEME_SET
            .themes
            .get("base16-eighties.dark")
            .or_else(|| THEME_SET.themes.values().next())
            .expect("syntect default themes must have at least one theme");

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut output = String::with_capacity(markdown.len() * 2);

        for line in markdown.lines() {
            match highlighter.highlight_line(line, &SYNTAX_SET) {
                Ok(ranges) => {
                    let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
                    output.push_str(&escaped);
                }
                Err(_) => {
                    // Fallback: plain text if highlighting fails
                    output.push_str(line);
                }
            }
            output.push('\n');
        }

        // Reset ANSI at end
        output.push_str("\x1b[0m");
        output
    }

    /// Generate TypeScript search helper snippet
    fn generate_search_helper(crawl_id: u32) -> String {
        format!(
            "scrape_url({{ action: 'SEARCH', crawl_id: {}, query: '<your query>' }})",
            crawl_id
        )
    }
}

impl Tool for FetchTool {
    type Args = FetchArgs;
    type Prompts = FetchPrompts;

    fn name() -> &'static str {
        FETCH
    }

    fn description() -> &'static str {
        "Fetch a single web page and display as ANSI-highlighted markdown. \
         Returns syntax-colored content for terminal display plus metadata \
         including file path and search helper for follow-up queries."
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
    ) -> Result<ToolResponse<FetchOutput>, McpError> {
        // Build scrape_url args for single-page fetch
        let scrape_args = ScrapeUrlArgs {
            action: ScrapeAction::Crawl,
            crawl_id: 0,
            await_completion_ms: 60_000, // 1 minute timeout for single page
            url: Some(args.url.clone()),
            output_dir: None,
            max_depth: 0,           // Single page only
            limit: Some(1),         // Maximum 1 page
            save_markdown: true,
            save_screenshots: false,
            enable_search: true,    // Enable for search_helper
            crawl_rate_rps: 2.0,
            allow_subdomains: false,
            content_types: None,
            query: None,
            search_limit: 10,
            search_offset: 0,
            search_highlight: true,
        };

        // Execute scrape_url
        let scrape_result = self.scrape_tool.execute(scrape_args, ctx).await?;
        let scrape_output = scrape_result.metadata;

        // Get the output directory
        let output_dir = scrape_output.output_dir.ok_or_else(|| {
            McpError::Other(anyhow::anyhow!("No output directory returned from scrape"))
        })?;

        // Use deterministic path calculation (same as crawler uses to save)
        let md_path = get_mirror_path(&args.url, std::path::Path::new(&output_dir), "index.md")
            .await
            .map_err(|e| McpError::Other(anyhow::anyhow!("Failed to calculate markdown path: {}", e)))?;

        let md_path_str = md_path.to_string_lossy().to_string();

        // Read the markdown content
        let markdown_content = match tokio::fs::read_to_string(&md_path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(McpError::Other(anyhow::anyhow!(
                    "Markdown file not found at expected path: {}",
                    md_path.display()
                )));
            }
            Err(e) => {
                return Err(McpError::Other(anyhow::anyhow!(
                    "Failed to read markdown from {}: {}",
                    md_path.display(),
                    e
                )));
            }
        };

        // Extract title from first # heading
        let title = markdown_content
            .lines()
            .find(|line| line.starts_with("# "))
            .map(|line| line.trim_start_matches("# ").to_string());

        // Generate ANSI-highlighted display
        let display = Self::highlight_markdown_to_ansi(&markdown_content);

        // Generate search helper
        let search_helper = Self::generate_search_helper(scrape_output.crawl_id);

        let output = FetchOutput {
            path: md_path_str,
            search_helper,
            url: args.url,
            title: title.clone(),
            content_length: markdown_content.len(),
        };

        Ok(ToolResponse::new(display, output))
    }
}

