//! Pre-warmed Chrome browser pool with dynamic scaling
//!
//! Provides instant browser access by maintaining a pool of pre-warmed Chrome instances.
//! Pool size dynamically scales based on demand: target = max(in_use + 2, min_pool_size).

use anyhow::{Context, Result};
use chromiumoxide::browser::Browser;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for the browser pool
#[derive(Debug, Clone)]
pub struct BrowserPoolConfig {
    /// Minimum browsers to maintain in pool (default: 2)
    pub min_pool_size: usize,
    /// Maximum browsers allowed (default: 10)
    pub max_pool_size: usize,
    /// Interval between keepalive pings (default: 30s)
    pub keepalive_interval: Duration,
    /// Remove browsers idle longer than this (default: 5 minutes)
    pub idle_timeout: Duration,
    /// Run browsers in headless mode (default: true)
    pub headless: bool,
}

impl Default for BrowserPoolConfig {
    fn default() -> Self {
        Self {
            min_pool_size: 2,
            max_pool_size: 10,
            keepalive_interval: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            headless: true,
        }
    }
}

// =============================================================================
// Browser Wrapper (Pool-specific)
// =============================================================================

/// RAII wrapper for pooled browser instance
///
/// Similar to [`web_search::browser::BrowserWrapper`](../web_search/browser.rs) but
/// designed for pool lifecycle management. Created fresh here to avoid visibility issues.
///
/// The browser is stored in an `Arc` to allow sharing across concurrent tasks
/// while the guard manages the lifecycle.
#[derive(Debug)]
pub struct PooledBrowserWrapper {
    browser: Arc<Browser>,
    handler: JoinHandle<()>,
    user_data_dir: Option<PathBuf>,
}

impl PooledBrowserWrapper {
    pub(crate) fn new(browser: Browser, handler: JoinHandle<()>, user_data_dir: PathBuf) -> Self {
        Self {
            browser: Arc::new(browser),
            handler,
            user_data_dir: Some(user_data_dir),
        }
    }

    /// Get reference to inner browser
    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    /// Get Arc-wrapped browser for sharing across concurrent tasks
    pub fn browser_arc(&self) -> Arc<Browser> {
        Arc::clone(&self.browser)
    }

    /// Get mutable reference to inner browser (only works if no other Arc refs exist)
    pub fn browser_mut(&mut self) -> Option<&mut Browser> {
        Arc::get_mut(&mut self.browser)
    }

    /// Clean up temp directory (blocking operation)
    pub fn cleanup_temp_dir(&mut self) {
        if let Some(path) = self.user_data_dir.take() {
            info!("Cleaning up pool browser temp directory: {}", path.display());
            if let Err(e) = std::fs::remove_dir_all(&path) {
                tracing::warn!(
                    "Failed to clean up temp directory {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }
}

impl Drop for PooledBrowserWrapper {
    fn drop(&mut self) {
        info!("Dropping PooledBrowserWrapper - aborting handler task");
        self.handler.abort();
        if self.user_data_dir.is_some() {
            self.cleanup_temp_dir();
        }
    }
}

// =============================================================================
// Pooled Browser Instance
// =============================================================================

/// A browser instance with pool metadata
#[derive(Debug)]
pub struct PooledBrowser {
    /// Unique identifier for this browser instance
    pub id: u64,
    /// The wrapped browser with handler
    pub wrapper: PooledBrowserWrapper,
    /// When this browser was launched
    pub created_at: Instant,
    /// Last time this browser was used (acquired or returned)
    pub last_used: Instant,
    /// Last successful health check
    pub last_health_check: Instant,
}

impl PooledBrowser {
    fn new(id: u64, wrapper: PooledBrowserWrapper) -> Self {
        let now = Instant::now();
        Self {
            id,
            wrapper,
            created_at: now,
            last_used: now,
            last_health_check: now,
        }
    }
}

// =============================================================================
// Browser Pool
// =============================================================================

/// Pre-warmed browser pool with dynamic scaling
#[derive(Debug)]
pub struct BrowserPool {
    config: BrowserPoolConfig,
    /// Available (ready) browsers
    available: Arc<Mutex<VecDeque<PooledBrowser>>>,
    /// Count of browsers currently checked out
    in_use_count: AtomicUsize,
    /// Counter for unique browser IDs
    next_id: AtomicU64,
    /// Background scaler task handle
    scaler_handle: Mutex<Option<JoinHandle<()>>>,
    /// Background keepalive task handle
    keepalive_handle: Mutex<Option<JoinHandle<()>>>,
    /// Shutdown signal
    shutdown: AtomicBool,
}

impl BrowserPool {
    /// Create a new browser pool (does NOT start background tasks)
    pub fn new(config: BrowserPoolConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            available: Arc::new(Mutex::new(VecDeque::new())),
            in_use_count: AtomicUsize::new(0),
            next_id: AtomicU64::new(0),
            scaler_handle: Mutex::new(None),
            keepalive_handle: Mutex::new(None),
            shutdown: AtomicBool::new(false),
        })
    }

    /// Start the pool and background tasks
    ///
    /// Pre-warms the pool to min_pool_size and starts scaler/keepalive tasks.
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        info!("Starting browser pool with config: {:?}", self.config);

        // Pre-warm to minimum size
        self.scale_to_target().await?;

        // Start background scaler (every 5 seconds)
        let pool_clone = Arc::clone(self);
        let scaler = tokio::spawn(async move {
            scaler_loop(pool_clone).await;
        });
        *self.scaler_handle.lock().await = Some(scaler);

        // Start background keepalive
        let pool_clone = Arc::clone(self);
        let keepalive = tokio::spawn(async move {
            keepalive_loop(pool_clone).await;
        });
        *self.keepalive_handle.lock().await = Some(keepalive);

        info!(
            "Browser pool started with {} pre-warmed browsers",
            self.available.lock().await.len()
        );
        Ok(())
    }

    /// Acquire a browser from the pool
    ///
    /// Returns a guard that automatically releases the browser when dropped.
    /// If no browsers available, waits briefly then launches new one if under max.
    pub async fn acquire(self: &Arc<Self>) -> Result<PooledBrowserGuard> {
        loop {
            let mut available = self.available.lock().await;

            if let Some(mut browser) = available.pop_front() {
                // Health check before handing out
                match browser.wrapper.browser().version().await {
                    Ok(_) => {
                        browser.last_used = Instant::now();
                        browser.last_health_check = Instant::now();
                        self.in_use_count.fetch_add(1, Ordering::Relaxed);
                        debug!("Acquired browser {} from pool", browser.id);

                        return Ok(PooledBrowserGuard {
                            browser: Some(browser),
                            pool: Arc::clone(self),
                        });
                    }
                    Err(e) => {
                        warn!(
                            "Browser {} failed health check during acquire: {}",
                            browser.id, e
                        );
                        continue;
                    }
                }
            }

            drop(available);

            // No healthy browser available - launch new one if under max
            let total =
                self.in_use_count.load(Ordering::Relaxed) + self.available.lock().await.len();

            if total < self.config.max_pool_size {
                let browser = self.launch_browser_internal().await?;
                self.in_use_count.fetch_add(1, Ordering::Relaxed);
                debug!(
                    "Launched new browser {} for acquire (pool was empty)",
                    browser.id
                );

                return Ok(PooledBrowserGuard {
                    browser: Some(browser),
                    pool: Arc::clone(self),
                });
            }

            warn!(
                "Browser pool at max capacity ({}), waiting...",
                self.config.max_pool_size
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Release a browser back to the pool
    fn release(&self, mut browser: PooledBrowser) {
        self.in_use_count.fetch_sub(1, Ordering::Relaxed);
        browser.last_used = Instant::now();

        let available = Arc::clone(&self.available);
        let id = browser.id;

        tokio::spawn(async move {
            available.lock().await.push_back(browser);
            debug!("Released browser {} back to pool", id);
        });
    }

    /// Gracefully shutdown the pool
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down browser pool");
        self.shutdown.store(true, Ordering::Relaxed);

        if let Some(handle) = self.scaler_handle.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.keepalive_handle.lock().await.take() {
            handle.abort();
        }

        let mut available = self.available.lock().await;
        while let Some(mut browser) = available.pop_front() {
            // Try to get mutable access - only works if no other Arc refs exist
            if let Some(b) = browser.wrapper.browser_mut() {
                if let Err(e) = b.close().await {
                    warn!("Failed to close browser {}: {}", browser.id, e);
                }
                let _ = b.wait().await;
            } else {
                // Other refs exist, just log and let Drop handle cleanup
                warn!("Browser {} has outstanding references, skipping graceful close", browser.id);
            }
            browser.wrapper.cleanup_temp_dir();
        }

        info!("Browser pool shutdown complete");
        Ok(())
    }

    /// Calculate target pool size: max(in_use + 2, min_pool_size)
    fn target_pool_size(&self) -> usize {
        let in_use = self.in_use_count.load(Ordering::Relaxed);
        (in_use + 2)
            .max(self.config.min_pool_size)
            .min(self.config.max_pool_size)
    }

    /// Scale pool to target size
    async fn scale_to_target(&self) -> Result<()> {
        let target = self.target_pool_size();
        let current = self.available.lock().await.len();

        if current >= target {
            return Ok(());
        }

        let to_launch = target - current;
        debug!(
            "Scaling pool: launching {} browsers (current={}, target={})",
            to_launch, current, target
        );

        let futs: Vec<_> = (0..to_launch)
            .map(|_| self.launch_browser_internal())
            .collect();

        let results = futures::future::join_all(futs).await;

        let mut available = self.available.lock().await;
        for result in results {
            match result {
                Ok(browser) => {
                    available.push_back(browser);
                }
                Err(e) => {
                    warn!("Failed to launch browser for pool: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Launch a new browser instance using [`browser_setup::launch_browser`](../browser_setup.rs)
    async fn launch_browser_internal(&self) -> Result<PooledBrowser> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Create unique temp directory for this pooled browser using UUID
        let profile = crate::browser_profile::create_unique_profile_with_prefix("kodegen_chrome_pool")
            .context("Failed to create unique pool browser profile")?;
        let user_data_dir = profile.into_path();

        // Use the existing browser_setup::launch_browser with correct signature
        let (browser, handler, _returned_dir) =
            crate::browser_setup::launch_browser(self.config.headless, Some(user_data_dir.clone()))
                .await
                .context("Failed to launch browser for pool")?;

        let wrapper = PooledBrowserWrapper::new(browser, handler, user_data_dir);
        Ok(PooledBrowser::new(id, wrapper))
    }
}

// =============================================================================
// RAII Guard
// =============================================================================

/// RAII guard that returns browser to pool on drop
pub struct PooledBrowserGuard {
    browser: Option<PooledBrowser>,
    pool: Arc<BrowserPool>,
}

impl PooledBrowserGuard {
    /// Get reference to the underlying Browser
    pub fn browser(&self) -> &Browser {
        self.browser.as_ref().expect("browser should be present").wrapper.browser()
    }

    /// Get Arc-wrapped browser for sharing across concurrent tasks
    ///
    /// This is the primary method for use in the orchestrator, where the browser
    /// needs to be cloned and passed to spawned tasks.
    pub fn browser_arc(&self) -> Arc<Browser> {
        self.browser.as_ref().expect("browser should be present").wrapper.browser_arc()
    }

    /// Get the browser's unique pool ID
    pub fn id(&self) -> u64 {
        self.browser.as_ref().expect("browser should be present").id
    }
}

impl Drop for PooledBrowserGuard {
    fn drop(&mut self) {
        if let Some(browser) = self.browser.take() {
            self.pool.release(browser);
        }
    }
}

// =============================================================================
// Background Tasks
// =============================================================================

/// Background task: Scale pool to target size every 5 seconds
async fn scaler_loop(pool: Arc<BrowserPool>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    while !pool.shutdown.load(Ordering::Relaxed) {
        interval.tick().await;

        if let Err(e) = pool.scale_to_target().await {
            warn!("Pool scaler error: {}", e);
        }

        // Remove idle browsers beyond min_pool_size
        let mut available = pool.available.lock().await;
        let now = Instant::now();
        let min_size = pool.config.min_pool_size;

        while available.len() > min_size {
            if let Some(browser) = available.front() {
                if now.duration_since(browser.last_used) > pool.config.idle_timeout {
                    if let Some(removed) = available.pop_front() {
                        debug!(
                            "Removing idle browser {} (idle {:?})",
                            removed.id,
                            now.duration_since(removed.last_used)
                        );
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    debug!("Scaler loop exiting");
}

/// Background task: Keepalive ping every 30 seconds using `browser.version()` CDP command
async fn keepalive_loop(pool: Arc<BrowserPool>) {
    let mut interval = tokio::time::interval(pool.config.keepalive_interval);

    while !pool.shutdown.load(Ordering::Relaxed) {
        interval.tick().await;

        let mut available = pool.available.lock().await;
        let mut healthy = VecDeque::new();

        while let Some(mut browser) = available.pop_front() {
            match browser.wrapper.browser().version().await {
                Ok(version) => {
                    browser.last_health_check = Instant::now();
                    healthy.push_back(browser);
                    debug!("Browser health check OK: {}", version.product);
                }
                Err(e) => {
                    warn!(
                        "Browser {} failed keepalive health check: {}",
                        browser.id, e
                    );
                }
            }
        }

        *available = healthy;
        debug!(
            "Keepalive complete: {} healthy browsers in pool",
            available.len()
        );
    }

    debug!("Keepalive loop exiting");
}
