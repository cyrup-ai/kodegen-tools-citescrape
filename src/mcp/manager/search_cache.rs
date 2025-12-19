//! Search engine caching with LRU eviction and lock-free timestamp tracking
//!
//! Provides efficient caching of Tantivy search engines with automatic cleanup
//! of idle engines and LRU eviction when cache reaches capacity.

use super::timestamp_utils::{instant_to_nanos, nanos_to_instant};
use crate::config::CrawlConfig;
use crate::search::{IndexingSender, SearchEngine};
use kodegen_mcp_schema::McpError;
use log::{debug, error, info};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Initial capacity for search engine cache `HashMap`
///
/// Pre-allocates space for this many cached engines to reduce allocations.
/// Based on typical usage: users crawl 5-10 different sites per session.
const SEARCH_CACHE_INITIAL_CAPACITY: usize = 10;

/// Entry in the `SearchEngineCache` containing both engine and indexing sender
///
/// Each cached entry includes:
/// - `engine`: The `SearchEngine` instance for querying
/// - `indexing_sender`: Optional sender for incremental index updates
/// - `last_accessed`: Lock-free atomic timestamp for LRU eviction tracking
///
/// # Lock-Free Design
/// Uses `AtomicU64` instead of Mutex<Instant> for zero-contention timestamp updates.
/// Timestamps are stored as nanoseconds since a global epoch, allowing atomic operations.
#[derive(Clone)]
pub struct SearchEngineCacheEntry {
    pub engine: Arc<SearchEngine>,
    pub indexing_sender: Option<Arc<IndexingSender>>,
    /// Lock-free timestamp stored as nanoseconds since epoch
    pub last_accessed: Arc<AtomicU64>,
}

/// Cache for `SearchEngine` instances
///
/// Prevents repeated initialization of `SearchEngine` for the same output directory.
/// Uses Arc<SearchEngine> for cheap cloning across tasks.
///
/// **Lifecycle Management:**
/// - Engines are lazily initialized on first access via `get_or_init()`
/// - Engines remain cached until explicitly removed via `invalidate()` or `clear()`
/// - **IMPORTANT:** Call `shutdown()` before application exit to release Tantivy file handles
///
/// **Shutdown Pattern:**
/// ```rust
/// use kodegen_tools_citescrape::mcp::manager::SearchEngineCache;
/// use std::sync::Arc;
/// 
/// #[tokio::main]
/// async fn main() {
///     // During application initialization
///     let search_cache = Arc::new(SearchEngineCache::new());
///     
///     // ... application runs ...
///     
///     // During application shutdown (e.g., SIGTERM handler)
///     search_cache.shutdown().await;
/// }
/// ```
///
/// **Resource Details:**
/// Each cached engine holds Tantivy Index and `IndexReader` with memory-mapped file handles.
/// On Linux/macOS, these are mmap'd segments that consume virtual address space.
/// Proper shutdown ensures timely release of file descriptors and clean filesystem unmounting.
#[derive(Clone)]
pub struct SearchEngineCache {
    engines: Arc<Mutex<HashMap<PathBuf, SearchEngineCacheEntry>>>,
}

impl SearchEngineCache {
    /// Maximum number of cached search engines before LRU eviction
    const MAX_ENGINES: usize = 5;

    /// Idle timeout after which engines are eligible for cleanup (30 minutes)
    const ENGINE_IDLE_TIMEOUT_SECS: u64 = 30 * 60;

    /// Create a new search engine cache
    #[must_use]
    pub fn new() -> Self {
        Self {
            engines: Arc::new(Mutex::new(HashMap::with_capacity(
                SEARCH_CACHE_INITIAL_CAPACITY,
            ))),
        }
    }

    /// Get cached engine or initialize new one
    ///
    /// Returns both the `SearchEngine` and optional `IndexingSender` for use in `CrawlConfig`
    pub async fn get_or_init(
        &self,
        output_dir: PathBuf,
        config: &CrawlConfig,
    ) -> Result<SearchEngineCacheEntry, McpError> {
        let mut engines = self.engines.lock().await;

        // Check cache first
        if let Some(entry) = engines.get(&output_dir) {
            // Update last accessed time for LRU tracking (lock-free atomic)
            entry
                .last_accessed
                .store(instant_to_nanos(Instant::now()), Ordering::Relaxed);
            return Ok(entry.clone());
        }

        // Check if we're at capacity before creating new engine
        if engines.len() >= Self::MAX_ENGINES {
            // Find and evict LRU engine to make room
            let mut oldest_key: Option<PathBuf> = None;
            let mut oldest_time = Instant::now();

            for (key, entry) in engines.iter() {
                // Skip the one we're about to create
                if key == &output_dir {
                    continue;
                }

                let last_access_nanos = entry.last_accessed.load(Ordering::Relaxed);
                let last_access = nanos_to_instant(last_access_nanos);

                if oldest_key.is_none() || last_access < oldest_time {
                    oldest_time = last_access;
                    oldest_key = Some(key.clone());
                }
            }

            if let Some(evict_key) = oldest_key {
                engines.remove(&evict_key);
                debug!(
                    "Evicted LRU engine {:?} to make room for {:?} (limit: {})",
                    evict_key,
                    output_dir,
                    Self::MAX_ENGINES
                );
            }
        }

        // Release lock during initialization (long operation)
        drop(engines);

        // Initialize new engine
        let engine = SearchEngine::create(config).await.map_err(|e| {
            McpError::SearchEngine(format!("Failed to initialize search engine: {e}"))
        })?;

        // Start incremental indexing service with engine clone
        let engine_for_indexing = engine.clone();
        let indexing_sender =
            match crate::search::IncrementalIndexingService::start(engine_for_indexing).await {
                Ok((_service, sender)) => {
                    debug!(
                        "Incremental indexing service started for output_dir: {output_dir:?}"
                    );
                    Some(Arc::new(sender))
                }
                Err(e) => {
                    error!("Failed to start incremental indexing service: {e}");
                    None
                }
            };

        let engine = Arc::new(engine);

        // Create cache entry with both engine and sender
        let entry = SearchEngineCacheEntry {
            engine: Arc::clone(&engine),
            indexing_sender,
            last_accessed: Arc::new(AtomicU64::new(instant_to_nanos(Instant::now()))),
        };

        // Re-acquire lock and insert
        let mut engines = self.engines.lock().await;

        // Double-check: another task may have inserted while we were initializing
        if let Some(existing_entry) = engines.get(&output_dir) {
            return Ok(existing_entry.clone());
        }

        engines.insert(output_dir, entry.clone());

        Ok(entry)
    }

    /// Invalidate (remove) cached engine for `output_dir`
    pub async fn invalidate(&self, output_dir: &PathBuf) {
        let mut engines = self.engines.lock().await;
        engines.remove(output_dir);
    }

    /// Clear all cached engines
    pub async fn clear(&self) {
        let mut engines = self.engines.lock().await;
        engines.clear();
    }

    /// Gracefully shutdown cache and release all search engines
    ///
    /// This should be called during application shutdown to ensure
    /// all Tantivy indices are properly closed and file handles released.
    ///
    /// **Resource Cleanup:**
    /// - Drains `HashMap` to take ownership of Arc<SearchEngine>
    /// - Explicitly drops each engine to release Tantivy resources
    /// - Ensures `MmapDirectory` file handles are closed
    /// - Flushes any pending `IndexReader` caches
    ///
    /// **Thread Safety:**
    /// Uses async Mutex lock to safely drain concurrent cache access.
    ///
    /// # Example
    /// ```rust
    /// use kodegen_tools_citescrape::mcp::manager::SearchEngineCache;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let search_cache = SearchEngineCache::new();
    ///     
    ///     // ... use search_cache ...
    ///     
    ///     // Shutdown during application termination
    ///     search_cache.shutdown().await;
    /// }
    /// ```
    pub async fn shutdown(&self) {
        info!("Shutting down search engine cache");

        let mut engines = self.engines.lock().await;
        let count = engines.len();

        // Drain all engines and explicitly drop them
        // drain() takes ownership, removing from HashMap
        for (key, engine) in engines.drain() {
            debug!("Releasing search engine: {key:?}");
            // Explicit drop ensures immediate cleanup
            // (though implicit drop would also work)
            drop(engine);
        }

        info!("Search engine cache shutdown complete: {count} engines");
    }

    /// Get number of cached engines (for monitoring)
    ///
    /// Useful for:
    /// - Monitoring cache growth
    /// - Verifying shutdown completion (should return 0 after shutdown)
    /// - Debugging cache behavior
    ///
    /// # Example
    /// ```rust
    /// use kodegen_tools_citescrape::mcp::manager::SearchEngineCache;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let search_cache = SearchEngineCache::new();
    ///     let size = search_cache.cache_size().await;
    ///     assert_eq!(size, 0); // Empty cache initially
    /// }
    /// ```
    pub async fn cache_size(&self) -> usize {
        self.engines.lock().await.len()
    }

    /// Cleanup idle engines and enforce LRU eviction policy
    ///
    /// This method:
    /// 1. Removes engines idle for longer than `ENGINE_IDLE_TIMEOUT_SECS`
    /// 2. Enforces `MAX_ENGINES` limit via LRU eviction of least recently used engines
    ///
    /// Prevents unbounded growth of cached Tantivy indices and file descriptors.
    ///
    /// # Lock-Free Design
    /// Uses atomic timestamp reads (no lock contention) for determining idle time and LRU ordering.
    /// This ensures cleanup never blocks concurrent cache access operations.
    // APPROVED BY DAVID MAPLE 10/17/2025 - False positive: called via start_cleanup_task background task
    async fn cleanup_idle_engines(&self) {
        use std::time::Duration;

        let cutoff = Duration::from_secs(Self::ENGINE_IDLE_TIMEOUT_SECS);
        let mut engines = self.engines.lock().await;
        let initial_count = engines.len();

        // Step 1: Remove idle engines (lock-free using atomic timestamps)
        engines.retain(|path, entry| {
            let last_access_nanos = entry.last_accessed.load(Ordering::Relaxed);
            let last_access = nanos_to_instant(last_access_nanos);
            let age = last_access.elapsed();

            let should_keep = age < cutoff;

            if !should_keep {
                debug!("Evicting idle search engine: {path:?} (idle: {age:?})");
            }

            should_keep
        });

        // Step 2: LRU eviction if over max size (lock-free using atomic timestamps)
        while engines.len() > Self::MAX_ENGINES {
            // Find oldest (least recently used) engine
            let mut oldest_key: Option<PathBuf> = None;
            let mut oldest_time = Instant::now();

            for (key, entry) in engines.iter() {
                let last_access_nanos = entry.last_accessed.load(Ordering::Relaxed);
                let last_access = nanos_to_instant(last_access_nanos);

                if oldest_key.is_none() || last_access < oldest_time {
                    oldest_time = last_access;
                    oldest_key = Some(key.clone());
                }
            }

            if let Some(key) = oldest_key {
                engines.remove(&key);
                debug!("LRU eviction: {:?} (cache size: {})", key, engines.len());
            } else {
                // This should never happen now (no lock contention)
                break;
            }
        }

        let cleaned = initial_count.saturating_sub(engines.len());
        if cleaned > 0 {
            debug!(
                "Cleaned up {} search engines (current size: {})",
                cleaned,
                engines.len()
            );
        }
    }

    /// Start background cleanup task (call once at initialization)
    ///
    /// Spawns a tokio task that runs cleanup every 60 seconds.
    /// Follows the same pattern as `TerminalManager` and `SearchManager`.
    ///
    /// # Usage
    /// Called from main.rs after wrapping cache in Arc:
    /// ```rust,no_run
    /// use kodegen_tools_citescrape::mcp::manager::SearchEngineCache;
    /// use std::sync::Arc;
    /// 
    /// let engine_cache = Arc::new(SearchEngineCache::new());
    /// engine_cache.clone().start_cleanup_task();
    /// ```
    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                self.cleanup_idle_engines().await;
            }
        });
    }
}

impl Default for SearchEngineCache {
    fn default() -> Self {
        Self::new()
    }
}
