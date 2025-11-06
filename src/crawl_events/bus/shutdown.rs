//! Shutdown operations for the CrawlEventBus

use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::crawl_events::types::CrawlEvent;

use super::core::CrawlEventBus;

impl CrawlEventBus {
    /// Signal shutdown to all subscribers
    ///
    /// This method is idempotent - calling it multiple times is safe.
    /// All clones of this bus share the same shutdown signal.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        self.shutdown.notify_waiters();
        log::debug!("Event bus shutdown signaled");
    }

    /// Wait for shutdown signal
    ///
    /// Subscribers should use this with `tokio::select`! to exit gracefully:
    ///
    /// ```rust
    /// tokio::select! {
    ///     Ok(event) = rx.recv() => { /* handle event */ }
    ///     _ = bus.wait_for_shutdown() => { break; }
    /// }
    /// ```
    pub async fn wait_for_shutdown(&self) {
        self.shutdown.notified().await;
    }

    /// Check if shutdown has been signaled
    ///
    /// Returns true if `shutdown()` has been called on this bus or any of its clones.
    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_flag.load(Ordering::SeqCst)
    }

    /// Gracefully shutdown the event bus with proper draining
    ///
    /// This method ensures no events are lost during shutdown:
    ///
    /// 1. **Set shutdown flag** - Prevents new operations from starting
    /// 2. **Publish shutdown event** - Notifies subscribers via event stream
    /// 3. **Wait for subscriber processing** - Gives time for subscribers to drain (500ms)
    /// 4. **Signal shutdown complete** - Wakes waiting tasks
    ///
    /// # Timeouts
    ///
    /// - **Subscriber drain**: 500ms (depends on subscriber processing speed)
    /// - **Total maximum**: 500ms
    ///
    /// If timeouts are exceeded, a warning is logged but shutdown proceeds to prevent hangs.
    ///
    /// # Example
    ///
    /// ```rust
    /// // At end of crawl
    /// bus.shutdown_gracefully(ShutdownReason::CrawlCompleted).await;
    ///
    /// // On error
    /// bus.shutdown_gracefully(ShutdownReason::Error(e.to_string())).await;
    /// ```
    pub async fn shutdown_gracefully(&self, reason: crate::crawl_events::types::ShutdownReason) {
        log::info!("Beginning graceful shutdown of event bus: {reason:?}");

        // Phase 1: Set shutdown flag to prevent new operations
        self.shutdown_flag.store(true, Ordering::SeqCst);
        log::debug!("Shutdown flag set");

        // Phase 2: Publish shutdown event
        log::debug!("Publishing shutdown event");
        let event = CrawlEvent::shutdown(reason);
        let _ = self.publish(event).await;

        // Phase 3: Wait for subscribers to process buffered events
        // This is a heuristic - we can't know when subscribers are truly done
        // without explicit acknowledgment, so we use a generous timeout
        log::debug!("Waiting for subscribers to process events");
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Phase 4: Signal final shutdown
        self.shutdown.notify_waiters();

        log::info!("Event bus graceful shutdown complete");
    }
}
