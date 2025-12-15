//! Memory-bounded crawl rate limiter for respectful web crawling
//!
//! This module provides a fast rate limiter using a token bucket algorithm
//! with per-domain tracking and LRU-based memory management. The implementation
//! bounds memory usage while maintaining lock-free token bucket operations.
//!
//! Key features:
//! - Async-friendly with `tokio::sync` primitives
//! - Thread-safe LRU cache with bounded capacity (max 1000 domains)
//! - Lock-free per-domain token bucket using atomic operations
//! - Automatic eviction of least-recently-used domains
//! - Safe for use with tokio multi-threaded runtime and task migration
//! - Per-domain rate limiting with independent token buckets
//! - Immediate Pass/Deny decisions with no blocking or sleep
//! - Fixed-point arithmetic for sub-token precision
//! - Instance-based API for test isolation

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Scaling factor for fixed-point token arithmetic (1000x precision)
const TOKEN_SCALE: u64 = 1000;

/// Scaling factor for nanosecond rate calculations
const RATE_SCALE: u64 = 1_000_000;

/// Maximum number of domains to track simultaneously
const MAX_DOMAIN_LIMITERS: usize = 1000;

/// Rate limit decision for a crawl request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    /// Request is allowed to proceed
    Allow,
    /// Request should be denied/deferred due to rate limiting
    /// Contains the duration to wait before retrying
    Deny { retry_after: Duration },
}

/// Per-domain rate limiter using atomic token bucket algorithm
#[derive(Debug)]
struct DomainRateLimiter {
    /// Current available tokens scaled by `TOKEN_SCALE` for sub-token precision
    tokens: AtomicU64,
    /// Last token refill timestamp as nanoseconds since base time
    last_refill_nanos: AtomicU64,
    /// Rate in tokens per nanosecond scaled by `TOKEN_SCALE` * `RATE_SCALE`
    rate_per_nano: u64,
    /// Maximum tokens scaled by `TOKEN_SCALE`
    max_tokens: u64,
}

impl DomainRateLimiter {
    /// Create a new domain rate limiter with the specified rate
    #[inline]
    fn new(rate_rps: f64, base_time: &Instant) -> Self {
        let max_tokens = (rate_rps.max(1.0) * TOKEN_SCALE as f64) as u64;
        let rate_per_nano =
            ((rate_rps * TOKEN_SCALE as f64 * RATE_SCALE as f64) / 1_000_000_000.0) as u64;

        let now_nanos = base_time.elapsed().as_nanos() as u64;

        Self {
            tokens: AtomicU64::new(max_tokens),
            last_refill_nanos: AtomicU64::new(now_nanos),
            rate_per_nano,
            max_tokens,
        }
    }

    /// Attempt to consume one token from the bucket
    #[inline]
    fn try_consume_token(&self, base_time: &Instant) -> RateLimitDecision {
        let now_nanos = base_time.elapsed().as_nanos() as u64;

        // Refill tokens based on elapsed time
        self.refill_tokens(now_nanos);

        // Try to consume one token atomically
        loop {
            let current_tokens = self.tokens.load(Ordering::Relaxed);
            if current_tokens < TOKEN_SCALE {
                // Not enough tokens available - calculate wait time
                let tokens_needed = TOKEN_SCALE.saturating_sub(current_tokens);

                // Calculate nanoseconds needed to accumulate required tokens
                let nanos_needed = if self.rate_per_nano > 0 {
                    (tokens_needed.saturating_mul(RATE_SCALE)) / self.rate_per_nano
                } else {
                    1_000_000 // 1ms
                };

                let retry_after = Duration::from_nanos(nanos_needed);
                return RateLimitDecision::Deny { retry_after };
            }

            let new_tokens = current_tokens - TOKEN_SCALE;
            match self.tokens.compare_exchange_weak(
                current_tokens,
                new_tokens,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return RateLimitDecision::Allow,
                Err(_) => continue, // Retry on contention
            }
        }
    }

    /// Refill tokens based on elapsed time since last refill
    ///
    /// **Critical Fix**: Preserves fractional nanoseconds by only advancing
    /// `last_refill_nanos` by the time that actually produced tokens. This prevents
    /// the integer division bug where concurrent threads would starve when
    /// elapsed time is too small to produce tokens.
    #[inline]
    fn refill_tokens(&self, now_nanos: u64) {
        loop {
            let last_refill = self.last_refill_nanos.load(Ordering::Relaxed);

            if now_nanos <= last_refill {
                break;
            }

            let elapsed_nanos = now_nanos.saturating_sub(last_refill);
            let tokens_to_add = (elapsed_nanos.saturating_mul(self.rate_per_nano)) / RATE_SCALE;

            // Calculate exact time that produced these tokens (inverse operation)
            // This prevents discarding fractional nanoseconds when tokens_to_add = 0
            let time_credited_nanos = if self.rate_per_nano > 0 {
                (tokens_to_add.saturating_mul(RATE_SCALE)) / self.rate_per_nano
            } else {
                0
            };

            // Only advance last_refill by time that produced tokens
            // This preserves fractional nanoseconds for future accumulation
            let new_last_refill = last_refill.saturating_add(time_credited_nanos);

            match self.last_refill_nanos.compare_exchange_weak(
                last_refill,
                new_last_refill,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Only add tokens if we actually produced any
                    if tokens_to_add > 0 {
                        loop {
                            let current_tokens = self.tokens.load(Ordering::Relaxed);
                            let new_tokens = current_tokens
                                .saturating_add(tokens_to_add)
                                .min(self.max_tokens);

                            if current_tokens == new_tokens {
                                break;
                            }

                            match self.tokens.compare_exchange_weak(
                                current_tokens,
                                new_tokens,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => break,
                                Err(_) => continue,
                            }
                        }
                    }
                    break;
                }
                Err(_) => continue,
            }
        }
    }
}

/// Instance-based crawl rate limiter with isolated state
///
/// Each instance maintains its own domain cache and time reference,
/// enabling test isolation when running tests in parallel.
///
/// # Example
///
/// ```rust
/// use kodegen_tools_citescrape::crawl_rate_limiter::{CrawlRateLimiter, RateLimitDecision};
///
/// #[tokio::main]
/// async fn main() {
///     let limiter = CrawlRateLimiter::new();
///     
///     // First request is allowed
///     assert_eq!(
///         limiter.check("https://example.com", 1.0).await,
///         RateLimitDecision::Allow
///     );
///     
///     // Immediate second request is denied
///     assert!(matches!(
///         limiter.check("https://example.com", 1.0).await,
///         RateLimitDecision::Deny { .. }
///     ));
/// }
/// ```
pub struct CrawlRateLimiter {
    /// Per-domain rate limiter cache
    cache: Mutex<LruCache<String, Arc<DomainRateLimiter>>>,
    /// Base time for all time calculations in this instance
    base_time: Instant,
}

impl Default for CrawlRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl CrawlRateLimiter {
    /// Create a new rate limiter instance with isolated state
    ///
    /// Each instance has its own domain cache and time reference,
    /// enabling test isolation when running tests in parallel.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(MAX_DOMAIN_LIMITERS).expect("MAX_DOMAIN_LIMITERS is non-zero"),
            )),
            base_time: Instant::now(),
        }
    }

    /// Check if a crawl request to the given URL should be rate limited
    ///
    /// # Arguments
    ///
    /// * `url` - The URL being requested
    /// * `rate_rps` - Maximum requests per second allowed for this domain
    ///
    /// # Returns
    ///
    /// * `RateLimitDecision::Allow` - Request can proceed
    /// * `RateLimitDecision::Deny { retry_after }` - Request should wait
    pub async fn check(&self, url: &str, rate_rps: f64) -> RateLimitDecision {
        if rate_rps <= 0.0 {
            return RateLimitDecision::Allow;
        }

        let domain = match extract_domain(url) {
            Some(domain) if !domain.is_empty() => domain,
            _ => return RateLimitDecision::Allow,
        };

        self.check_domain(&domain, rate_rps).await
    }

    /// Check rate limit for a specific domain
    async fn check_domain(&self, domain: &str, rate_rps: f64) -> RateLimitDecision {
        let mut cache = self.cache.lock().await;

        if let Some(limiter) = cache.get(domain) {
            let limiter = Arc::clone(limiter);
            drop(cache);
            return limiter.try_consume_token(&self.base_time);
        }

        let limiter = Arc::new(DomainRateLimiter::new(rate_rps, &self.base_time));
        cache.put(domain.to_string(), Arc::clone(&limiter));
        drop(cache);
        limiter.try_consume_token(&self.base_time)
    }

    /// Clear all domain rate limiters in this instance
    pub async fn clear(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
    }

    /// Get the number of domains currently being tracked
    pub async fn tracked_count(&self) -> usize {
        let cache = self.cache.lock().await;
        cache.len()
    }
}

// =============================================================================
// Global API (for production use - wraps a global CrawlRateLimiter instance)
// =============================================================================

/// Global rate limiter instance for production use
static GLOBAL_LIMITER: OnceLock<CrawlRateLimiter> = OnceLock::new();

/// Get or initialize the global rate limiter
#[inline]
fn get_global_limiter() -> &'static CrawlRateLimiter {
    GLOBAL_LIMITER.get_or_init(CrawlRateLimiter::new)
}

/// Extract domain from URL
#[inline]
#[must_use]
pub fn extract_domain(url: &str) -> Option<String> {
    if let Some(scheme_end) = url.find("://") {
        let after_scheme = &url[scheme_end + 3..];
        let domain_end = after_scheme
            .find(['/', '?', '#', ':'])
            .unwrap_or(after_scheme.len());
        let domain = &after_scheme[..domain_end];
        let normalized = if domain.starts_with("www.") && domain.len() > 4 {
            &domain[4..]
        } else {
            domain
        };
        Some(normalized.to_lowercase())
    } else {
        let domain = url.split(['/', '?', '#', ':']).next().unwrap_or(url);
        let normalized = if domain.starts_with("www.") && domain.len() > 4 {
            &domain[4..]
        } else {
            domain
        };
        Some(normalized.to_lowercase())
    }
}

/// Check if a crawl request to the given URL should be rate limited (global instance)
///
/// This is a convenience function that uses the global rate limiter instance.
/// For test isolation, use `CrawlRateLimiter::new()` to create isolated instances.
#[inline]
pub async fn check_crawl_rate_limit(url: &str, rate_rps: f64) -> RateLimitDecision {
    get_global_limiter().check(url, rate_rps).await
}

/// Check if an HTTP request should be rate limited (global instance)
///
/// Alias for `check_crawl_rate_limit`.
#[inline]
pub async fn check_http_rate_limit(url: &str, rate_rps: f64) -> RateLimitDecision {
    check_crawl_rate_limit(url, rate_rps).await
}

/// Clear all domain rate limiters (global instance)
///
/// This clears the global rate limiter's domain cache.
/// For test isolation, use `CrawlRateLimiter::new()` to create isolated instances.
pub async fn clear_domain_limiters() {
    get_global_limiter().clear().await
}

/// Get the number of domains currently being tracked for rate limiting (global instance)
pub async fn get_tracked_domain_count() -> usize {
    get_global_limiter().tracked_count().await
}
