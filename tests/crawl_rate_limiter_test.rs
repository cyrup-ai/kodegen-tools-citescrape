//! Tests for the crawl rate limiter
//!
//! These tests use isolated `CrawlRateLimiter` instances to ensure
//! they can run in parallel without interfering with each other.

use kodegen_tools_citescrape::crawl_rate_limiter::*;

#[test]
fn test_extract_domain() {
    assert_eq!(
        extract_domain("https://example.com"),
        Some("example.com".to_string())
    );
    assert_eq!(
        extract_domain("https://www.example.com"),
        Some("example.com".to_string())
    );
    assert_eq!(
        extract_domain("https://example.com/path"),
        Some("example.com".to_string())
    );
    assert_eq!(
        extract_domain("https://example.com:8080"),
        Some("example.com".to_string())
    );
    assert_eq!(
        extract_domain("https://sub.example.com"),
        Some("sub.example.com".to_string())
    );
    assert_eq!(
        extract_domain("example.com"),
        Some("example.com".to_string())
    );
    assert_eq!(
        extract_domain("www.example.com"),
        Some("example.com".to_string())
    );
}

#[tokio::test]
async fn test_rate_limit_basic() {
    let limiter = CrawlRateLimiter::new();

    // First request should be allowed
    assert_eq!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Allow
    );

    // Immediate second request should be denied
    assert!(matches!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Deny { .. }
    ));
}

#[tokio::test]
async fn test_per_domain_limiting() {
    let limiter = CrawlRateLimiter::new();

    // Requests to different domains should be independent
    assert_eq!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Allow
    );
    assert_eq!(
        limiter.check("https://different.com", 1.0).await,
        RateLimitDecision::Allow
    );

    // Second requests should both be denied
    assert!(matches!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Deny { .. }
    ));
    assert!(matches!(
        limiter.check("https://different.com", 1.0).await,
        RateLimitDecision::Deny { .. }
    ));
}

#[tokio::test]
async fn test_invalid_rates() {
    let limiter = CrawlRateLimiter::new();

    // Zero or negative rates should allow all requests
    assert_eq!(
        limiter.check("https://example.com", 0.0).await,
        RateLimitDecision::Allow
    );
    assert_eq!(
        limiter.check("https://example.com", -1.0).await,
        RateLimitDecision::Allow
    );
}

#[tokio::test]
async fn test_invalid_urls() {
    let limiter = CrawlRateLimiter::new();

    // Invalid URLs should be allowed
    assert_eq!(limiter.check("", 1.0).await, RateLimitDecision::Allow);
    assert_eq!(
        limiter.check("not-a-url", 1.0).await,
        RateLimitDecision::Allow
    );
}

#[tokio::test]
async fn test_concurrent_tasks() {
    use std::sync::Arc;

    // Create a shared limiter for concurrent access testing
    let limiter = Arc::new(CrawlRateLimiter::new());

    // Use domain in this task
    assert_eq!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Allow
    );
    assert!(matches!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Deny { .. }
    ));

    // Spawn concurrent task - they share the same rate limiter state
    let limiter_clone = Arc::clone(&limiter);
    let handle = tokio::spawn(async move {
        // Should be rate limited since we already consumed the token
        assert!(matches!(
            limiter_clone.check("https://example.com", 1.0).await,
            RateLimitDecision::Deny { .. }
        ));
    });

    handle.await.unwrap();
}

#[tokio::test]
async fn test_high_rate_limits() {
    let limiter = CrawlRateLimiter::new();

    // High rate limits should allow multiple requests
    let high_rate = 100.0; // 100 RPS

    let mut allowed_count = 0;
    for _ in 0..10 {
        if limiter.check("https://example.com", high_rate).await == RateLimitDecision::Allow {
            allowed_count += 1;
        }
    }

    // Should allow multiple requests with high rate
    assert!(
        allowed_count > 1,
        "Expected more than 1 allowed request with high rate, got {}",
        allowed_count
    );
}

#[tokio::test]
async fn test_domain_normalization() {
    let limiter = CrawlRateLimiter::new();

    // These should all be treated as the same domain
    assert_eq!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Allow
    );
    assert!(matches!(
        limiter.check("https://www.example.com", 1.0).await,
        RateLimitDecision::Deny { .. }
    ));
    assert!(matches!(
        limiter.check("https://EXAMPLE.COM", 1.0).await,
        RateLimitDecision::Deny { .. }
    ));
}

#[tokio::test]
async fn test_clear_limiters() {
    let limiter = CrawlRateLimiter::new();

    // Use up token
    assert_eq!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Allow
    );
    assert!(matches!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Deny { .. }
    ));

    // Clear limiters
    limiter.clear().await;

    // Should be allowed again after clearing
    assert_eq!(
        limiter.check("https://example.com", 1.0).await,
        RateLimitDecision::Allow
    );
}

#[tokio::test]
async fn test_tracked_domain_count() {
    let limiter = CrawlRateLimiter::new();
    assert_eq!(limiter.tracked_count().await, 0);

    // Add some domains
    limiter.check("https://example.com", 1.0).await;
    assert_eq!(limiter.tracked_count().await, 1);

    limiter.check("https://different.com", 1.0).await;
    assert_eq!(limiter.tracked_count().await, 2);

    // Same domain shouldn't increase count
    limiter.check("https://example.com/path", 1.0).await;
    assert_eq!(limiter.tracked_count().await, 2);
}
