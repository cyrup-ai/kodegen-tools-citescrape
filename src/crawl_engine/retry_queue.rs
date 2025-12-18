//! Retry queue for circuit-breaker-rejected URLs
//!
//! When the circuit breaker is OPEN, URLs are preserved here instead of
//! being discarded. When the circuit transitions to HalfOpen, items are
//! drained back to the main queue for retry.

use dashmap::DashMap;
use log::info;
use std::sync::Arc;

use super::circuit_breaker::{extract_domain, CircuitBreaker};
use super::crawl_types::CrawlQueue;

/// Holds items rejected by circuit breaker for later retry
///
/// Items are keyed by domain so we can efficiently check which domains
/// have recovered (transitioned to HalfOpen or Closed).
pub struct RetryQueue {
    /// Domain -> Vec of pending items
    items: DashMap<String, Vec<CrawlQueue>>,
    /// Reference to circuit breaker for state checks
    circuit_breaker: Arc<CircuitBreaker>,
}

impl RetryQueue {
    /// Create a new retry queue linked to the given circuit breaker
    pub fn new(circuit_breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            items: DashMap::new(),
            circuit_breaker,
        }
    }

    /// Add an item rejected due to open circuit
    ///
    /// Items are grouped by domain for efficient recovery checking.
    pub fn add(&self, item: CrawlQueue) {
        if let Ok(domain) = extract_domain(&item.url) {
            let mut entry = self.items.entry(domain.clone()).or_default();
            entry.push(item);
        }
    }

    /// Drain items ready for retry (circuit is now HalfOpen or Closed)
    ///
    /// This method checks each domain's circuit state and returns all
    /// items for domains that are now allowing requests. The check
    /// via `should_attempt()` also triggers the Open→HalfOpen transition
    /// if the timeout has elapsed.
    pub fn drain_ready(&self) -> Vec<CrawlQueue> {
        let mut ready = Vec::new();
        let mut domains_to_clear = Vec::new();

        // First pass: identify domains ready for retry
        for entry in self.items.iter() {
            let domain = entry.key();
            // should_attempt() returns true if Closed or HalfOpen
            // AND triggers Open→HalfOpen transition if timeout elapsed
            if self.circuit_breaker.should_attempt(domain) {
                domains_to_clear.push(domain.clone());
            }
        }

        // Second pass: drain items from ready domains
        for domain in domains_to_clear {
            if let Some((_, items)) = self.items.remove(&domain) {
                info!(
                    "Circuit breaker RECOVERY: re-queueing {} URLs for domain {}",
                    items.len(),
                    domain
                );
                ready.extend(items);
            }
        }

        ready
    }

    /// Count of items waiting for retry across all domains
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.iter().map(|e| e.value().len()).sum()
    }

    /// Check if retry queue is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get count of items per domain (for debugging/metrics)
    #[must_use]
    pub fn domain_counts(&self) -> Vec<(String, usize)> {
        self.items
            .iter()
            .map(|e| (e.key().clone(), e.value().len()))
            .collect()
    }
}
