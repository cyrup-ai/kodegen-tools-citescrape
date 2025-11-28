use kodegen_tools_citescrape::config::CrawlConfig;
use kodegen_tools_citescrape::crawl_engine::should_visit_url;

#[test]
fn test_same_host_different_path_rejected() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://stackoverflow.com/questions/12345/article")
        .build()
        .unwrap();

    // Same host, different article path - should be REJECTED
    assert!(!should_visit_url(
        "https://stackoverflow.com/questions/67890/other",
        &config
    ));

    // Same host, different section - should be REJECTED
    assert!(!should_visit_url(
        "https://stackoverflow.com/users/profile",
        &config
    ));

    // Same host, root path - should be REJECTED
    assert!(!should_visit_url("https://stackoverflow.com/", &config));
}

#[test]
fn test_same_path_variations_allowed() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://stackoverflow.com/questions/12345/article")
        .build()
        .unwrap();

    // Exact match - ALLOWED
    assert!(should_visit_url(
        "https://stackoverflow.com/questions/12345/article",
        &config
    ));

    // Exact match with trailing slash - ALLOWED
    assert!(should_visit_url(
        "https://stackoverflow.com/questions/12345/article/",
        &config
    ));

    // Child path - ALLOWED
    assert!(should_visit_url(
        "https://stackoverflow.com/questions/12345/article/related",
        &config
    ));

    // With query params - ALLOWED (query ignored)
    assert!(should_visit_url(
        "https://stackoverflow.com/questions/12345/article?page=2",
        &config
    ));

    // With fragment - ALLOWED (fragment ignored)
    assert!(should_visit_url(
        "https://stackoverflow.com/questions/12345/article#answer-123",
        &config
    ));
}

#[test]
fn test_different_host_rejected() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://stackoverflow.com/questions/12345/article")
        .build()
        .unwrap();

    // Different domain - REJECTED
    assert!(!should_visit_url("https://github.com/repo/issue", &config));

    // Subdomain - REJECTED (allow_subdomains is false)
    assert!(!should_visit_url(
        "https://meta.stackoverflow.com/questions/123",
        &config
    ));
}

#[test]
fn test_root_path_allows_all() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/")
        .build()
        .unwrap();

    // Root allows any path on same domain
    assert!(should_visit_url("https://example.com/any/path", &config));
    assert!(should_visit_url("https://example.com/other", &config));

    // But still rejects different host
    assert!(!should_visit_url("https://other.com/", &config));
}

#[test]
fn test_trailing_slash_normalization() {
    // Start URL with trailing slash
    let config1 = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs/")
        .build()
        .unwrap();

    // Should allow URLs with or without trailing slash
    assert!(should_visit_url("https://example.com/docs", &config1));
    assert!(should_visit_url("https://example.com/docs/", &config1));
    assert!(should_visit_url(
        "https://example.com/docs/guide",
        &config1
    ));

    // Start URL without trailing slash
    let config2 = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs")
        .build()
        .unwrap();

    // Should also allow URLs with or without trailing slash
    assert!(should_visit_url("https://example.com/docs", &config2));
    assert!(should_visit_url("https://example.com/docs/", &config2));
    assert!(should_visit_url(
        "https://example.com/docs/guide",
        &config2
    ));
}

#[test]
fn test_query_and_fragment_ignored() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs/api")
        .build()
        .unwrap();

    // Query params should be ignored in path matching
    assert!(should_visit_url(
        "https://example.com/docs/api?search=foo",
        &config
    ));
    assert!(should_visit_url(
        "https://example.com/docs/api?page=1&size=20",
        &config
    ));

    // Fragments should be ignored
    assert!(should_visit_url(
        "https://example.com/docs/api#section",
        &config
    ));

    // Both query and fragment
    assert!(should_visit_url(
        "https://example.com/docs/api?search=foo#section",
        &config
    ));

    // But path must still be under start path
    assert!(!should_visit_url(
        "https://example.com/other?path=/docs/api",
        &config
    ));
}

#[test]
fn test_scheme_must_match() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs")
        .build()
        .unwrap();

    // HTTPS start URL rejects HTTP
    assert!(!should_visit_url("http://example.com/docs", &config));

    // Different scheme entirely
    assert!(!should_visit_url("ftp://example.com/docs", &config));
}

#[test]
fn test_partial_path_prefix_not_allowed() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs")
        .build()
        .unwrap();

    // "/documentation" is NOT under "/docs" (different path)
    assert!(!should_visit_url(
        "https://example.com/documentation",
        &config
    ));

    // "/doc" is NOT under "/docs" (shorter path)
    assert!(!should_visit_url("https://example.com/doc", &config));

    // But "/docs/other" IS under "/docs"
    assert!(should_visit_url(
        "https://example.com/docs/other",
        &config
    ));
}

#[test]
fn test_invalid_url_rejected() {
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs")
        .build()
        .unwrap();

    // Invalid URLs should be rejected
    assert!(!should_visit_url("not a url", &config));
    assert!(!should_visit_url("", &config));
    assert!(!should_visit_url("://invalid", &config));
}
