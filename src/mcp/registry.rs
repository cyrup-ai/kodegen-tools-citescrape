//! Crawl session registry - manages multiple crawl instances with connection isolation
//!
//! Pattern based on: packages/kodegen-tools-terminal/src/registry.rs

use crate::mcp::session::CrawlSession;
use crate::mcp::manager::SearchEngineCache;
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
    ) -> Result<serde_json::Value, anyhow::Error> {
        let crawls = self.crawls.lock().await;
        let mut snapshots = Vec::new();

        for ((conn_id, crawl_num), session) in crawls.iter() {
            if conn_id == connection_id {
                let state = session.get_current_state().await?;
                snapshots.push(serde_json::json!({
                    "crawl": crawl_num,
                    "output_dir": state.output_dir,
                    "status": state.status,
                    "pages_crawled": state.pages_crawled,
                    "current_url": state.current_url,
                }));
            }
        }

        // Sort by crawl ID
        snapshots.sort_by_key(|v| v["crawl"].as_u64().unwrap_or(0));

        Ok(serde_json::json!({
            "connection_id": connection_id,
            "total_crawls": snapshots.len(),
            "crawls": snapshots,
        }))
    }

    /// Kill a crawl and cleanup all resources
    ///
    /// Pattern from: terminal/registry.rs:81-104
    pub async fn kill_crawl(
        &self,
        connection_id: &str,
        crawl_id: u32,
    ) -> Result<serde_json::Value, anyhow::Error> {
        let key = (connection_id.to_string(), crawl_id);
        let mut crawls = self.crawls.lock().await;

        if let Some(session) = crawls.remove(&key) {
            // Cancel the crawl if running
            session.cancel().await?;

            Ok(serde_json::json!({
                "status": "killed",
                "crawl": crawl_id,
                "connection_id": connection_id,
            }))
        } else {
            Ok(serde_json::json!({
                "status": "not_found",
                "crawl": crawl_id,
                "connection_id": connection_id,
            }))
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
