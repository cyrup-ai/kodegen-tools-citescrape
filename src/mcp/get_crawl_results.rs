//! `scrape_check_results` MCP tool implementation
//!
//! Retrieves crawl status and results for active or completed crawls.

use chrono::Utc;
use kodegen_mcp_schema::citescrape::{ScrapeCheckResultsArgs, ScrapeCheckResultsPromptArgs};
use kodegen_mcp_tool::Tool;
use kodegen_mcp_tool::error::McpError;
use rmcp::model::{Content, PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

use crate::mcp::manager::{CrawlSessionManager, ManifestManager};
use crate::mcp::types::{ActiveCrawlSession, CrawlManifest, CrawlStatus};

// =============================================================================
// Tool Struct
// =============================================================================

#[derive(Clone)]
pub struct ScrapeCheckResultsTool {
    session_manager: Arc<CrawlSessionManager>,
}

impl ScrapeCheckResultsTool {
    #[must_use]
    pub fn new(session_manager: Arc<CrawlSessionManager>) -> Self {
        Self { session_manager }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    fn validate_file_types(types: &[String]) -> Result<Vec<String>, McpError> {
        const VALID_TYPES: &[&str] = &["md", "html", "json", "png"];
        let mut normalized = Vec::new();

        for t in types {
            let lower = t.to_lowercase();
            if !VALID_TYPES.contains(&lower.as_str()) {
                return Err(McpError::InvalidArguments(format!(
                    "Invalid file_type '{t}'. Valid: md, html, json, png"
                )));
            }
            normalized.push(lower);
        }

        if normalized.is_empty() {
            return Err(McpError::InvalidArguments(
                "file_types cannot be empty. Valid: md, html, json, png".to_string(),
            ));
        }

        Ok(normalized)
    }

    fn validate_args(args: &ScrapeCheckResultsArgs) -> Result<(), McpError> {
        if args.crawl_id.is_none() && args.output_dir.is_none() {
            return Err(McpError::InvalidArguments(
                "Either crawl_id or output_dir must be provided".to_string(),
            ));
        }

        Ok(())
    }

    async fn list_crawled_files(
        dir: &PathBuf,
        file_types: Option<&[String]>,
    ) -> Result<Vec<String>, McpError> {
        let mut files = Vec::new();

        if !dir.exists() {
            return Ok(files);
        }

        let mut entries = fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_dir() {
                // Skip .search_index directory
                if path.file_name().and_then(|n| n.to_str()) == Some(".search_index") {
                    continue;
                }

                // Recursively list subdirectories - CRITICAL: Use Box::pin
                let subfiles = Box::pin(Self::list_crawled_files(&path, file_types)).await?;
                files.extend(subfiles);
            } else if let Some(ext) = path.extension() {
                // Skip temporary files
                let ext_str = ext.to_string_lossy().to_lowercase();
                if ext_str == "tmp" {
                    continue;
                }

                // Skip manifest.json (crawl metadata, not content)
                if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                    && filename == "manifest.json"
                {
                    continue;
                }

                // Determine if this file type should be included
                let should_include = match file_types {
                    Some(types) if !types.is_empty() => {
                        // User specified filter - check if extension matches
                        types.iter().any(|t| t.to_lowercase() == ext_str)
                    }
                    _ => {
                        // No filter OR empty filter = include all default types
                        ext_str == "md"
                            || ext_str == "html"
                            || ext_str == "json"
                            || ext_str == "png"
                    }
                };

                if should_include {
                    // Return absolute paths for direct file access
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }

        files.sort();
        Ok(files)
    }

    // =========================================================================
    // Response Formatters
    // =========================================================================

    async fn format_active_response(
        session: &ActiveCrawlSession,
        args: &ScrapeCheckResultsArgs,
    ) -> Result<Vec<Content>, McpError> {
        let elapsed_secs = (Utc::now() - session.start_time).num_seconds();
        let status_str = match &session.status {
            CrawlStatus::Running => "running",
            CrawlStatus::Completed => "completed",
            CrawlStatus::Failed { .. } => "failed",
        };
        
        let mut contents = Vec::new();
        
        // Content[0]: Human summary
        let summary = format!(
            "📊 Crawl Status: {}\n\n\
             Crawl ID: {}\n\
             Pages crawled: {}\n\
             Elapsed: {}s\n\
             Output: {}\n\
             Current URL: {}",
            status_str.to_uppercase(),
            session.crawl_id,
            session.total_pages,
            elapsed_secs,
            session.output_dir.display(),
            session.current_url.as_deref().unwrap_or("N/A")
        );
        contents.push(Content::text(summary));
        
        // Content[1]: Full machine-readable data
        let mut response = json!({
            "crawl_id": session.crawl_id,
            "status": status_str,
            "output_dir": session.output_dir.to_string_lossy(),
            "total_pages": session.total_pages,
            "elapsed_seconds": elapsed_secs,
        });
        
        if args.include_progress
            && let Some(obj) = response.as_object_mut()
        {
            obj.insert("progress".to_string(), json!({
                "phase": format!("{:?}", session.progress),
                "current_url": session.current_url,
            }));
        }
        
        let json_str = serde_json::to_string_pretty(&response)
            .unwrap_or_else(|_| "{}".to_string());
        contents.push(Content::text(json_str));
        
        Ok(contents)
    }

    async fn format_manifest_response_with_types(
        manifest: &CrawlManifest,
        args: &ScrapeCheckResultsArgs,
        file_types: Option<Vec<String>>,
    ) -> Result<Vec<Content>, McpError> {
        let status_str = match &manifest.status {
            CrawlStatus::Running => "running",
            CrawlStatus::Completed => "completed",
            CrawlStatus::Failed { .. } => "failed",
        };
        
        let mut contents = Vec::new();
        
        // Calculate duration from timestamps
        let duration_secs = if let Some(end_time) = manifest.end_time {
            (end_time - manifest.start_time).num_seconds()
        } else {
            0
        };
        
        // Get file list if requested
        let files = if args.list_files {
            Self::list_crawled_files(&manifest.output_dir, file_types.as_deref()).await?
        } else {
            Vec::new()
        };
        
        // Content[0]: Human summary
        let file_summary = if args.list_files {
            format!("\n\nFiles generated: {} files", files.len())
        } else {
            String::new()
        };
        
        let summary = format!(
            "📊 Crawl Completed\n\n\
             Status: {}\n\
             Total pages: {}\n\
             Duration: {}s\n\
             Output: {}{}",
            status_str.to_uppercase(),
            manifest.total_pages,
            duration_secs,
            manifest.output_dir.display(),
            file_summary
        );
        contents.push(Content::text(summary));
        
        // Content[1]: Full machine-readable data
        let file_count = files.len();
        let response = json!({
            "status": status_str,
            "output_dir": manifest.output_dir.to_string_lossy(),
            "total_pages": manifest.total_pages,
            "duration_seconds": duration_secs,
            "start_time": manifest.start_time,
            "end_time": manifest.end_time,
            "files": if args.list_files { files } else { Vec::new() },
            "file_count": if args.list_files { file_count } else { 0 }
        });
        
        let json_str = serde_json::to_string_pretty(&response)
            .unwrap_or_else(|_| "{}".to_string());
        contents.push(Content::text(json_str));
        
        Ok(contents)
    }
}

// =============================================================================
// Tool Trait Implementation
// =============================================================================

impl Tool for ScrapeCheckResultsTool {
    type Args = ScrapeCheckResultsArgs;
    type PromptArgs = ScrapeCheckResultsPromptArgs;

    fn name() -> &'static str {
        "scrape_check_results"
    }

    fn description() -> &'static str {
        "Check crawl status and retrieve results for active or completed crawls. \
         Returns progress information for running crawls and summary with file list \
         for completed crawls. Requires either crawl_id (for active) or output_dir \
         (for completed).\n\n\
         Returns:\n\
         - Status: running, completed, or failed\n\
         - Progress: current URL, pages crawled, elapsed time (active only)\n\
         - File list: absolute paths to crawled files (HTML, JSON, markdown, PNG) (completed, optional)\n\
         - Summary: total pages, duration, timestamps (completed only)\n\n\
         File Filtering:\n\
         Use file_types parameter to filter results: [\"md\", \"html\", \"json\", \"png\"]\n\
         Default (no filter): Returns all file types\n\
         File types are case-insensitive (\"md\", \"MD\" both work).\n\
         Empty arrays are rejected with error.\n\n\
         Example: scrape_check_results({\"crawl_id\": \"...\", \"file_types\": [\"md\", \"html\"]})"
    }

    fn read_only() -> bool {
        true
    }

    fn destructive() -> bool {
        false
    }

    fn open_world() -> bool {
        false
    }

    async fn execute(&self, args: Self::Args) -> Result<Vec<Content>, McpError> {
        // 1. Validate arguments
        Self::validate_args(&args)?;

        // 2. Validate and normalize file_types if provided
        let file_types = if let Some(ref types) = args.file_types {
            Some(Self::validate_file_types(types)?)
        } else {
            None
        };

        // 3. Try to find active session first (by crawl_id)
        if let Some(ref crawl_id) = args.crawl_id
            && let Some(session) = self.session_manager.get_session(crawl_id).await
        {
            return Self::format_active_response(&session, &args).await;
        }

        // 4. If not active, try to load from manifest (by output_dir)
        if let Some(ref output_dir_str) = args.output_dir {
            let output_dir = PathBuf::from(output_dir_str);
            if ManifestManager::exists(&output_dir).await {
                let manifest = ManifestManager::load(&output_dir).await?;
                return Self::format_manifest_response_with_types(&manifest, &args, file_types)
                    .await;
            }
        }

        // 5. Crawl not found
        Err(McpError::ResourceNotFound(
            "Crawl not found. Verify crawl_id or output_dir.".to_string(),
        ))
    }

    fn prompt_arguments() -> Vec<PromptArgument> {
        vec![]
    }

    async fn prompt(&self, _args: Self::PromptArgs) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::text("How do I check crawl status?"),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "The get_crawl_results tool checks status and retrieves results:\n\n\
                     **Check active crawl:**\n\
                     ```json\n\
                     get_crawl_results({\"crawl_id\": \"550e8400-e29b-...\"})\n\
                     ```\n\n\
                     **Check completed crawl by directory:**\n\
                     ```json\n\
                     get_crawl_results({\"output_dir\": \"docs/ratatui.rs\"})\n\
                     ```\n\n\
                     **Response for running crawl:**\n\
                     ```json\n\
                     {\n\
                       \"status\": \"running\",\n\
                       \"total_pages\": 15,\n\
                       \"elapsed_seconds\": 45,\n\
                       \"progress\": {\n\
                         \"phase\": \"PageLoaded\",\n\
                         \"current_url\": \"https://ratatui.rs/tutorials/hello-world\"\n\
                       }\n\
                     }\n\
                     ```\n\n\
                     **Response for completed crawl:**\n\
                     ```json\n\
                     {\n\
                       \"status\": \"completed\",\n\
                       \"summary\": {\n\
                         \"total_pages\": 42,\n\
                         \"duration_seconds\": 126\n\
                       },\n\
                       \"files\": [\n\
                         \"/docs/ratatui.rs/index.html\",\n\
                         \"/docs/ratatui.rs/index.json\",\n\
                         \"/docs/ratatui.rs/index.md\",\n\
                         \"/docs/ratatui.rs/tutorials/hello-world.html\",\n\
                         \"/docs/ratatui.rs/tutorials/hello-world.json\",\n\
                         \"/docs/ratatui.rs/tutorials/hello-world.md\"\n\
                       ]\n\
                     }\n\
                     ```\n\n\
                     **Typical workflow:**\n\
                     1. start_crawl returns crawl_id\n\
                     2. Poll with get_crawl_results until status is 'completed'\n\
                     3. Use files list to read specific pages\n\
                     4. Search with search_crawl_results",
                ),
            },
        ])
    }
}
