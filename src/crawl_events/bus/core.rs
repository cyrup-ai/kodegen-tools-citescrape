//! Core CrawlEventBus struct definition and constructors

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use tokio::sync::{Mutex, Notify, broadcast};

use crate::crawl_events::config::EventBusConfig;
use crate::crawl_events::metrics::EventBusMetrics;
use crate::crawl_events::types::CrawlEvent;

/// Event bus for publishing and subscribing to crawl events
#[derive(Debug)]
pub struct CrawlEventBus {
    pub(super) sender: broadcast::Sender<CrawlEvent>,
    pub(super) config: Arc<EventBusConfig>,
    pub(super) metrics: EventBusMetrics,
    pub(super) shutdown: Arc<Notify>,
    pub(super) shutdown_flag: Arc<AtomicBool>,
    pub(super) capacity_notify: Arc<Notify>,
    pub(super) send_lock: Arc<Mutex<()>>,
    /// Tracks consecutive publish timeout failures for circuit breaker
    pub(super) consecutive_timeouts: Arc<AtomicUsize>,
    /// Reference count for tracking CrawlEventBus instances (for proper Drop semantics)
    pub(super) num_instances: Arc<AtomicUsize>,
}

impl CrawlEventBus {
    /// Create a new event bus with the specified capacity
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of events that can be buffered
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let config = EventBusConfig {
            capacity,
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create a new event bus with custom configuration
    ///
    /// # Arguments
    /// * `config` - Event bus configuration
    #[must_use]
    pub fn with_config(config: EventBusConfig) -> Self {
        let (sender, _) = broadcast::channel(config.capacity);
        let metrics = EventBusMetrics::new();
        let shutdown = Arc::new(Notify::new());
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let capacity_notify = Arc::new(Notify::new());
        let send_lock = Arc::new(Mutex::new(()));
        let consecutive_timeouts = Arc::new(AtomicUsize::new(0));
        let num_instances = Arc::new(AtomicUsize::new(1));
        Self {
            sender,
            config: Arc::new(config),
            metrics,
            shutdown,
            shutdown_flag,
            capacity_notify,
            send_lock,
            consecutive_timeouts,
            num_instances,
        }
    }

    /// Get the current configuration
    #[must_use]
    pub fn config(&self) -> &EventBusConfig {
        &self.config
    }

    /// Get current metrics
    ///
    /// # Consistency Notes
    ///
    /// Returns a reference to the metrics object. Individual counter reads
    /// are atomic, but relationships between counters may be temporarily
    /// inconsistent during concurrent operations. For a consistent view
    /// across all metrics, use `metrics().snapshot()`.
    ///
    /// # Example
    ///
    /// ```rust
    /// // Individual reads (may be inconsistent)
    /// let published = bus.metrics().get_published();
    /// let dropped = bus.metrics().get_dropped();
    ///
    /// // Consistent snapshot
    /// let snapshot = bus.metrics().snapshot();
    /// assert!(snapshot.events_published >= snapshot.events_dropped);
    /// ```
    #[must_use]
    pub fn metrics(&self) -> &EventBusMetrics {
        &self.metrics
    }

    /// Get current channel pressure (0.0 to 1.0+)
    ///
    /// Returns the ratio of used capacity to total capacity.
    /// - 0.0 = empty
    /// - 0.5 = half full
    /// - 1.0 = at capacity
    /// - >1.0 = impossible (channel drops oldest events)
    ///
    /// # Example
    /// ```
    /// let bus = CrawlEventBus::new(1000);
    /// // ... publish some events ...
    /// let pressure = bus.pressure();
    /// if pressure > 0.8 {
    ///     log::warn!("Channel is {}% full", pressure * 100.0);
    /// }
    /// ```
    #[must_use]
    pub fn pressure(&self) -> f64 {
        let current = self.sender.len();
        let capacity = self.config.capacity;
        current as f64 / capacity as f64
    }

    /// Check if channel is overloaded
    ///
    /// Returns true if pressure exceeds the configured threshold
    /// (default 0.8 = 80% capacity)
    ///
    /// # Example
    /// ```
    /// if bus.is_overloaded() {
    ///     log::warn!("Event bus overloaded, consider slowing down");
    ///     tokio::time::sleep(Duration::from_millis(10)).await;
    /// }
    /// ```
    #[must_use]
    pub fn is_overloaded(&self) -> bool {
        self.pressure() >= self.config.overload_threshold
    }

    /// Get current number of events in the channel buffer
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.sender.len()
    }

    /// Get remaining capacity before channel is full
    #[must_use]
    pub fn remaining_capacity(&self) -> usize {
        self.config.capacity.saturating_sub(self.sender.len())
    }
}
