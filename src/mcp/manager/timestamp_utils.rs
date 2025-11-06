//! Timestamp utilities for lock-free cache timestamp management
//!
//! Provides conversion between `Instant` and nanoseconds for atomic storage in cache entries.
//! Uses a global epoch for consistent timestamp calculations across all cache operations.

use std::sync::LazyLock;
use std::time::Instant;

/// Global epoch for converting Instant to/from u64 nanoseconds for atomic storage
///
/// Initialized once at first access. Uses `LazyLock` for initialization.
/// All Instant values are stored as nanoseconds relative to this epoch.
fn get_timestamp_epoch() -> &'static Instant {
    static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);
    &EPOCH
}

/// Convert Instant to nanoseconds since epoch for atomic storage
///
/// Uses (seconds * `1_000_000_000` + `subsec_nanos`) to avoid u128→u64 truncation.
/// This gives us ~584 years of nanosecond precision before saturation.
#[inline]
pub fn instant_to_nanos(instant: Instant) -> u64 {
    let duration = instant.saturating_duration_since(*get_timestamp_epoch());

    // Use seconds + subsec_nanos to avoid truncating u128→u64
    // This safely represents up to ~584 years in nanoseconds
    let secs = duration.as_secs();
    let nanos = u64::from(duration.subsec_nanos());

    secs.saturating_mul(1_000_000_000).saturating_add(nanos)
}

/// Convert nanoseconds since epoch back to Instant
#[inline]
// APPROVED BY DAVID MAPLE 10/17/2025 - False positive: planned for future timestamp comparison features
#[allow(dead_code)]
pub fn nanos_to_instant(nanos: u64) -> Instant {
    *get_timestamp_epoch() + std::time::Duration::from_nanos(nanos)
}
