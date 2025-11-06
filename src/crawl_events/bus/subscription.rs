//! Subscription operations for the CrawlEventBus

use tokio::sync::broadcast;

use crate::crawl_events::streaming::FilteredReceiver;
use crate::crawl_events::types::CrawlEvent;

use super::core::CrawlEventBus;

impl CrawlEventBus {
    /// Subscribe to events
    ///
    /// # Returns
    /// A receiver that can be used to listen for events
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<CrawlEvent> {
        self.sender.subscribe()
    }

    /// Get the number of active subscribers
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        let count = self.sender.receiver_count();
        if self.config.enable_metrics {
            self.metrics.update_subscriber_count(count);
        }
        count
    }

    /// Check if the event bus has any active subscribers
    #[must_use]
    pub fn has_subscribers(&self) -> bool {
        self.subscriber_count() > 0
    }

    /// Create a filtered subscriber that only receives specific event types
    ///
    /// # Arguments
    /// * `filter` - Function that returns true if event should be passed through
    pub fn subscribe_filtered<F>(&self, filter: F) -> FilteredReceiver<F>
    where
        F: Fn(&CrawlEvent) -> bool + Send + Sync + 'static,
    {
        let receiver = self.subscribe();
        FilteredReceiver::new(receiver, filter)
    }
}
