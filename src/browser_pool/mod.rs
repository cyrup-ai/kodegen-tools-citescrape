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
use tokio::sync::mpsc;
use tokio::sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

// =============================================================================
// Cleanup Channel Types
// =============================================================================

/// Message types for async cleanup channel
pub(crate) enum CleanupMessage {
    /// Path to a temp directory that needs cleanup
    Path(PathBuf),
    /// Signal to shut down the cleanup task
    Shutdown,
}

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
    /// Interval between idle browser cleanup runs (default: 60s)
    pub cleanup_interval: Duration,
    /// Run browsers in headless mode (default: true)
    pub headless: bool,
    /// Timeout for individual health check CDP calls (default: 5s)
    pub health_check_timeout: Duration,
    /// Timeout for graceful shutdown waiting for in-use browsers (default: 30s)
    pub shutdown_timeout: Duration,
}

impl Default for BrowserPoolConfig {
    fn default() -> Self {
        Self {
            min_pool_size: 2,
            max_pool_size: 10,
            keepalive_interval: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            cleanup_interval: Duration::from_secs(60),
            headless: true,
            health_check_timeout: Duration::from_secs(5),
            shutdown_timeout: Duration::from_secs(30),
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
    cleanup_tx: mpsc::UnboundedSender<CleanupMessage>,
    /// Capacity permit - auto-released when browser is destroyed
    _permit: OwnedSemaphorePermit,
}

impl PooledBrowserWrapper {
    pub(crate) fn new(
        browser: Browser,
        handler: JoinHandle<()>,
        user_data_dir: PathBuf,
        cleanup_tx: mpsc::UnboundedSender<CleanupMessage>,
        permit: OwnedSemaphorePermit,
    ) -> Self {
        Self {
            browser: Arc::new(browser),
            handler,
            user_data_dir: Some(user_data_dir),
            cleanup_tx,
            _permit: permit,
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

    /// Spawn async cleanup of temp directory (non-blocking)
    ///
    /// This spawns a background task to clean up the temp directory
    /// using `tokio::fs::remove_dir_all` instead of blocking the runtime.
    pub fn spawn_cleanup(mut self) {
        if let Some(path) = self.user_data_dir.take() {
            tokio::spawn(async move {
                info!("Async cleanup: removing {}", path.display());
                if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                    warn!("Failed to clean up temp directory {}: {}", path.display(), e);
                }
            });
        }
        // Handler is aborted in Drop, which runs after this method
    }

    /// Take ownership of user_data_dir path for async cleanup
    ///
    /// After calling this, Drop will NOT perform blocking cleanup.
    /// Caller is responsible for async cleanup via `tokio::fs::remove_dir_all`.
    pub fn take_user_data_dir(&mut self) -> Option<PathBuf> {
        self.user_data_dir.take()
    }
}

impl Drop for PooledBrowserWrapper {
    fn drop(&mut self) {
        info!("Dropping PooledBrowserWrapper - aborting handler task");
        self.handler.abort();

        // Non-blocking: send path to cleanup channel instead of blocking delete
        if let Some(path) = self.user_data_dir.take() {
            // Ignore send errors - channel may be closed during shutdown
            let _ = self.cleanup_tx.send(CleanupMessage::Path(path));
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
    /// Enforces max_pool_size atomically - each browser holds one permit
    capacity_semaphore: Arc<Semaphore>,
    /// Count of browsers currently checked out (monitoring only, not for gating)
    in_use_count: AtomicUsize,
    /// Counter for unique browser IDs
    next_id: AtomicU64,
    /// Background scaler task handle
    scaler_handle: Mutex<Option<JoinHandle<()>>>,
    /// Background keepalive task handle
    keepalive_handle: Mutex<Option<JoinHandle<()>>>,
    /// Background cleanup task handle
    cleanup_handle: Mutex<Option<JoinHandle<()>>>,
    /// Sender for async cleanup requests
    cleanup_tx: mpsc::UnboundedSender<CleanupMessage>,
    /// Receiver stored until start() (then moved to cleanup task)
    cleanup_rx: Mutex<Option<mpsc::UnboundedReceiver<CleanupMessage>>>,
    /// Shutdown signal
    shutdown: AtomicBool,
    /// Notified when a browser is released (for shutdown wait)
    release_notify: Notify,

    // Channel-based release queue
    /// Sender for returning browsers to pool (sync Mutex for use in Drop context)
    release_tx: std::sync::Mutex<Option<mpsc::UnboundedSender<PooledBrowser>>>,
    /// Receiver for the release channel (taken by start() to spawn release task)
    release_rx: Mutex<Option<mpsc::UnboundedReceiver<PooledBrowser>>>,
    /// Handle to the release task for shutdown coordination
    release_task_handle: Mutex<Option<JoinHandle<()>>>,
    /// Notification sent when a browser is released back to pool (for acquire waiters)
    available_notify: Arc<Notify>,
}

impl BrowserPool {
    /// Create a new browser pool (does NOT start background tasks)
    pub fn new(config: BrowserPoolConfig) -> Arc<Self> {
        let (release_tx, release_rx) = mpsc::unbounded_channel();
        let (cleanup_tx, cleanup_rx) = mpsc::unbounded_channel();

        Arc::new(Self {
            capacity_semaphore: Arc::new(Semaphore::new(config.max_pool_size)),
            config,
            available: Arc::new(Mutex::new(VecDeque::new())),
            in_use_count: AtomicUsize::new(0),
            next_id: AtomicU64::new(0),
            scaler_handle: Mutex::new(None),
            keepalive_handle: Mutex::new(None),
            cleanup_handle: Mutex::new(None),
            cleanup_tx,
            cleanup_rx: Mutex::new(Some(cleanup_rx)),
            shutdown: AtomicBool::new(false),
            release_notify: Notify::new(),
            // Initialize channel
            release_tx: std::sync::Mutex::new(Some(release_tx)),
            release_rx: Mutex::new(Some(release_rx)),
            release_task_handle: Mutex::new(None),
            available_notify: Arc::new(Notify::new()),
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

        // Start background cleanup task
        if let Some(cleanup_rx) = self.cleanup_rx.lock().await.take() {
            let cleanup = tokio::spawn(async move {
                cleanup_loop(cleanup_rx).await;
            });
            *self.cleanup_handle.lock().await = Some(cleanup);
        }

        // Start the release queue task
        let release_rx = self.release_rx.lock().await.take();
        if let Some(rx) = release_rx {
            let pool_clone = Arc::clone(self);
            let release_task = tokio::spawn(async move {
                release_loop(pool_clone, rx).await;
            });
            *self.release_task_handle.lock().await = Some(release_task);
        } else {
            warn!("start() called multiple times or release_rx already taken");
        }

        info!(
            "Browser pool started with {} pre-warmed browsers",
            self.available.lock().await.len()
        );
        Ok(())
    }

    /// Acquire a browser from the pool with timeout
    ///
    /// Returns a guard that automatically releases the browser when dropped.
    /// If no browsers available, waits with exponential backoff up to timeout.
    ///
    /// # Arguments
    /// * `timeout` - Maximum time to wait for a browser
    ///
    /// # Errors
    /// Returns error if timeout expires, browser launch fails, or pool is shutting down
    pub async fn acquire_timeout(
        self: &Arc<Self>,
        timeout: Duration,
    ) -> Result<PooledBrowserGuard> {
        // Check shutdown flag first - reject new acquisitions during shutdown
        if self.shutdown.load(Ordering::Acquire) {
            return Err(anyhow::anyhow!("Browser pool is shutting down"));
        }

        let deadline = Instant::now() + timeout;
        let mut backoff = Duration::from_millis(10);
        let max_backoff = Duration::from_secs(1);
        let mut wait_logged = false;
        let health_timeout = self.config.health_check_timeout;

        loop {
            // Check timeout first
            let now = Instant::now();
            if now >= deadline {
                return Err(anyhow::anyhow!(
                    "Timeout after {:?} waiting for browser from pool",
                    timeout
                ));
            }

            // Also check shutdown inside loop in case it started while waiting
            if self.shutdown.load(Ordering::Acquire) {
                return Err(anyhow::anyhow!("Browser pool is shutting down"));
            }

            // Phase 1: Pop browser from pool (brief lock)
            let browser = {
                let mut available = self.available.lock().await;
                available.pop_front()
            };
            // Lock released immediately

            if let Some(mut browser) = browser {
                // Phase 2: Health check WITHOUT holding lock
                let health_result = tokio::time::timeout(
                    health_timeout,
                    browser.wrapper.browser().version()
                ).await;

                match health_result {
                    Ok(Ok(_)) => {
                        // Browser is healthy - return it
                        browser.last_used = Instant::now();
                        browser.last_health_check = Instant::now();
                        self.in_use_count.fetch_add(1, Ordering::AcqRel);

                        if wait_logged {
                            debug!("Acquired browser {} after waiting", browser.id);
                        } else {
                            debug!("Acquired browser {} from pool", browser.id);
                        }

                        return Ok(PooledBrowserGuard {
                            browser: Some(browser),
                            pool: Arc::clone(self),
                        });
                    }
                    Ok(Err(e)) => {
                        warn!("Browser {} failed health check during acquire: {}", browser.id, e);
                        // Spawn async cleanup and try next browser
                        let PooledBrowser { id, mut wrapper, .. } = browser;
                        if let Some(path) = wrapper.user_data_dir.take() {
                            tokio::spawn(async move {
                                debug!("Async cleanup for failed browser {}", id);
                                let _ = tokio::fs::remove_dir_all(&path).await;
                            });
                        }
                        continue;
                    }
                    Err(_) => {
                        warn!("Browser {} health check timed out during acquire", browser.id);
                        // Spawn async cleanup and try next browser
                        let PooledBrowser { id, mut wrapper, .. } = browser;
                        if let Some(path) = wrapper.user_data_dir.take() {
                            tokio::spawn(async move {
                                debug!("Async cleanup for timed out browser {}", id);
                                let _ = tokio::fs::remove_dir_all(&path).await;
                            });
                        }
                        continue;
                    }
                }
            }

            // No browser available - try to launch new one
            // Semaphore in launch_browser_internal() atomically gates capacity (no TOCTOU race!)
            match self.launch_browser_internal().await {
                Ok(browser) => {
                    self.in_use_count.fetch_add(1, Ordering::AcqRel);
                    debug!(
                        "Launched new browser {} for acquire (pool was empty)",
                        browser.id
                    );

                    return Ok(PooledBrowserGuard {
                        browser: Some(browser),
                        pool: Arc::clone(self),
                    });
                }
                Err(e) => {
                    // Check if this is a capacity error (semaphore exhausted)
                    let err_msg = e.to_string();
                    if err_msg.contains("capacity") || err_msg.contains("semaphore") {
                        // At max capacity - fall through to wait with backoff
                    } else {
                        // Real launch error - propagate it
                        return Err(e);
                    }
                }
            }

            // At max capacity - wait with backoff
            if !wait_logged {
                warn!(
                    "Browser pool at max capacity ({}), waiting (timeout: {:?})",
                    self.config.max_pool_size, timeout
                );
                wait_logged = true;
            }

            // Calculate remaining time
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(anyhow::anyhow!(
                    "Timeout after {:?} waiting for browser from pool",
                    timeout
                ));
            }

            // Wait for notification OR backoff timeout (whichever is shorter)
            let wait_time = backoff.min(remaining);
            tokio::select! {
                biased;

                _ = self.available_notify.notified() => {
                    // Browser released, reset backoff and retry immediately
                    backoff = Duration::from_millis(10);
                }
                _ = tokio::time::sleep(wait_time) => {
                    // Timeout, increase backoff for next iteration
                    backoff = (backoff * 2).min(max_backoff);
                }
            }
        }
    }

    /// Acquire a browser from the pool with default 30-second timeout
    ///
    /// Returns a guard that automatically releases the browser when dropped.
    /// If no browsers available, waits briefly then launches new one if under max.
    pub async fn acquire(self: &Arc<Self>) -> Result<PooledBrowserGuard> {
        self.acquire_timeout(Duration::from_secs(30)).await
    }

    /// Release a browser back to the pool via the release queue
    ///
    /// This method is called from PooledBrowserGuard::drop() (sync context).
    /// The browser is sent through an unbounded channel to the release_loop
    /// background task, which handles the actual queue insertion and
    /// in_use_count decrement.
    fn release(&self, mut browser: PooledBrowser) {
        browser.last_used = Instant::now();
        let id = browser.id;

        // Clone sender from mutex-protected Option
        let tx = {
            let guard = match self.release_tx.lock() {
                Ok(g) => g,
                Err(poisoned) => {
                    warn!("Release mutex poisoned, browser {} will be dropped", id);
                    // Still get the guard to attempt sending
                    poisoned.into_inner()
                }
            };
            guard.clone()
        };

        match tx {
            Some(tx) => {
                if tx.send(browser).is_err() {
                    // Channel closed (shutdown in progress)
                    warn!("Release channel closed, browser {} will be dropped", id);
                }
            }
            None => {
                // Sender was taken during shutdown
                warn!("Pool shutting down, browser {} will be dropped during release", id);
            }
        }
    }

    /// Gracefully shutdown the pool
    ///
    /// Waits for in-use browsers to be returned (with timeout), then closes
    /// all browsers and cleans up resources.
    ///
    /// # Behavior
    /// 1. Sets shutdown flag to prevent new acquisitions
    /// 2. Aborts background scaler and keepalive tasks
    /// 3. Waits for in-use browsers to be returned (with configurable timeout)
    /// 4. Closes release channel and waits for release task to drain
    /// 5. Drains and closes all available browsers
    /// 6. Cleans up temp directories
    ///
    /// If timeout expires while browsers are still in use, those browsers will
    /// be closed when their guards are dropped (via the modified release_loop).
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down browser pool");
        
        // Set shutdown flag with SeqCst ordering to ensure ordering with semaphore close
        self.shutdown.store(true, Ordering::SeqCst);

        // Close semaphore to unblock any waiting acquire_owned() calls
        // and prevent new browser launches
        self.capacity_semaphore.close();

        // Abort background scaling/keepalive tasks first
        if let Some(handle) = self.scaler_handle.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.keepalive_handle.lock().await.take() {
            handle.abort();
        }

        // Wait for in-use browsers to be returned (with timeout)
        let deadline = Instant::now() + self.config.shutdown_timeout;
        let initial_in_use = self.in_use_count.load(Ordering::Acquire);
        
        if initial_in_use > 0 {
            info!(
                "Waiting for {} in-use browser(s) to be returned (timeout: {:?})",
                initial_in_use, self.config.shutdown_timeout
            );
        }
        
        while self.in_use_count.load(Ordering::Acquire) > 0 {
            if Instant::now() > deadline {
                let orphaned = self.in_use_count.load(Ordering::Acquire);
                warn!(
                    "Shutdown timeout: {} browser(s) still in use after {:?}. \
                     They will be closed when their guards are dropped.",
                    orphaned, self.config.shutdown_timeout
                );
                break;
            }
            
            // Wait for release notification or poll every 100ms
            tokio::select! {
                _ = self.release_notify.notified() => {
                    debug!(
                        "Browser released during shutdown wait ({} remaining)",
                        self.in_use_count.load(Ordering::Acquire)
                    );
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Poll timeout, check again
                }
            }
        }

        // Close the release channel by taking the sender
        // This signals the release task to drain remaining items and exit
        {
            let mut tx_guard = match self.release_tx.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            *tx_guard = None; // Drop sender, closing channel
        }

        // Wait for release task to complete (drains pending releases)
        if let Some(handle) = self.release_task_handle.lock().await.take() {
            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(Ok(())) => debug!("Release task completed cleanly"),
                Ok(Err(e)) => warn!("Release task panicked: {:?}", e),
                Err(_) => {
                    warn!("Timeout waiting for release task after 5s");
                    // Task will be aborted when handle is dropped
                }
            }
        }

        // Drain and close all available browsers
        let mut available = self.available.lock().await;
        let browser_count = available.len();

        while let Some(mut browser) = available.pop_front() {
            // Try to get mutable access - only works if no other Arc refs exist
            if let Some(b) = browser.wrapper.browser_mut() {
                if let Err(e) = b.close().await {
                    warn!("Failed to close browser {}: {}", browser.id, e);
                }
                let _ = b.wait().await;
            } else {
                warn!("Browser {} has outstanding references, skipping graceful close", browser.id);
            }
            // Use synchronous cleanup during shutdown for guaranteed cleanup
            browser.wrapper.cleanup_temp_dir();
        }
        drop(available);

        // Signal cleanup task to shutdown and wait for completion
        let _ = self.cleanup_tx.send(CleanupMessage::Shutdown);
        if let Some(handle) = self.cleanup_handle.lock().await.take() {
            // Wait for pending cleanups to complete (with timeout)
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                handle
            ).await;
        }

        let final_in_use = self.in_use_count.load(Ordering::Acquire);
        if final_in_use > 0 {
            warn!(
                "Browser pool shutdown complete: {} browser(s) closed, {} orphaned",
                browser_count, final_in_use
            );
        } else {
            info!("Browser pool shutdown complete: {} browser(s) closed", browser_count);
        }
        
        Ok(())
    }

    /// Calculate target pool size with hysteresis to prevent oscillation
    ///
    /// Hysteresis band: +/- 2 browsers
    /// - Scale UP only when significantly below target (current < target - 1)
    /// - Scale DOWN only when significantly above target (current > target + 2)
    /// - Otherwise maintain current size (in hysteresis band)
    fn target_pool_size_with_hysteresis(&self, current_available: usize) -> usize {
        let in_use = self.in_use_count.load(Ordering::Acquire);
        let current_total = in_use + current_available;

        // Base target: in_use + 2 buffer, clamped to [min, max]
        let base_target = (in_use + 2)
            .max(self.config.min_pool_size)
            .min(self.config.max_pool_size);

        // Apply hysteresis band
        if current_total < base_target.saturating_sub(1) {
            // Significantly below target - scale up
            base_target
        } else if current_total > base_target + 2 {
            // Significantly above target - scale down
            base_target
        } else {
            // Within hysteresis band - maintain current
            current_total.min(self.config.max_pool_size)
        }
    }

    /// Scale pool to target size (uses hysteresis)
    async fn scale_to_target(&self) -> Result<()> {
        let current = self.available.lock().await.len();
        let target = self.target_pool_size_with_hysteresis(current);

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

    /// Remove idle browsers with non-blocking cleanup
    ///
    /// Identifies idle browsers under brief lock, then spawns async cleanup
    /// tasks for each removed browser's temp directory.
    async fn remove_idle_browsers(&self) {
        let now = Instant::now();
        let min_size = self.config.min_pool_size;
        let idle_timeout = self.config.idle_timeout;

        // Collect browsers to remove with paths (brief lock)
        let mut to_cleanup: Vec<PathBuf> = Vec::new();
        {
            let mut available = self.available.lock().await;

            // Remove from front (oldest first) while above min_size
            while available.len() > min_size {
                if let Some(browser) = available.front() {
                    if now.duration_since(browser.last_used) > idle_timeout {
                        if let Some(mut removed) = available.pop_front() {
                            debug!(
                                "Removing idle browser {} (idle {:?})",
                                removed.id,
                                now.duration_since(removed.last_used)
                            );
                            // Extract path BEFORE drop to prevent blocking cleanup
                            if let Some(path) = removed.wrapper.take_user_data_dir() {
                                to_cleanup.push(path);
                            }
                            // Browser dropped here - but no blocking I/O since path was taken
                        }
                    } else {
                        // Front browser is not idle, none behind it will be either
                        // (VecDeque maintains insertion order, oldest at front)
                        break;
                    }
                } else {
                    break;
                }
            }
        } // Lock released here

        // Spawn async cleanup tasks outside the lock
        for path in to_cleanup {
            tokio::spawn(async move {
                info!("Async cleanup of browser temp directory: {}", path.display());
                if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                    warn!("Failed to clean up temp directory {}: {}", path.display(), e);
                }
            });
        }
    }

    /// Launch a new browser instance with atomic capacity enforcement
    ///
    /// Acquires a semaphore permit BEFORE launching - this is the atomic gate
    /// that prevents over-provisioning. The permit is stored in the wrapper
    /// and auto-released when the browser is destroyed.
    async fn launch_browser_internal(&self) -> Result<PooledBrowser> {
        // Atomic gate: try to acquire permit BEFORE any browser creation
        // This prevents TOCTOU race - the semaphore is the single source of truth
        let permit = self
            .capacity_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| anyhow::anyhow!("Browser pool at max capacity (semaphore exhausted)"))?;

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

        // Pass the permit to the wrapper - it will be auto-released on Drop
        let wrapper = PooledBrowserWrapper::new(
            browser,
            handler,
            user_data_dir,
            self.cleanup_tx.clone(),
            permit,
        );
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

/// Background task: Scale pool and remove idle browsers on separate timers
///
/// - Scale check: every 5 seconds (fast response to demand)
/// - Idle cleanup: every 60 seconds (configurable, reduces churn)
async fn scaler_loop(pool: Arc<BrowserPool>) {
    let mut scale_interval = tokio::time::interval(Duration::from_secs(5));
    let mut cleanup_interval = tokio::time::interval(pool.config.cleanup_interval);

    // Consume first tick immediately (tokio intervals fire immediately on first tick)
    scale_interval.tick().await;
    cleanup_interval.tick().await;

    loop {
        tokio::select! {
            _ = scale_interval.tick() => {
                if pool.shutdown.load(Ordering::Acquire) {
                    break;
                }
                if let Err(e) = pool.scale_to_target().await {
                    warn!("Pool scaler error: {}", e);
                }
            }
            _ = cleanup_interval.tick() => {
                if pool.shutdown.load(Ordering::Acquire) {
                    break;
                }
                pool.remove_idle_browsers().await;
            }
        }
    }

    debug!("Scaler loop exiting");
}

/// Background task: Process browser releases from the channel
///
/// This task owns the receive end of the release channel and processes
/// browser returns to the pool. It ensures in_use_count is only decremented
/// AFTER the browser is safely back in the available queue.
///
/// During shutdown, browsers are closed immediately instead of being returned
/// to the pool.
///
/// The task exits when the channel is closed (sender dropped during shutdown).
async fn release_loop(
    pool: Arc<BrowserPool>,
    mut rx: mpsc::UnboundedReceiver<PooledBrowser>,
) {
    debug!("Release loop started");

    while let Some(mut browser) = rx.recv().await {
        let id = browser.id;

        // Check if shutdown is in progress
        if pool.shutdown.load(Ordering::Acquire) {
            // During shutdown: close browser immediately instead of returning to pool
            debug!("Closing browser {} during shutdown release", id);
            
            // Try to close browser gracefully
            if let Some(b) = browser.wrapper.browser_mut() {
                if let Err(e) = b.close().await {
                    warn!("Failed to close browser {} during shutdown: {}", id, e);
                }
                let _ = b.wait().await;
            }
            browser.wrapper.cleanup_temp_dir();
            
            // Decrement in_use_count and notify shutdown waiter
            pool.in_use_count.fetch_sub(1, Ordering::Release);
            pool.release_notify.notify_one();
            
            debug!("Browser {} closed during shutdown", id);
        } else {
            // Normal operation: return browser to available queue
            pool.available.lock().await.push_back(browser);

            // Decrement in_use_count AFTER browser is safely back
            // Using Release ordering to ensure the push_back is visible
            pool.in_use_count.fetch_sub(1, Ordering::Release);
            
            // Notify in case shutdown is waiting
            pool.release_notify.notify_one();
            // Wake one waiter immediately after browser is available
            pool.available_notify.notify_one();

            debug!("Released browser {} back to pool", id);
        }
    }

    debug!("Release loop exiting (channel closed)");
}

/// Background task: Keepalive ping every 30 seconds using `browser.version()` CDP command
///
/// Uses parallel health checks with timeout to minimize lock contention:
/// 1. Brief lock to drain all browsers from pool
/// 2. Parallel health checks with configurable timeout (no lock held)
/// 3. Brief lock to return healthy browsers to pool
/// 4. Async cleanup spawned for failed browsers (non-blocking)
async fn keepalive_loop(pool: Arc<BrowserPool>) {
    let mut interval = tokio::time::interval(pool.config.keepalive_interval);
    let health_timeout = pool.config.health_check_timeout;

    while !pool.shutdown.load(Ordering::Acquire) {
        interval.tick().await;

        // Phase 1: Drain all browsers from pool (brief lock)
        let browsers: Vec<PooledBrowser> = {
            let mut available = pool.available.lock().await;
            std::mem::take(&mut *available).into_iter().collect()
        };
        // Lock released immediately

        if browsers.is_empty() {
            continue;
        }

        let browser_count = browsers.len();

        // Phase 2: Parallel health checks with timeout (NO LOCK HELD)
        let health_check_futures = browsers.into_iter().map(|mut browser| {
            let timeout = health_timeout;
            async move {
                match tokio::time::timeout(
                    timeout,
                    browser.wrapper.browser().version()
                ).await {
                    Ok(Ok(version)) => {
                        browser.last_health_check = Instant::now();
                        debug!("Browser {} health check OK: {}", browser.id, version.product);
                        Ok(browser)
                    }
                    Ok(Err(e)) => {
                        warn!("Browser {} failed keepalive: {}", browser.id, e);
                        Err(browser)
                    }
                    Err(_) => {
                        warn!("Browser {} health check timed out after {:?}", browser.id, timeout);
                        Err(browser)
                    }
                }
            }
        });

        let results = futures::future::join_all(health_check_futures).await;

        // Separate healthy from unhealthy browsers
        let mut healthy_browsers = VecDeque::with_capacity(browser_count);
        let mut unhealthy_browsers = Vec::new();

        for result in results {
            match result {
                Ok(browser) => healthy_browsers.push_back(browser),
                Err(browser) => unhealthy_browsers.push(browser),
            }
        }

        let healthy_count = healthy_browsers.len();
        let unhealthy_count = unhealthy_browsers.len();

        // Phase 3: Return healthy browsers to pool (brief lock)
        {
            let mut available = pool.available.lock().await;
            available.append(&mut healthy_browsers);
        }
        // Lock released immediately

        // Phase 4: Spawn async cleanup for unhealthy browsers (non-blocking)
        for browser in unhealthy_browsers {
            // Extract path before dropping browser
            let PooledBrowser { id, mut wrapper, .. } = browser;
            if let Some(path) = wrapper.user_data_dir.take() {
                tokio::spawn(async move {
                    debug!("Async cleanup for failed browser {}: {}", id, path.display());
                    if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                        warn!("Failed to clean up browser {} temp dir: {}", id, e);
                    }
                });
            }
            // wrapper.handler is aborted when wrapper is dropped here
        }

        debug!(
            "Keepalive complete: {} healthy, {} removed",
            healthy_count,
            unhealthy_count
        );
    }

    debug!("Keepalive loop exiting");
}

/// Background task: Async cleanup of temp directories
///
/// Processes cleanup messages from the channel and performs async directory
/// removal using `tokio::fs::remove_dir_all` to avoid blocking the runtime.
async fn cleanup_loop(mut rx: mpsc::UnboundedReceiver<CleanupMessage>) {
    debug!("Cleanup loop started");

    while let Some(msg) = rx.recv().await {
        match msg {
            CleanupMessage::Path(path) => {
                debug!("Async cleanup of temp directory: {}", path.display());
                if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                    warn!("Failed to clean up temp directory {}: {}", path.display(), e);
                }
            }
            CleanupMessage::Shutdown => {
                debug!("Cleanup loop received shutdown signal");
                break;
            }
        }
    }

    debug!("Cleanup loop exiting");
}
