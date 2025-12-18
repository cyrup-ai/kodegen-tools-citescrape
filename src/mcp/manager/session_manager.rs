//! Crawl session management for tracking active background crawls
//!
//! Provides thread-safe session tracking with automatic cleanup of completed/failed sessions.

use super::types::{ActiveCrawlSession, CrawlStatus};
use log;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Initial capacity for crawl session `HashMap`
///
/// Pre-allocates space for this many active sessions to reduce allocations.
/// Based on typical usage: users run 5-20 concurrent crawls per server instance.
const SESSION_CACHE_INITIAL_CAPACITY: usize = 16;

/// Manager for tracking active crawl sessions
///
/// Uses `tokio::sync::Mutex` for async-safe concurrent access.
/// Sessions automatically tracked by `crawl_id` (UUID v4).
#[derive(Clone)]
pub struct CrawlSessionManager {
    sessions: Arc<Mutex<HashMap<String, ActiveCrawlSession>>>,
}

impl CrawlSessionManager {
    /// Create a new session manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::with_capacity(
                SESSION_CACHE_INITIAL_CAPACITY,
            ))),
        }
    }

    /// Register a new crawl session
    pub async fn register(&self, crawl_id: String, session: ActiveCrawlSession) {
        let mut sessions = self.sessions.lock().await;
        sessions.insert(crawl_id, session);
    }

    /// Update status for a crawl session
    pub async fn update_status(&self, crawl_id: &str, status: CrawlStatus) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(crawl_id) {
            session.status = status;
        }
    }

    /// Update progress counters for a crawl session
    pub async fn update_progress(
        &self,
        crawl_id: &str,
        total_pages: usize,
        current_url: Option<String>,
    ) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(crawl_id) {
            session.total_pages = total_pages;
            if let Some(url) = current_url {
                session.current_url = Some(url);
            }
        }
    }

    /// Get a clone of a crawl session
    pub async fn get_session(&self, crawl_id: &str) -> Option<ActiveCrawlSession> {
        let sessions = self.sessions.lock().await;
        sessions.get(crawl_id).cloned()
    }

    /// Remove and return a crawl session
    pub async fn remove_session(&self, crawl_id: &str) -> Option<ActiveCrawlSession> {
        let mut sessions = self.sessions.lock().await;
        sessions.remove(crawl_id)
    }

    /// List all active crawl IDs
    pub async fn list_active(&self) -> Vec<String> {
        let sessions = self.sessions.lock().await;
        sessions.keys().cloned().collect()
    }

    /// Cleanup completed/failed sessions older than retention period
    ///
    /// Removes sessions that are in Completed or Failed status and older than 5 minutes.
    /// This prevents unbounded growth of the sessions `HashMap` over time.
    /// Also aborts task handles for removed sessions to free up resources.
    // APPROVED BY DAVID MAPLE 10/17/2025 - False positive: called via start_cleanup_task background task
    async fn cleanup_sessions(&self) {
        use std::time::Duration;

        let cutoff = Duration::from_secs(5 * 60); // 5 minutes
        let now = chrono::Utc::now();
        let mut sessions = self.sessions.lock().await;
        let initial_count = sessions.len();

        sessions.retain(|crawl_id, session| {
            let age = now.signed_duration_since(session.start_time);
            let is_terminal = matches!(
                session.status,
                CrawlStatus::Completed | CrawlStatus::Failed { .. }
            );

            // Keep running sessions, remove terminal sessions older than cutoff
            let should_keep = !is_terminal || age.to_std().unwrap_or(Duration::ZERO) < cutoff;

            if !should_keep {
                log::debug!(
                    "Removing old crawl session {}: {:?} (age: {:?})",
                    crawl_id,
                    session.status,
                    age
                );

                // No background tasks to abort in long-running pattern
                // Sessions are only retained during execute() and for post-crawl search queries
            }

            should_keep
        });

        let cleaned = initial_count - sessions.len();
        if cleaned > 0 {
            log::debug!("Cleaned up {cleaned} crawl sessions");
        }
    }

    /// Start background cleanup task (call once at initialization)
    ///
    /// Spawns a tokio task that runs cleanup every 60 seconds.
    /// Follows the same pattern as `TerminalManager` and `SearchManager`.
    ///
    /// # Usage
    /// Called from main.rs after wrapping manager in Arc:
    /// ```rust,no_run
    /// use kodegen_tools_citescrape::mcp::manager::CrawlSessionManager;
    /// use std::sync::Arc;
    /// 
    /// let session_manager = Arc::new(CrawlSessionManager::new());
    /// session_manager.clone().start_cleanup_task();
    /// ```
    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                self.cleanup_sessions().await;
            }
        });
    }
}

impl Default for CrawlSessionManager {
    fn default() -> Self {
        Self::new()
    }
}
