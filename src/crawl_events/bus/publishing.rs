//! Publishing operations for the CrawlEventBus

use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::crawl_events::config::BackpressureMode;
use crate::crawl_events::errors::EventBusError;
use crate::crawl_events::types::{BatchPublishResult, CrawlEvent};

use super::core::CrawlEventBus;

impl CrawlEventBus {
    /// Publish an event to all subscribers
    ///
    /// # Arguments
    /// * `event` - The event to publish
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of active subscribers that received the event
    /// * `Err(EventBusError)` - If publishing failed
    pub async fn publish(&self, event: CrawlEvent) -> Result<usize, EventBusError> {
        if let Ok(subscriber_count) = self.sender.send(event) {
            if self.config.enable_metrics {
                self.metrics.increment_published();
                self.metrics.update_subscriber_count(subscriber_count);

                if subscriber_count == 0 {
                    self.metrics.increment_dropped();
                    log::debug!("Published event but no active subscribers");
                }
            }
            Ok(subscriber_count)
        } else {
            if self.config.enable_metrics {
                self.metrics.increment_failed();
            }
            Err(EventBusError::NoSubscribers)
        }
    }

    /// Publish an event with backpressure control
    ///
    /// Unlike the basic `publish()` method, this method respects the
    /// configured backpressure mode:
    ///
    /// - **`DropOldest`**: Same as `publish()`, never blocks
    /// - **Block**: Waits until space is available (applies backpressure)
    /// - **Error**: Returns `ChannelFull` error if at capacity
    ///
    /// # Arguments
    /// * `event` - The event to publish
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of subscribers that received the event
    /// * `Err(EventBusError::ChannelFull)` - Channel at capacity (Error mode only)
    /// * `Err(EventBusError::NoSubscribers)` - No active subscribers
    ///
    /// # Example with Error Mode
    /// ```
    /// let config = EventBusConfig {
    ///     backpressure_mode: BackpressureMode::Error,
    ///     ..Default::default()
    /// };
    /// let bus = CrawlEventBus::with_config(config);
    ///
    /// match bus.publish_with_backpressure(event).await {
    ///     Ok(count) => log::info!("Published to {} subscribers", count),
    ///     Err(EventBusError::ChannelFull) => {
    ///         log::warn!("Channel full, dropping event or retry later");
    ///     }
    ///     Err(e) => log::error!("Publish failed: {}", e),
    /// }
    /// ```
    ///
    /// # Example with Block Mode
    /// ```
    /// let config = EventBusConfig {
    ///     backpressure_mode: BackpressureMode::Block,
    ///     ..Default::default()
    /// };
    /// let bus = CrawlEventBus::with_config(config);
    ///
    /// // This will wait until space is available
    /// let count = bus.publish_with_backpressure(event).await?;
    /// ```
    pub async fn publish_with_backpressure(
        &self,
        event: CrawlEvent,
    ) -> Result<usize, EventBusError> {
        match self.config.backpressure_mode {
            BackpressureMode::DropOldest => {
                // Delegate to publish() - same behavior, no duplication
                self.publish(event).await
            }

            BackpressureMode::Block => {
                // Circuit breaker: check if we've exceeded timeout threshold
                let timeout_count = self.consecutive_timeouts.load(Ordering::Acquire);
                if timeout_count > 10 {
                    log::warn!(
                        "Circuit breaker opened after {timeout_count} consecutive timeouts, falling back to async mode"
                    );
                    // Fall back to DropOldest mode to prevent complete system hang
                    return self.publish(event).await;
                }

                // Wrap the blocking wait in a 30-second timeout to prevent deadlocks
                let publish_future = async {
                    // Wait until space is available using notification + timeout fallback
                    loop {
                        // Check if we have space
                        if self.sender.len() < self.config.capacity {
                            break;
                        }

                        // Check if bus is shutdown
                        if self.is_shutdown() {
                            return Err(EventBusError::Shutdown);
                        }

                        // Wait for capacity notification OR timeout (5ms fallback)
                        // Timeout ensures we recheck even if notification is missed
                        let _ = tokio::time::timeout(
                            tokio::time::Duration::from_millis(5),
                            self.capacity_notify.notified(),
                        )
                        .await;
                    }

                    // Now publish (should succeed since we have space)
                    if let Ok(subscriber_count) = self.sender.send(event) {
                        if self.config.enable_metrics {
                            self.metrics.increment_published();
                            self.metrics.update_subscriber_count(subscriber_count);

                            if subscriber_count == 0 {
                                self.metrics.increment_dropped();
                            }
                        }

                        // Wake one waiting publisher (if any) now that we've published
                        // This creates a chain where publishers wake each other
                        self.capacity_notify.notify_one();

                        Ok(subscriber_count)
                    } else {
                        if self.config.enable_metrics {
                            self.metrics.increment_failed();
                        }
                        Err(EventBusError::NoSubscribers)
                    }
                };

                // Apply 30-second timeout to prevent indefinite deadlock
                match tokio::time::timeout(Duration::from_secs(30), publish_future).await {
                    Ok(Ok(count)) => {
                        // Success: reset the timeout counter
                        self.consecutive_timeouts.store(0, Ordering::Release);
                        Ok(count)
                    }
                    Ok(Err(e)) => {
                        // Publish failed but not due to timeout
                        Err(e)
                    }
                    Err(_elapsed) => {
                        // Timeout occurred: increment counter and check circuit breaker
                        let new_count =
                            self.consecutive_timeouts.fetch_add(1, Ordering::AcqRel) + 1;

                        if new_count > 10 {
                            // Circuit breaker will trigger on next call
                            log::error!(
                                "Publish timeout #{new_count}: circuit breaker will open on next attempt"
                            );
                        } else {
                            log::warn!(
                                "Publish timeout #{new_count} after 30s waiting for channel capacity"
                            );
                        }

                        Err(EventBusError::PublishTimeout)
                    }
                }
            }

            BackpressureMode::Error => {
                // Acquire lock to serialize check-and-send (eliminates TOCTOU race)
                let _guard = self.send_lock.lock().await;

                // Check and send are now atomic (serialized by mutex)
                if self.sender.len() >= self.config.capacity {
                    return Err(EventBusError::ChannelFull);
                }

                // Send with reserved slot (protected by lock)
                if let Ok(subscriber_count) = self.sender.send(event) {
                    if self.config.enable_metrics {
                        self.metrics.increment_published();
                        self.metrics.update_subscriber_count(subscriber_count);

                        if subscriber_count == 0 {
                            self.metrics.increment_dropped();
                        }
                    }
                    Ok(subscriber_count)
                } else {
                    if self.config.enable_metrics {
                        self.metrics.increment_failed();
                    }
                    Err(EventBusError::NoSubscribers)
                }
                // Lock automatically released when _guard drops
            }
        }
    }

    /// Publish multiple events as a batch with best-effort delivery
    ///
    /// This method publishes all events in the batch regardless of individual failures.
    /// Unlike a transactional approach, partial success is acceptable and fully reported.
    ///
    /// # Best-Effort Semantics
    ///
    /// All events are attempted. Failures (typically due to no active subscribers) don't
    /// stop processing of remaining events. Returns a `BatchPublishResult` with explicit
    /// counts showing exactly how many succeeded vs failed.
    ///
    /// # Arguments
    ///
    /// * `events` - Vector of events to publish
    ///
    /// # Returns
    ///
    /// `BatchPublishResult` with detailed success/failure statistics
    ///
    /// # Example
    ///
    /// ```rust
    /// let events = vec![
    ///     CrawlEvent::page_crawled(...),
    ///     CrawlEvent::page_crawled(...),
    ///     CrawlEvent::crawl_completed(...),
    /// ];
    ///
    /// let result = bus.publish_batch(events).await;
    /// println!("Published {}/{} events to {} subscribers",
    ///          result.published, result.total, result.max_subscribers);
    ///
    /// if result.has_failures() {
    ///     log::warn!("{} events failed (no subscribers)", result.failed);
    /// }
    ///
    /// if result.is_complete() {
    ///     log::info!("All events delivered successfully");
    /// }
    /// ```
    pub async fn publish_batch(&self, events: Vec<CrawlEvent>) -> BatchPublishResult {
        let total = events.len();
        let mut published = 0;
        let mut failed = 0;
        let mut max_subscribers = 0;

        for event in events {
            if let Ok(count) = self.sender.send(event) {
                published += 1;
                max_subscribers = std::cmp::max(max_subscribers, count);

                if self.config.enable_metrics {
                    self.metrics.increment_published();
                    self.metrics.update_subscriber_count(count);
                    if count == 0 {
                        self.metrics.increment_dropped();
                    }
                }
            } else {
                failed += 1;
                if self.config.enable_metrics {
                    self.metrics.increment_failed();
                }
            }
        }

        BatchPublishResult {
            total,
            published,
            failed,
            max_subscribers,
        }
    }
}
