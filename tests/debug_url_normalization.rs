/// Debug test to understand ImUrl behavior and identify potential normalization issues

use kodegen_tools_citescrape::imurl::ImUrl;

#[test]
fn test_imurl_path_extraction() {
    // Test various URL formats to see how ImUrl extracts paths
    
    let test_cases = vec![
        ("https://stackoverflow.com/questions/12345/article", "/questions/12345/article"),
        ("https://stackoverflow.com/questions/12345/article/", "/questions/12345/article/"),
        ("https://stackoverflow.com/questions/12345/article?page=2", "/questions/12345/article"),
        ("https://stackoverflow.com/questions/12345/article#answer", "/questions/12345/article"),
        ("https://stackoverflow.com/", "/"),
        ("https://stackoverflow.com", "/"),
        ("https://example.com/docs/api", "/docs/api"),
        ("https://example.com/docs/api/", "/docs/api/"),
    ];
    
    println!("\n=== ImUrl Path Extraction Test ===");
    for (url, expected_path) in test_cases {
        let parsed = ImUrl::parse(url).unwrap();
        let actual_path = parsed.path();
        let query = parsed.query();
        let fragment = parsed.fragment();
        
        println!("\nURL: {}", url);
        println!("  Path: {} (expected: {})", actual_path, expected_path);
        println!("  Query: {:?}", query);
        println!("  Fragment: {:?}", fragment);
        println!("  Match: {}", actual_path == expected_path);
        
        assert_eq!(actual_path, expected_path, "Path extraction mismatch for {}", url);
    }
}

#[test]
fn test_path_normalization_logic() {
    // Test the exact normalization logic used in should_visit_url()
    
    let test_cases = vec![
        ("/questions/12345/article", "/questions/12345/article"),
        ("/questions/12345/article/", "/questions/12345/article"),
        ("/", ""),  // Root path after trim
        ("", ""),
    ];
    
    println!("\n=== Path Normalization Test ===");
    for (path, expected_normalized) in test_cases {
        let normalized = path.trim_end_matches('/');
        
        println!("\nPath: '{}'", path);
        println!("  Normalized: '{}' (expected: '{}')", normalized, expected_normalized);
        println!("  Is empty: {}", normalized.is_empty());
        println!("  Is '/': {}", normalized == "/");
        println!("  Match: {}", normalized == expected_normalized);
        
        assert_eq!(normalized, expected_normalized, "Normalization mismatch for '{}'", path);
    }
}

#[test]
fn test_path_comparison_logic() {
    // Test the exact comparison logic used in should_visit_url()
    
    let start_path = "/questions/12345/article";
    
    let test_urls = vec![
        ("/questions/12345/article", true, "exact match"),
        ("/questions/12345/article/", true, "trailing slash"),
        ("/questions/12345/article/answer", true, "child path"),
        ("/questions/67890/other", false, "different article"),
        ("/questions/12345", false, "parent path"),
        ("/questions", false, "grandparent path"),
        ("/users/123", false, "different section"),
        ("/", false, "root"),
    ];
    
    println!("\n=== Path Comparison Logic Test ===");
    println!("Start path: '{}'", start_path);
    
    let norm_start_path = start_path.trim_end_matches('/');
    println!("Normalized start path: '{}'", norm_start_path);
    
    for (url_path, should_allow, description) in test_urls {
        let norm_url_path = url_path.trim_end_matches('/');
        
        // Exact logic from should_visit_url()
        let path_allowed = if norm_start_path.is_empty() || norm_start_path == "/" {
            true  // Root path allows all
        } else {
            norm_url_path == norm_start_path
                || norm_url_path.starts_with(&format!("{}/", norm_start_path))
        };
        
        println!("\nURL path: '{}' - {}", url_path, description);
        println!("  Normalized: '{}'", norm_url_path);
        println!("  Exact match: {}", norm_url_path == norm_start_path);
        println!("  Child match: {}", norm_url_path.starts_with(&format!("{}/", norm_start_path)));
        println!("  Result: {} (expected: {})", path_allowed, should_allow);
        
        assert_eq!(path_allowed, should_allow, 
            "Comparison logic mismatch for '{}' ({})", url_path, description);
    }
}

#[test]
fn test_root_path_edge_case() {
    // This is critical: if start_url is just the domain, it allows everything
    
    println!("\n=== Root Path Edge Case Test ===");
    
    // When start URL is "https://stackoverflow.com/" or "https://stackoverflow.com"
    // The path becomes "/"
    let url1 = ImUrl::parse("https://stackoverflow.com/").unwrap();
    let url2 = ImUrl::parse("https://stackoverflow.com").unwrap();
    
    println!("URL 1: https://stackoverflow.com/");
    println!("  Path: '{}'", url1.path());
    println!("  After trim: '{}'", url1.path().trim_end_matches('/'));
    
    println!("\nURL 2: https://stackoverflow.com");
    println!("  Path: '{}'", url2.path());
    println!("  After trim: '{}'", url2.path().trim_end_matches('/'));
    
    // CRITICAL: If normalized path is empty or "/", ALL paths are allowed
    let path1_norm = url1.path().trim_end_matches('/');
    let path2_norm = url2.path().trim_end_matches('/');
    
    println!("\nRoot path check:");
    println!("  path1_norm.is_empty(): {}", path1_norm.is_empty());
    println!("  path1_norm == '/': {}", path1_norm == "/");
    println!("  path2_norm.is_empty(): {}", path2_norm.is_empty());
    println!("  path2_norm == '/': {}", path2_norm == "/");
    
    // This would allow ALL paths on the domain
    assert!(path1_norm.is_empty() || path1_norm == "/", 
        "Root URL should result in empty or '/' normalized path");
}
