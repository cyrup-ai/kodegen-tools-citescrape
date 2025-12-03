//! Crawl session registry - manages multiple crawl instances with connection isolation
//!
//! Pattern based on: packages/kodegen-tools-terminal/src/registry.rs

use crate::mcp::session::CrawlSession;
use crate::mcp::manager::SearchEngineCache;
use kodegen_mcp_schema::citescrape::{CrawlSnapshot, ScrapeUrlOutput};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Registry key: (connection_id, crawl_id)
type CrawlMap = HashMap<(String, u32), Arc<CrawlSession>>;

/// Registry for managing multiple crawl instances keyed by (connection_id, crawl_id)
///
/// **Connection Isolation:**
/// - Each MCP connection gets its own namespace of crawl instances
/// - crawl:0 for connection_A is different from crawl:0 for connection_B
/// - Prevents cross-connection interference
///
/// **Instance Numbering:**
/// - Users can run parallel crawls: crawl:0, crawl:1, crawl:2...
/// - Each instance is stateful and reusable
/// - Same pattern as terminal tool
#[derive(Clone)]
pub struct CrawlRegistry {
    crawls: Arc<Mutex<CrawlMap>>,
    engine_cache: Arc<SearchEngineCache>,
}

impl CrawlRegistry {
    /// Create a new crawl registry
    pub fn new(engine_cache: Arc<SearchEngineCache>) -> Self {
        Self {
            crawls: Arc::new(Mutex::new(HashMap::new())),
            engine_cache,
        }
    }

    /// Find or create a crawl session
    ///
    /// Pattern from: terminal/registry.rs:25-47
    pub async fn find_or_create_crawl(
        &self,
        connection_id: &str,
        crawl_id: u32,  // Instance ID, not action
        output_dir: PathBuf,
    ) -> Result<Arc<CrawlSession>, anyhow::Error> {
        let key = (connection_id.to_string(), crawl_id);
        let mut crawls = self.crawls.lock().await;

        if let Some(session) = crawls.get(&key) {
            return Ok(session.clone());
        }

        let session = Arc::new(
            CrawlSession::new(
                crawl_id,
                output_dir,
                self.engine_cache.clone(),
            )
        );

        crawls.insert(key, session.clone());
        Ok(session)
    }

    /// List all active crawls for a connection with their current states
    ///
    /// Pattern from: terminal/registry.rs:49-79
    pub async fn list_all_crawls(
        &self,
        connection_id: &str,
    ) -> Result<ScrapeUrlOutput, anyhow::Error> {
        let crawls = self.crawls.lock().await;
        let mut snapshots = Vec::new();

        for ((conn_id, crawl_num), session) in crawls.iter() {
            if conn_id == connection_id {
                let state = session.get_current_state().await?;
                snapshots.push(CrawlSnapshot {
                    crawl_id: *crawl_num,
                    status: state.status.clone(),
                    url: state.current_url.clone(),
                    pages_crawled: state.pages_crawled,
                    elapsed_ms: 0,
                });
            }
        }

        // Sort by crawl ID
        snapshots.sort_by_key(|s| s.crawl_id);

        Ok(ScrapeUrlOutput {
            crawl_id: 0,
            status: "list".to_string(),
            url: None,
            pages_crawled: 0,
            pages_queued: 0,
            output_dir: None,
            elapsed_ms: 0,
            completed: true,
            error: None,
            crawls: Some(snapshots),
            search_results: None,
        })
    }

    /// Kill a crawl and cleanup all resources
    ///
    /// Pattern from: terminal/registry.rs:81-104
    pub async fn kill_crawl(
        &self,
        _connection_id: &str,
        crawl_id: u32,
    ) -> Result<ScrapeUrlOutput, anyhow::Error> {
        let key = (_connection_id.to_string(), crawl_id);
        let mut crawls = self.crawls.lock().await;

        if let Some(session) = crawls.remove(&key) {
            // Cancel the crawl if running
            session.cancel().await?;

            Ok(ScrapeUrlOutput {
                crawl_id,
                status: "killed".to_string(),
                url: None,
                pages_crawled: 0,
                pages_queued: 0,
                output_dir: None,
                elapsed_ms: 0,
                completed: true,
                error: None,
                crawls: None,
                search_results: None,
            })
        } else {
            Ok(ScrapeUrlOutput {
                crawl_id,
                status: "not_found".to_string(),
                url: None,
                pages_crawled: 0,
                pages_queued: 0,
                output_dir: None,
                elapsed_ms: 0,
                completed: true,
                error: Some(format!("Crawl {} not found", crawl_id)),
                crawls: None,
                search_results: None,
            })
        }
    }

    /// Cleanup all crawls for a connection (called on connection drop)
    ///
    /// Cancels running crawls but preserves output directories at docs/<domain>/
    pub async fn cleanup_connection(&self, connection_id: &str) -> usize {
        let mut crawls = self.crawls.lock().await;
        let to_remove: Vec<(String, u32)> = crawls
            .keys()
            .filter(|(conn_id, _)| conn_id == connection_id)
            .cloned()
            .collect();
        
        let count = to_remove.len();
        for key in to_remove {
            if let Some(session) = crawls.remove(&key) {
                log::debug!(
                    "Cleaning up crawl {} for connection {} (preserving output directory)",
                    key.1,
                    connection_id
                );
                // Cancel the crawl if running (output dir is preserved)
                let _ = session.cancel().await;
            }
        }
        count
    }
}
