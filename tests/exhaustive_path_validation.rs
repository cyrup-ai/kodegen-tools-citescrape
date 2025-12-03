//! Exhaustive path validation testing to verify crawl scope enforcement
//! 
//! This test systematically verifies that path-based scope enforcement works
//! correctly for real-world URLs like stackoverflow.com
use kodegen_tools_citescrape::config::CrawlConfig;
use kodegen_tools_citescrape::crawl_engine::should_visit_url;

#[test]
fn test_stackoverflow_article_scope() {
    // Scenario: User wants to crawl a single stackoverflow article
    // Start URL: https://stackoverflow.com/questions/12345/how-to-use-rust
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://stackoverflow.com/questions/12345/how-to-use-rust")
        .build()
        .unwrap();

    println!("Testing path scope for: {}", config.start_url());

    // SHOULD ALLOW: Exact match
    assert!(
        should_visit_url("https://stackoverflow.com/questions/12345/how-to-use-rust", &config),
        "Should allow exact URL match"
    );

    // SHOULD ALLOW: Same URL with trailing slash
    assert!(
        should_visit_url("https://stackoverflow.com/questions/12345/how-to-use-rust/", &config),
        "Should allow exact URL with trailing slash"
    );

    // SHOULD ALLOW: Child paths (e.g., answer anchors)
    assert!(
        should_visit_url("https://stackoverflow.com/questions/12345/how-to-use-rust/answer-123", &config),
        "Should allow child paths under the article"
    );

    // SHOULD ALLOW: Query parameters on same path
    assert!(
        should_visit_url("https://stackoverflow.com/questions/12345/how-to-use-rust?page=2", &config),
        "Should allow query parameters (ignored by path comparison)"
    );

    // SHOULD ALLOW: Fragments on same path
    assert!(
        should_visit_url("https://stackoverflow.com/questions/12345/how-to-use-rust#answer-456", &config),
        "Should allow fragments (ignored by path comparison)"
    );

    // SHOULD REJECT: Different article (sibling path)
    assert!(
        !should_visit_url("https://stackoverflow.com/questions/67890/other-question", &config),
        "CRITICAL: Should REJECT different article - THIS IS THE MAIN BUG"
    );

    // SHOULD REJECT: Questions root
    assert!(
        !should_visit_url("https://stackoverflow.com/questions", &config),
        "Should REJECT parent path (questions root)"
    );

    // SHOULD REJECT: Different sections
    assert!(
        !should_visit_url("https://stackoverflow.com/users/123/john", &config),
        "Should REJECT different section (/users/)"
    );

    assert!(
        !should_visit_url("https://stackoverflow.com/tags/rust", &config),
        "Should REJECT different section (/tags/)"
    );

    // SHOULD REJECT: Root domain
    assert!(
        !should_visit_url("https://stackoverflow.com/", &config),
        "Should REJECT domain root"
    );

    // SHOULD REJECT: Different domain
    assert!(
        !should_visit_url("https://github.com/repo", &config),
        "Should REJECT different domain"
    );
}

#[test]
fn test_path_with_trailing_slash_in_start_url() {
    // Test with trailing slash in start URL
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs/api/")
        .build()
        .unwrap();

    assert!(should_visit_url("https://example.com/docs/api", &config));
    assert!(should_visit_url("https://example.com/docs/api/", &config));
    assert!(should_visit_url("https://example.com/docs/api/v1", &config));
    assert!(!should_visit_url("https://example.com/docs", &config));
    assert!(!should_visit_url("https://example.com/docs/guide", &config));
}

#[test]
fn test_root_path_allows_all() {
    // When start URL is domain root, should allow all paths on that domain
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/")
        .build()
        .unwrap();

    assert!(should_visit_url("https://example.com/", &config));
    assert!(should_visit_url("https://example.com/any/path", &config));
    assert!(should_visit_url("https://example.com/docs/api", &config));
    assert!(!should_visit_url("https://other.com/", &config));
}

#[test]
fn test_empty_path_normalization() {
    // Test URL with just domain (no path)
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com")
        .build()
        .unwrap();

    // Should behave same as "https://example.com/"
    assert!(should_visit_url("https://example.com/", &config));
    assert!(should_visit_url("https://example.com", &config));
    assert!(should_visit_url("https://example.com/any/path", &config));
}

#[test]
fn test_query_and_fragment_in_start_url() {
    // What happens if start URL has query params or fragments?
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs/api?version=v1#section")
        .build()
        .unwrap();

    // The path should be extracted as "/docs/api" (query and fragment ignored)
    assert!(should_visit_url("https://example.com/docs/api", &config));
    assert!(should_visit_url("https://example.com/docs/api?version=v2", &config));
    assert!(should_visit_url("https://example.com/docs/api#other", &config));
    assert!(!should_visit_url("https://example.com/docs", &config));
}

#[test]
fn test_partial_path_overlap_rejection() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/api")
        .build()
        .unwrap();

    // These should be REJECTED (not proper children)
    assert!(!should_visit_url("https://example.com/api-v2", &config));
    assert!(!should_visit_url("https://example.com/apis", &config));
    assert!(!should_visit_url("https://example.com/api_old", &config));

    // These should be ALLOWED (proper children)
    assert!(should_visit_url("https://example.com/api/", &config));
    assert!(should_visit_url("https://example.com/api/v1", &config));
}

#[test]
fn test_scheme_mismatch() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs")
        .build()
        .unwrap();

    // HTTP vs HTTPS should be rejected
    assert!(!should_visit_url("http://example.com/docs", &config));
    assert!(!should_visit_url("ftp://example.com/docs", &config));
}

#[test]
fn test_case_sensitivity() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/Docs/API")
        .build()
        .unwrap();

    // URLs are case-sensitive for path (but not host)
    assert!(should_visit_url("https://example.com/Docs/API", &config));
    assert!(should_visit_url("https://EXAMPLE.com/Docs/API", &config)); // Host is case-insensitive
    
    // Path IS case sensitive in URLs
    // This behavior depends on the server, but URL spec says paths are case-sensitive
    // Let's verify what the current implementation does
    let different_case_allowed = should_visit_url("https://example.com/docs/api", &config);
    println!("Case-insensitive path matching: {}", different_case_allowed);
}

#[test]
fn test_deep_nested_paths() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/a/b/c/d/e")
        .build()
        .unwrap();

    assert!(should_visit_url("https://example.com/a/b/c/d/e", &config));
    assert!(should_visit_url("https://example.com/a/b/c/d/e/f", &config));
    assert!(should_visit_url("https://example.com/a/b/c/d/e/f/g/h", &config));
    
    assert!(!should_visit_url("https://example.com/a/b/c/d", &config));
    assert!(!should_visit_url("https://example.com/a/b/c", &config));
    assert!(!should_visit_url("https://example.com/a/b/c/d/e2", &config));
}
