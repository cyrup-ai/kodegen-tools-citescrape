//! Per-domain concurrency limiter
//!
//! This module provides domain-level concurrency limiting to prevent
//! rate limiting and bot detection when crawling websites.

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// Maximum number of domains to track simultaneously
/// 
/// Matches the bound used in `rate_limiter.rs` for architectural consistency.
/// Each entry is ~100 bytes, so 1000 domains ≈ 100KB of memory.
const MAX_DOMAIN_SEMAPHORES: usize = 1000;

/// Per-domain concurrency limiter using LRU-bounded cache
///
/// Each domain gets its own semaphore to limit concurrent requests,
/// preventing rate limiting and reducing bot detection risk.
/// 
/// # Memory Bounds
/// 
/// The cache is bounded to `MAX_DOMAIN_SEMAPHORES` (1000) domains using LRU eviction.
/// When the cache is full and a new domain is accessed, the least-recently-used
/// domain's semaphore is evicted. This prevents unbounded memory growth in
/// multi-domain crawling scenarios.
pub struct DomainLimiter {
    domain_semaphores: Mutex<LruCache<String, Arc<Semaphore>>>,
    max_per_domain: usize,
}

impl DomainLimiter {
    /// Create a new domain limiter with the specified per-domain limit
    ///
    /// # Arguments
    /// * `max_per_domain` - Maximum concurrent requests per domain
    #[must_use]
    pub fn new(max_per_domain: usize) -> Self {
        Self {
            domain_semaphores: Mutex::new(
                LruCache::new(
                    NonZeroUsize::new(MAX_DOMAIN_SEMAPHORES)
                        .expect("MAX_DOMAIN_SEMAPHORES is non-zero")
                )
            ),
            max_per_domain,
        }
    }

    /// Acquire permit for domain (creates semaphore if not exists)
    ///
    /// Returns an owned permit that will be released when dropped.
    /// The semaphore is lazily created on first access for each domain.
    /// 
    /// When the cache reaches `MAX_DOMAIN_SEMAPHORES`, the least-recently-used
    /// domain's semaphore is evicted to make room for the new domain.
    ///
    /// # Arguments
    /// * `domain` - Domain to acquire permit for
    ///
    /// # Concurrency Safety
    /// 
    /// This method locks the cache mutex only to get/insert the semaphore Arc,
    /// then immediately releases the lock before awaiting the semaphore permit.
    /// This prevents holding the mutex across await points, which could cause
    /// deadlocks in concurrent scenarios.
    pub async fn acquire(&self, domain: String) -> OwnedSemaphorePermit {
        // Lock cache, get or insert semaphore, then immediately release lock
        let semaphore = {
            let mut cache = self.domain_semaphores.lock().await;
            
            // LruCache::get updates the LRU order, so we use get_or_insert pattern
            if let Some(sem) = cache.get(&domain) {
                Arc::clone(sem)
            } else {
                let new_semaphore = Arc::new(Semaphore::new(self.max_per_domain));
                cache.put(domain.clone(), Arc::clone(&new_semaphore));
                new_semaphore
            }
            // Mutex lock is dropped here
        };

        // Await semaphore permit WITHOUT holding the cache lock
        // This is critical for preventing deadlocks in concurrent crawling
        semaphore
            .acquire_owned()
            .await
            .expect("Semaphore should never be closed")
    }
}
