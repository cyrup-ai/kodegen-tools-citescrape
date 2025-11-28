/// Integration test to verify config creation matches MCP tool behavior

use kodegen_tools_citescrape::config::CrawlConfig;
use kodegen_tools_citescrape::crawl_engine::should_visit_url;

#[test]
fn test_config_builder_preserves_full_url() {
    // Simulate exactly what the MCP tool does in start_crawl.rs:122-137
    let args_url = "https://stackoverflow.com/questions/12345/how-to-use-rust".to_string();
    
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url(&args_url)  // Line 125 in start_crawl.rs: start_url: args.url.clone()
        .build()
        .unwrap();
    
    println!("\n=== Config Builder Test ===");
    println!("Input URL: {}", args_url);
    println!("Config start_url: {}", config.start_url());
    println!("Match: {}", config.start_url() == args_url);
    
    // Verify the full URL is preserved
    assert_eq!(config.start_url(), args_url, 
        "Config builder should preserve the full URL including path");
    
    // Verify path validation works with this config
    assert!(should_visit_url(&args_url, &config), "Should allow start URL");
    assert!(!should_visit_url("https://stackoverflow.com/questions/67890/other", &config),
        "Should reject different article");
}

#[test]
fn test_default_config_start_url() {
    // Check what happens with default config
    let config = CrawlConfig::default();
    
    println!("\n=== Default Config Test ===");
    println!("Default start_url: '{}'", config.start_url());
    println!("Is empty: {}", config.start_url().is_empty());
    
    assert_eq!(config.start_url(), "", "Default config should have empty start_url");
}

#[test]
fn test_config_with_builder() {
    // Use builder pattern like real code does
    
    let start_url = "https://stackoverflow.com/questions/12345/article".to_string();
    
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url(&start_url)
        .save_markdown(true)
        .max_depth(3)
        .build()
        .unwrap();
    
    println!("\n=== Config with Builder Test ===");
    println!("Created start_url: {}", config.start_url());
    println!("Original: {}", start_url);
    println!("Match: {}", config.start_url() == start_url);
    
    assert_eq!(config.start_url(), start_url, 
        "Config builder should preserve start_url");
    
    // Verify this config works correctly with path validation
    assert!(should_visit_url("https://stackoverflow.com/questions/12345/article", &config));
    assert!(!should_visit_url("https://stackoverflow.com/questions/67890/other", &config));
}

#[test]
fn test_actual_mcp_tool_simulation() {
    // Complete simulation of scrape_url MCP tool flow
    
    // Step 1: Simulate MCP args
    let user_provided_url = "https://stackoverflow.com/questions/12345/how-to-use-rust";
    
    // Step 2: Build config using builder like real MCP tool
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test-stackoverflow")
        .start_url(user_provided_url)
        .save_markdown(true)
        .max_depth(3)
        .build()
        .unwrap();
    
    println!("\n=== Full MCP Tool Simulation ===");
    println!("User URL: {}", user_provided_url);
    println!("Config start_url: {}", config.start_url());
    println!("Config max_depth: {}", config.max_depth());
    println!("Config allow_subdomains: {}", config.allow_subdomains());
    println!("Config allow_external_domains: {}", config.allow_external_domains());
    
    // Step 3: Test link filtering (simulates extract_valid_urls flow)
    let test_links = vec![
        ("https://stackoverflow.com/questions/12345/how-to-use-rust", true, "exact match"),
        ("https://stackoverflow.com/questions/12345/how-to-use-rust#answer", true, "same page with anchor"),
        ("https://stackoverflow.com/questions/67890/python-help", false, "DIFFERENT ARTICLE"),
        ("https://stackoverflow.com/users/123/john", false, "different section"),
        ("https://stackoverflow.com/", false, "root"),
    ];
    
    println!("\nLink filtering results:");
    for (url, expected, description) in &test_links {
        let allowed = should_visit_url(url, &config);
        let status = if allowed { "ALLOW" } else { "REJECT" };
        let correct = allowed == *expected;
        
        println!("  {} - {} ({}) {}", 
            status, description, url,
            if correct { "✓" } else { "✗ WRONG!" });
        
        assert_eq!(allowed, *expected, 
            "Link filtering failed for {} ({})", url, description);
    }
    
    println!("\n✅ All link filtering works correctly!");
}

#[test]
fn test_edge_case_url_without_trailing_slash() {
    // What if user provides URL without trailing slash?
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs")  // No trailing slash
        .build()
        .unwrap();
    
    println!("\n=== URL Without Trailing Slash Test ===");
    println!("Start URL: {}", config.start_url());
    
    assert!(should_visit_url("https://example.com/docs", &config));
    assert!(should_visit_url("https://example.com/docs/", &config));
    assert!(should_visit_url("https://example.com/docs/api", &config));
    assert!(!should_visit_url("https://example.com/doc", &config));
}

#[test]
fn test_edge_case_url_with_trailing_slash() {
    // What if user provides URL with trailing slash?
    let config = CrawlConfig::builder()
        .storage_dir("/tmp/test")
        .start_url("https://example.com/docs/")  // WITH trailing slash
        .build()
        .unwrap();
    
    println!("\n=== URL With Trailing Slash Test ===");
    println!("Start URL: {}", config.start_url());
    
    assert!(should_visit_url("https://example.com/docs", &config));
    assert!(should_visit_url("https://example.com/docs/", &config));
    assert!(should_visit_url("https://example.com/docs/api", &config));
    assert!(!should_visit_url("https://example.com/doc", &config));
}
