//! `scrape_url` MCP tool implementation
//!
//! Initiates background web crawls with automatic search indexing.

use chrono::Utc;
use kodegen_mcp_schema::citescrape::{ScrapeUrlArgs, ScrapeUrlPromptArgs};
use kodegen_mcp_tool::{Tool, ToolExecutionContext};
use kodegen_mcp_tool::error::McpError;
use rmcp::model::{Content, PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;
use uuid::Uuid;

use crate::ChromiumoxideCrawler;
use crate::CrawlRequest;
use crate::Crawler;
use crate::config::CrawlConfig;
use crate::mcp::manager::{
    CrawlSessionManager, ManifestManager, SearchEngineCache, url_to_output_dir,
};
use crate::mcp::types::{ActiveCrawlSession, CrawlManifest, CrawlStatus};

// =============================================================================
// Tool Struct
// =============================================================================

#[derive(Clone)]
pub struct ScrapeUrlTool {
    session_manager: Arc<CrawlSessionManager>,
    engine_cache: Arc<SearchEngineCache>,
}

impl ScrapeUrlTool {
    #[must_use]
    pub fn new(
        session_manager: Arc<CrawlSessionManager>,
        engine_cache: Arc<SearchEngineCache>,
    ) -> Self {
        Self {
            session_manager,
            engine_cache,
        }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    fn validate_content_types(types: &[String]) -> Result<Vec<String>, McpError> {
        const VALID_TYPES: &[&str] = &["markdown", "html", "json", "png"];
        let mut normalized = Vec::new();

        for t in types {
            let lower = t.to_lowercase();
            if !VALID_TYPES.contains(&lower.as_str()) {
                return Err(McpError::InvalidArguments(format!(
                    "Invalid content_type '{t}'. Valid: markdown, html, json, png"
                )));
            }
            normalized.push(lower);
        }

        if normalized.is_empty() {
            return Err(McpError::InvalidArguments(
                "content_types cannot be empty. Valid: markdown, html, json, png".to_string(),
            ));
        }

        Ok(normalized)
    }

    fn validate_url(url: &str) -> Result<(), McpError> {
        Url::parse(url).map_err(|e| McpError::InvalidUrl(format!("Invalid URL '{url}': {e}")))?;
        Ok(())
    }

    fn resolve_output_dir(args: &ScrapeUrlArgs) -> Result<PathBuf, McpError> {
        if let Some(ref dir) = args.output_dir {
            Ok(PathBuf::from(dir))
        } else {
            url_to_output_dir(&args.url, None)
        }
    }

    fn build_config(
        args: &ScrapeUrlArgs,
        output_dir: &Path,
        content_types: Option<Vec<String>>,
    ) -> Result<CrawlConfig, McpError> {
        let search_index_dir = if args.enable_search {
            Some(output_dir.join(".search_index"))
        } else {
            None
        };

        // Determine save flags based on content_types if provided (now normalized)
        let save_markdown = if let Some(ref types) = content_types {
            types.iter().any(|t| t == "markdown")
        } else {
            args.save_markdown
        };

        let save_screenshots = if let Some(ref types) = content_types {
            types.iter().any(|t| t == "png")
        } else {
            args.save_screenshots
        };

        let config = CrawlConfig {
            storage_dir: output_dir.to_path_buf(),
            start_url: args.url.clone(),
            target_url: args.url.clone(),
            limit: args.limit,
            allow_subdomains: args.allow_subdomains,
            save_screenshots,
            save_markdown,
            max_depth: args.max_depth,
            search_index_dir,
            crawl_rate_rps: Some(args.crawl_rate_rps),
            ..Default::default()
        };

        Ok(config)
    }

    /// List all crawled files recursively
    /// 
    /// Extracted from get_crawl_results.rs for file listing functionality
    async fn list_crawled_files(
        dir: &PathBuf,
        file_types: Option<&[String]>,
    ) -> Result<Vec<String>, McpError> {
        use tokio::fs;
        
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
                let ext_str = ext.to_string_lossy().to_lowercase();
                
                // Skip temporary files and manifest
                if ext_str == "tmp" {
                    continue;
                }
                if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                    && filename == "manifest.json"
                {
                    continue;
                }
                
                // Filter by file type if specified
                let should_include = match file_types {
                    Some(types) if !types.is_empty() => {
                        types.iter().any(|t| t.to_lowercase() == ext_str)
                    }
                    _ => {
                        // Include markdown, html, json, png
                        ext_str == "md" || ext_str == "html" 
                            || ext_str == "json" || ext_str == "png"
                    }
                };
                
                if should_include {
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }
        
        files.sort();
        Ok(files)
    }
    
    /// Extract unique file types from file list
    fn get_file_types(files: &[String]) -> Vec<String> {
        use std::collections::HashSet;
        
        let mut types = HashSet::new();
        for file in files {
            if let Some(ext) = std::path::Path::new(file).extension() {
                types.insert(ext.to_string_lossy().to_lowercase());
            }
        }
        
        let mut result: Vec<_> = types.into_iter().collect();
        result.sort();
        result
    }
}

// =============================================================================
// Tool Trait Implementation
// =============================================================================

impl Tool for ScrapeUrlTool {
    type Args = ScrapeUrlArgs;
    type PromptArgs = ScrapeUrlPromptArgs;

    fn name() -> &'static str {
        kodegen_mcp_schema::citescrape::SCRAPE_URL
    }

    fn description() -> &'static str {
        "Execute a complete web crawl that blocks until finished, returning comprehensive \
         results with all crawled files and metadata. This is a long-running operation \
         that may take 30 seconds to 10 minutes depending on site size and depth.\n\n\
         **Execution Model:**\n\
         - Blocks until crawl completes or timeout reached\n\
         - Returns full results immediately (no polling needed)\n\
         - Default timeout: 600 seconds (10 minutes)\n\
         - Partial results returned if timeout reached\n\n\
         **Features:**\n\
         - Automatic Tantivy search indexing\n\
         - Rate limiting (default 2 req/sec)\n\
         - Multiple output formats: Markdown, HTML, JSON\n\
         - Screenshot capture support\n\
         - Configurable timeout with partial result handling\n\n\
         **Returns:**\n\
         - crawl_id: For use with scrape_search_results\n\
         - pages_crawled: Total pages successfully crawled\n\
         - output_dir: Directory containing all crawled files\n\
         - files: Absolute paths to all generated files\n\
         - file_types: Types of files generated (md, html, json, png)\n\
         - elapsed_seconds: Total crawl duration\n\
         - timeout_reached: Whether timeout interrupted crawl\n\n\
         **Example:**\n\
         scrape_url({\"url\": \"https://ratatui.rs\", \"max_depth\": 2, \"timeout_seconds\": 300})"
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

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<Vec<Content>, McpError> {
        use tokio::time::{timeout, Duration};
        use std::time::Instant;
        
        // 1. Validate input
        Self::validate_url(&args.url)?;

        // 2. Validate and normalize content_types if provided
        let content_types = if let Some(ref types) = args.content_types {
            Some(Self::validate_content_types(types)?)
        } else {
            None
        };

        // 3. Resolve output directory
        let output_dir = Self::resolve_output_dir(&args)?;

        // 4. Build crawl config
        let mut config = Self::build_config(&args, &output_dir, content_types)?;

        // 5. Get or initialize search engine and indexing sender if search is enabled
        if args.enable_search {
            let entry = self
                .engine_cache
                .get_or_init(output_dir.clone(), &config)
                .await?;
            if let Some(indexing_sender) = entry.indexing_sender {
                config = config.with_indexing_sender(indexing_sender);
            }
        }

        // 6. Generate unique crawl ID
        let crawl_id = Uuid::new_v4().to_string();

        // 7. Create unique Chrome user data directory for this crawl session
        let chrome_data_dir = std::env::temp_dir().join(format!("enigo_chrome_{crawl_id}"));
        tracing::debug!(
            chrome_data_dir = %chrome_data_dir.display(),
            crawl_id = %crawl_id,
            "Created isolated Chrome user data directory for crawl session"
        );

        // 8. Create event bus for this crawl
        let event_bus = std::sync::Arc::new(crate::crawl_events::CrawlEventBus::new(1000));

        // 9. Attach event bus and chrome data dir to config
        let config = config
            .with_event_bus(event_bus.clone())
            .with_chrome_data_dir(chrome_data_dir.clone());

        tracing::debug!(
            chrome_data_dir = ?config.chrome_data_dir,
            "Chrome data directory configured in crawl config"
        );

        // 10. Subscribe to events for real-time progress tracking
        let mut event_receiver = event_bus.subscribe();
        let progress_session_manager = self.session_manager.clone();
        let progress_crawl_id = crawl_id.clone();

        tokio::spawn(async move {
            let mut page_count = 0;
            while let Ok(event) = event_receiver.recv().await {
                match event {
                    crate::crawl_events::CrawlEvent::PageCrawled { url, .. } => {
                        page_count += 1;
                        progress_session_manager
                            .update_progress(&progress_crawl_id, page_count, Some(url))
                            .await;
                    }
                    crate::crawl_events::CrawlEvent::Shutdown { .. } => {
                        break;
                    }
                    _ => {}
                }
            }
        });

        // 11. Create crawler and get request
        let crawler = ChromiumoxideCrawler::new(config.clone());
        let request: CrawlRequest = crawler.crawl();

        // 12. Register session WITHOUT task_handle
        let session = ActiveCrawlSession {
            crawl_id: crawl_id.clone(),
            config: config.clone(),
            start_time: Utc::now(),
            output_dir: output_dir.clone(),
            status: CrawlStatus::Running,
            progress: None,
            total_pages: 0,
            current_url: Some(args.url.clone()),
        };

        self.session_manager
            .register(crawl_id.clone(), session)
            .await;

        // 13. Await crawl with timeout
        let start_time = Instant::now();
        let timeout_duration = Duration::from_secs(args.timeout_seconds);

        let crawl_result = match timeout(timeout_duration, request).await {
            // Timeout reached - return partial results
            Err(_elapsed) => {
                let partial_pages = if let Some(sess) = self.session_manager.get_session(&crawl_id).await {
                    sess.total_pages
                } else {
                    0
                };

                // Update session status
                self.session_manager
                    .update_status(&crawl_id, CrawlStatus::Completed)
                    .await;

                // Save manifest with partial results
                if let Some(final_session) = self.session_manager.get_session(&crawl_id).await {
                    let mut manifest = CrawlManifest::from_session(&final_session);
                    manifest.complete(partial_pages);
                    ManifestManager::save(&manifest).await?;
                }

                (partial_pages, true) // (pages_crawled, timeout_reached)
            }

            // Crawl completed within timeout
            Ok(Ok(())) => {
                let total_pages = if let Some(sess) = self.session_manager.get_session(&crawl_id).await {
                    sess.total_pages
                } else {
                    0
                };

                // Validate pages crawled (fail if zero)
                if total_pages == 0 {
                    let error_msg = "No pages could be crawled. This may indicate an invalid URL, network issue, or inaccessible domain.";
                    self.session_manager
                        .update_status(&crawl_id, CrawlStatus::Failed {
                            error: error_msg.to_string(),
                        })
                        .await;

                    return Err(McpError::Other(anyhow::anyhow!("{}", error_msg)));
                }

                // Success
                self.session_manager
                    .update_status(&crawl_id, CrawlStatus::Completed)
                    .await;

                // Save manifest
                if let Some(final_session) = self.session_manager.get_session(&crawl_id).await {
                    let mut manifest = CrawlManifest::from_session(&final_session);
                    manifest.complete(total_pages);
                    ManifestManager::save(&manifest).await?;
                }

                (total_pages, false) // (pages_crawled, timeout_reached)
            }

            // Crawl failed with error
            Ok(Err(e)) => {
                let error_msg = format!("Crawl failed: {e}");
                self.session_manager
                    .update_status(&crawl_id, CrawlStatus::Failed {
                        error: error_msg.clone(),
                    })
                    .await;

                return Err(McpError::Other(anyhow::anyhow!("{}", error_msg)));
            }
        };

        let (pages_crawled, timeout_reached) = crawl_result;
        let elapsed_seconds = start_time.elapsed().as_secs_f64();

        // 14. Build comprehensive response with file listing
        let files = Self::list_crawled_files(&output_dir, None).await?;
        let file_types = Self::get_file_types(&files);

        let mut contents = Vec::new();

        // Content[0]: Human summary (ANSI formatted)
        let timeout_marker = if timeout_reached { " (⏱️  TIMEOUT)" } else { "" };
        let summary = format!(
            "\x1b[32m✅ Crawl Completed{}\x1b[0m\n\n\
             URL: {}\n\
             Pages: {} · Duration: {:.1}s\n\
             Output: {}\n\
             Files: {} ({:?})",
            timeout_marker,
            args.url,
            pages_crawled,
            elapsed_seconds,
            output_dir.display(),
            files.len(),
            file_types
        );
        contents.push(Content::text(summary));

        // Content[1]: Machine-readable JSON
        let metadata = json!({
            "crawl_id": crawl_id,
            "summary": format!("Crawled {} pages from {}", pages_crawled, args.url),
            "pages_crawled": pages_crawled,
            "output_dir": output_dir.to_string_lossy(),
            "indexed": args.enable_search,
            "elapsed_seconds": elapsed_seconds,
            "timeout_reached": timeout_reached,
            "file_types": file_types,
            "files": files,
            "config": {
                "url": args.url,
                "max_depth": args.max_depth,
                "max_pages": args.limit,
                "timeout_seconds": args.timeout_seconds
            }
        });
        contents.push(Content::text(serde_json::to_string_pretty(&metadata)?));

        Ok(contents)
    }

    fn prompt_arguments() -> Vec<PromptArgument> {
        vec![]
    }

    async fn prompt(&self, _args: Self::PromptArgs) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::text("How do I crawl a website?"),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "The start_crawl tool initiates background web crawling:\n\n\
                     **Basic usage:**\n\
                     ```json\n\
                     start_crawl({\"url\": \"https://ratatui.rs\"})\n\
                     ```\n\n\
                     **With options:**\n\
                     ```json\n\
                     start_crawl({\n\
                       \"url\": \"https://docs.rs\",\n\
                       \"output_dir\": \"docs/docs.rs\",\n\
                       \"max_depth\": 2,\n\
                       \"limit\": 50,\n\
                       \"save_markdown\": true,\n\
                       \"enable_search\": true\n\
                     })\n\
                     ```\n\n\
                     **Key features:**\n\
                     - Runs in background (returns immediately)\n\
                     - Auto-generates output_dir from URL if not specified\n\
                     - Saves as markdown, HTML, and JSON\n\
                     - Optional search indexing (Tantivy full-text)\n\
                     - Rate limiting (default 2 req/sec)\n\
                     - Progress tracking via get_crawl_results\n\n\
                     **Recommended workflow:**\n\
                     1. start_crawl({\"url\": \"https://example.com\"})\n\
                     2. Returns crawl_id\n\
                     3. Poll with get_crawl_results({\"crawl_id\": \"...\"})\n\
                     4. Search with search_crawl_results({\"output_dir\": \"docs/example.com\", \"query\": \"...\"})",
                ),
            },
        ])
    }
}
