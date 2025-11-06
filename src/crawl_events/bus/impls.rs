//! Standard trait implementations for CrawlEventBus

use std::sync::atomic::Ordering;

use crate::crawl_events::config::EventBusConfig;

use super::core::CrawlEventBus;

impl Default for CrawlEventBus {
    fn default() -> Self {
        Self::with_config(EventBusConfig::default())
    }
}

impl Clone for CrawlEventBus {
    fn clone(&self) -> Self {
        // Increment instance count (follows tokio's broadcast::Sender pattern)
        self.num_instances.fetch_add(1, Ordering::Relaxed);
        Self {
            sender: self.sender.clone(),
            config: self.config.clone(),
            metrics: self.metrics.clone(),
            shutdown: self.shutdown.clone(),
            shutdown_flag: self.shutdown_flag.clone(),
            capacity_notify: self.capacity_notify.clone(),
            send_lock: self.send_lock.clone(),
            consecutive_timeouts: self.consecutive_timeouts.clone(),
            num_instances: self.num_instances.clone(),
        }
    }
}

impl Drop for CrawlEventBus {
    fn drop(&mut self) {
        // Only shutdown when the LAST instance is dropped (follows tokio's pattern)
        // fetch_sub returns the value BEFORE decrementing
        if 1 == self.num_instances.fetch_sub(1, Ordering::AcqRel) {
            // This was the last instance - trigger shutdown
            self.shutdown_flag.store(true, Ordering::SeqCst);
            self.shutdown.notify_waiters();
            log::trace!("Event bus dropped (last instance), shutdown signal sent");
        }
    }
}
