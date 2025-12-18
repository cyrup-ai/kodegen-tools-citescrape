//! Crawl session - wraps ChromiumoxideCrawler with timeout support and state tracking

use crate::ChromiumoxideCrawler;
use crate::Crawler;  // Import the Crawler trait
use crate::config::CrawlConfig;
use crate::mcp::manager::SearchEngineCache;
use anyhow::Result;
use kodegen_mcp_schema::citescrape::{ScrapeSearchResult, ScrapeUrlOutput};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

/// Crawl session state
#[derive(Debug, Clone)]
pub struct CrawlState {
    pub output_dir: PathBuf,
    pub status: String,  // "idle", "running", "completed", "failed", "cancelled"
    pub pages_crawled: usize,
    pub current_url: Option<String>,
    pub start_time: Option<std::time::Instant>,
}

/// Crawl session wrapping ChromiumoxideCrawler with timeout and state management
pub struct CrawlSession {
    crawl_id: u32,
    output_dir: PathBuf,
    state: Arc<Mutex<CrawlState>>,
    engine_cache: Arc<SearchEngineCache>,
    /// Shared browser pool for pre-warmed Chrome instances
    browser_pool: Arc<crate::browser_pool::BrowserPool>,
}

impl CrawlSession {
    /// Create a new crawl session with browser pool
    pub fn new(
        crawl_id: u32,
        output_dir: PathBuf,
        engine_cache: Arc<SearchEngineCache>,
        browser_pool: Arc<crate::browser_pool::BrowserPool>,
    ) -> Self {
        Self {
            crawl_id,
            output_dir: output_dir.clone(),
            state: Arc::new(Mutex::new(CrawlState {
                output_dir,
                status: "idle".to_string(),
                pages_crawled: 0,
                current_url: None,
                start_time: None,
            })),
            engine_cache,
            browser_pool,
        }
    }

    /// Execute crawl with timeout support
    ///
    /// **Timeout Behavior:**
    /// - await_completion_ms = 0: Fire-and-forget, returns immediately
    /// - await_completion_ms > 0: Waits up to duration, returns partial results on timeout
    /// - Crawl continues in background after timeout
    /// - Use read_current_state() to check progress
    ///
    /// **Pattern from:** terminal tool's execute_command with timeout
    pub async fn execute_crawl_with_timeout(
        &self,
        args: kodegen_mcp_schema::citescrape::ScrapeUrlArgs,
        await_completion_ms: u64,
    ) -> Result<ScrapeUrlOutput> {
        use std::time::Instant;

        let url = args.url.ok_or_else(|| anyhow::anyhow!("url required for CRAWL action"))?;

        // Update state to running
        {
            let mut state = self.state.lock().await;
            state.status = "running".to_string();
            state.current_url = Some(url.clone());
            state.start_time = Some(Instant::now());
        }

        // Create unique Chrome user data directory for this crawl session using UUID
        // UUID ensures profile isolation between concurrent/sequential crawls
        let chrome_data_dir = crate::browser_profile::create_unique_profile_with_prefix(
            &format!("kodegen_chrome_crawl_{}", self.crawl_id)
        )?.into_path();

        // Build crawl config (reuse existing logic from start_crawl.rs)
        let mut config = CrawlConfig {
            storage_dir: self.output_dir.clone(),
            start_url: url.clone(),
            limit: args.limit,
            allow_subdomains: args.allow_subdomains,
            save_screenshots: args.save_screenshots,
            save_markdown: args.save_markdown,
            max_depth: args.max_depth,
            search_index_dir: if args.enable_search {
                Some(self.output_dir.join(".search_index"))
            } else {
                None
            },
            crawl_rate_rps: Some(args.crawl_rate_rps),
            ..Default::default()
        };

        // Attach chrome data dir to config for browser profile isolation
        config = config.with_chrome_data_dir(chrome_data_dir);

        // Attach browser pool for pre-warmed browser instances
        config = config.with_browser_pool(self.browser_pool.clone());

        // Get or initialize search engine if enabled
        if args.enable_search {
            let entry = self.engine_cache.get_or_init(self.output_dir.clone(), &config).await?;
            if let Some(indexing_sender) = entry.indexing_sender {
                config = config.with_indexing_sender(indexing_sender);
            }
        }

        // Create event bus for progress tracking
        let event_bus = Arc::new(crate::crawl_events::CrawlEventBus::new(1000));
        let state_clone = self.state.clone();
        let mut event_receiver = event_bus.subscribe();

        // Spawn progress tracker
        tokio::spawn(async move {
            let mut page_count = 0;
            while let Ok(event) = event_receiver.recv().await {
                match event {
                    crate::crawl_events::CrawlEvent::PageCrawled { url, .. } => {
                        page_count += 1;
                        let mut state = state_clone.lock().await;
                        state.pages_crawled = page_count;
                        state.current_url = Some(url);
                    }
                    crate::crawl_events::CrawlEvent::Shutdown { .. } => break,
                    _ => {}
                }
            }
        });

        config = config.with_event_bus(event_bus);

        // Create crawler and start crawl
        let crawler = ChromiumoxideCrawler::new(config);
        let crawl_future = crawler.crawl();

        // Handle timeout
        let start = Instant::now();
        let output_dir_str = self.output_dir.to_string_lossy().to_string();

        if await_completion_ms == 0 {
            // Fire-and-forget: spawn and return immediately
            tokio::spawn(async move {
                let _ = crawl_future.await;
            });

            Ok(ScrapeUrlOutput {
                crawl_id: self.crawl_id,
                status: "background".to_string(),
                url: Some(url),
                pages_crawled: 0,
                pages_queued: 0,
                output_dir: Some(output_dir_str),
                elapsed_ms: 0,
                completed: false,
                error: None,
                crawls: None,
                search_results: None,
            })
        } else {
            // Wait with timeout
            match timeout(Duration::from_millis(await_completion_ms), crawl_future).await {
                Ok(Ok(())) => {
                    // Completed successfully
                    let mut state = self.state.lock().await;
                    state.status = "completed".to_string();

                    Ok(ScrapeUrlOutput {
                        crawl_id: self.crawl_id,
                        status: "completed".to_string(),
                        url: Some(url),
                        pages_crawled: state.pages_crawled,
                        pages_queued: 0,
                        output_dir: Some(output_dir_str),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        completed: true,
                        error: None,
                        crawls: None,
                        search_results: None,
                    })
                }
                Ok(Err(e)) => {
                    // Failed
                    let mut state = self.state.lock().await;
                    state.status = "failed".to_string();

                    Err(anyhow::anyhow!("Crawl failed: {}", e))
                }
                Err(_) => {
                    // Timeout - return partial results
                    let state = self.state.lock().await;

                    Ok(ScrapeUrlOutput {
                        crawl_id: self.crawl_id,
                        status: "timeout".to_string(),
                        url: Some(url),
                        pages_crawled: state.pages_crawled,
                        pages_queued: 0,
                        output_dir: Some(output_dir_str),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        completed: false,
                        error: None,
                        crawls: None,
                        search_results: None,
                    })
                }
            }
        }
    }

    /// Read current crawl state without executing
    ///
    /// **Pattern from:** terminal tool's read_current_state()
    pub async fn read_current_state(&self) -> Result<ScrapeUrlOutput> {
        let state = self.state.lock().await;

        Ok(ScrapeUrlOutput {
            crawl_id: self.crawl_id,
            status: state.status.clone(),
            url: state.current_url.clone(),
            pages_crawled: state.pages_crawled,
            pages_queued: 0,
            output_dir: Some(self.output_dir.to_string_lossy().to_string()),
            elapsed_ms: state.start_time.map_or(0, |t| t.elapsed().as_millis() as u64),
            completed: state.status == "completed",
            error: None,
            crawls: None,
            search_results: None,
        })
    }

    /// Search indexed content (replaces scrape_search_results tool)
    ///
    /// **Intelligent Auto-Crawl:**
    /// If search index doesn't exist, automatically crawls the URL first with search enabled.
    /// This makes search a single-step operation - no manual crawl required.
    ///
    /// **Migrated from:** search_crawl_results.rs execute() method
    pub async fn search_indexed_content(
        &self,
        url: Option<String>,
        query: String,
        limit: usize,
        _offset: usize,
        highlight: bool,
        crawl_args: kodegen_mcp_schema::citescrape::ScrapeUrlArgs,
    ) -> Result<ScrapeUrlOutput> {
        use crate::search::query::SearchQueryBuilder;

        // Check if search index exists
        let search_index_dir = self.output_dir.join(".search_index");
        if !search_index_dir.join("meta.json").exists() {
            // Auto-crawl if url provided and index doesn't exist
            if let Some(ref url) = url {
                // Crawl with search enabled
                let mut auto_crawl_args = crawl_args.clone();
                auto_crawl_args.url = Some(url.clone());
                auto_crawl_args.enable_search = true;

                self.execute_crawl_with_timeout(auto_crawl_args, 600_000).await?;
            } else {
                return Err(anyhow::anyhow!(
                    "Search index not found and no URL provided for auto-crawl."
                ));
            }
        }

        // Create minimal config for search engine
        let config = crate::config::CrawlConfig {
            storage_dir: self.output_dir.clone(),
            start_url: "http://localhost".to_string(),
            search_index_dir: Some(search_index_dir),
            ..Default::default()
        };

        // Get or initialize search engine
        let entry = self.engine_cache.get_or_init(self.output_dir.clone(), &config).await?;

        // Extract domain from url for filtering
        let domain_filter = url.as_ref().and_then(|u| {
            u.strip_prefix("https://")
                .or_else(|| u.strip_prefix("http://"))
                .and_then(|s| s.split('/').next())
                .map(|s| s.to_string())
        });

        // Execute search (reuse existing SearchQueryBuilder)
        let search_results = SearchQueryBuilder::new(&query)
            .limit(limit)
            .offset(_offset)
            .highlight(highlight)
            .domain_filter(domain_filter)
            .execute_with_metadata((*entry.engine).clone())
            .await?;

        // Format results using schema type
        let results: Vec<ScrapeSearchResult> = search_results
            .results
            .iter()
            .map(|item| ScrapeSearchResult {
                url: item.url.clone(),
                title: Some(item.title.clone()),
                snippet: item.excerpt.clone(),
                score: item.score,
                path: Some(item.path.clone()),
            })
            .collect();

        Ok(ScrapeUrlOutput {
            crawl_id: self.crawl_id,
            status: "search".to_string(),
            url,
            pages_crawled: 0,
            pages_queued: 0,
            output_dir: Some(self.output_dir.to_string_lossy().to_string()),
            elapsed_ms: 0,
            completed: true,
            error: None,
            crawls: None,
            search_results: Some(results),
        })
    }

    /// Cancel the crawl
    pub async fn cancel(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.status = "cancelled".to_string();
        // Actual cancellation would need to signal the crawler
        // This is a simplified version
        Ok(())
    }

    /// Get current state (for LIST action)
    pub async fn get_current_state(&self) -> Result<CrawlState> {
        Ok(self.state.lock().await.clone())
    }
}
