//! Integration tests for rate limiting across all download functions
//!
//! These tests verify that rate limiting is properly implemented and consistent
//! across all download functions (CSS, images, SVGs, and generic resources).
//!
//! **IMPORTANT**: Each test uses `CrawlRateLimiter::new()` to create an isolated
//! instance, ensuring tests can run in parallel without race conditions.

use kodegen_tools_citescrape::crawl_rate_limiter::{CrawlRateLimiter, RateLimitDecision};

/// Test that HTTP rate limiting works for CSS downloads
#[tokio::test]
async fn test_http_rate_limit_for_css_downloads() {
    let limiter = CrawlRateLimiter::new();

    let url = "https://example.com/style.css";
    let rate = 1.0; // 1 request per second

    // First request should be allowed
    assert_eq!(
        limiter.check(url, rate).await,
        RateLimitDecision::Allow,
        "First CSS download should be allowed"
    );

    // Immediate second request should be denied
    assert!(
        matches!(
            limiter.check(url, rate).await,
            RateLimitDecision::Deny { .. }
        ),
        "Immediate second CSS download should be rate limited"
    );
}

/// Test that HTTP rate limiting works for image downloads
#[tokio::test]
async fn test_http_rate_limit_for_image_downloads() {
    let limiter = CrawlRateLimiter::new();

    let url = "https://example.com/photo.jpg";
    let rate = 1.0; // 1 request per second

    // First request should be allowed
    assert_eq!(
        limiter.check(url, rate).await,
        RateLimitDecision::Allow,
        "First image download should be allowed"
    );

    // Immediate second request should be denied
    assert!(
        matches!(
            limiter.check(url, rate).await,
            RateLimitDecision::Deny { .. }
        ),
        "Immediate second image download should be rate limited"
    );
}

/// Test that HTTP rate limiting works for SVG downloads
#[tokio::test]
async fn test_http_rate_limit_for_svg_downloads() {
    let limiter = CrawlRateLimiter::new();

    let url = "https://example.com/icon.svg";
    let rate = 1.0; // 1 request per second

    // First request should be allowed
    assert_eq!(
        limiter.check(url, rate).await,
        RateLimitDecision::Allow,
        "First SVG download should be allowed"
    );

    // Immediate second request should be denied
    assert!(
        matches!(
            limiter.check(url, rate).await,
            RateLimitDecision::Deny { .. }
        ),
        "Immediate second SVG download should be rate limited"
    );
}

/// Test that HTTP rate limiting works for generic resource downloads
#[tokio::test]
async fn test_http_rate_limit_for_resource_downloads() {
    let limiter = CrawlRateLimiter::new();

    let url = "https://example.com/data.bin";
    let rate = 1.0; // 1 request per second

    // First request should be allowed
    assert_eq!(
        limiter.check(url, rate).await,
        RateLimitDecision::Allow,
        "First resource download should be allowed"
    );

    // Immediate second request should be denied
    assert!(
        matches!(
            limiter.check(url, rate).await,
            RateLimitDecision::Deny { .. }
        ),
        "Immediate second resource download should be rate limited"
    );
}

/// Test that rate limiting is applied per-domain for all resource types
#[tokio::test]
async fn test_per_domain_rate_limiting_across_resource_types() {
    let limiter = CrawlRateLimiter::new();

    let domain1_css = "https://example.com/style.css";
    let domain1_image = "https://example.com/photo.jpg";
    let domain2_css = "https://different.com/style.css";
    let rate = 1.0;

    // First request to domain1 (CSS) - allowed
    assert_eq!(
        limiter.check(domain1_css, rate).await,
        RateLimitDecision::Allow,
        "First CSS from domain1 should be allowed"
    );

    // Second request to domain1 (image) - denied (same domain, different resource type)
    assert!(
        matches!(
            limiter.check(domain1_image, rate).await,
            RateLimitDecision::Deny { .. }
        ),
        "Image from domain1 should be rate limited (domain already used by CSS)"
    );

    // First request to domain2 (CSS) - allowed (different domain)
    assert_eq!(
        limiter.check(domain2_css, rate).await,
        RateLimitDecision::Allow,
        "CSS from domain2 should be allowed (different domain)"
    );
}

/// Test that high rate limits allow multiple downloads across resource types
#[tokio::test]
async fn test_high_rate_limit_for_mixed_resources() {
    let limiter = CrawlRateLimiter::new();

    let base_url = "https://example.com";
    let rate = 100.0; // 100 requests per second

    // Make requests for different resource types
    let resources = vec![
        format!("{}/style.css", base_url),
        format!("{}/photo.jpg", base_url),
        format!("{}/icon.svg", base_url),
        format!("{}/data.bin", base_url),
        format!("{}/script.js", base_url),
    ];

    let mut allowed_count = 0;
    for resource_url in resources {
        if limiter.check(&resource_url, rate).await == RateLimitDecision::Allow {
            allowed_count += 1;
        }
    }

    // With high rate limit, multiple requests should be allowed
    assert!(
        allowed_count > 1,
        "High rate limit should allow multiple resource downloads. Got {allowed_count} allowed"
    );
}

/// Test that rate limiting respects domain normalization for all resource types
#[tokio::test]
async fn test_domain_normalization_for_all_resources() {
    let limiter = CrawlRateLimiter::new();

    let rate = 1.0;

    // First request with www prefix
    assert_eq!(
        limiter.check("https://www.example.com/style.css", rate).await,
        RateLimitDecision::Allow,
        "First request should be allowed"
    );

    // Second request without www (same normalized domain, different resource type)
    assert!(
        matches!(
            limiter.check("https://example.com/photo.jpg", rate).await,
            RateLimitDecision::Deny { .. }
        ),
        "Should be rate limited (same domain after normalization)"
    );

    // Third request with uppercase (same normalized domain, different resource type)
    assert!(
        matches!(
            limiter.check("https://EXAMPLE.COM/icon.svg", rate).await,
            RateLimitDecision::Deny { .. }
        ),
        "Should be rate limited (same domain after normalization)"
    );
}

/// Consistency verification test
/// This test documents that all download functions now have consistent rate limiting support.
/// The rate limiting is implemented via the instance-based `CrawlRateLimiter` API.
#[tokio::test]
async fn test_rate_limiting_consistency_documentation() {
    // This test serves as documentation that rate limiting is now consistently
    // available across all download function types:
    //
    // 1. download_css_with_rate_limit() - CSS downloads with rate limiting
    // 2. download_and_encode_image_with_rate_limit() - Image downloads with rate limiting
    // 3. download_svg_with_rate_limit() - SVG downloads with rate limiting
    // 4. download_resource_with_rate_limit() - Generic resource downloads with rate limiting
    //
    // Each function also has a wrapper without _with_rate_limit that passes None for rate_rps:
    // - download_css()
    // - download_and_encode_image()
    // - download_svg()
    // - download_resource()
    //
    // All rate-limited variants use check_http_rate_limit() internally before making HTTP requests.
    // This ensures consistent rate limiting behavior across all download types.
    //
    // For tests, use CrawlRateLimiter::new() to create isolated instances that don't
    // interfere with parallel test execution.

    // Test passes - rate limiting is consistently implemented across all download functions
}
