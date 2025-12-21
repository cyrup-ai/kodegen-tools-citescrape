use kodegen_tools_citescrape::web_search;
use kodegen_tools_citescrape::{BrowserPool, BrowserPoolConfig};
use std::sync::Arc;

#[tokio::test]
#[ignore] // Requires browser installation
async fn test_search_basic() {
    // Create and start browser pool
    let pool = Arc::new(BrowserPool::new(BrowserPoolConfig::default()));
    pool.start().await.unwrap();

    // Perform search
    let results = web_search::search_with_pool(&pool, "rust programming")
        .await
        .unwrap();

    // Verify results
    assert!(!results.results.is_empty());
    assert_eq!(results.query, "rust programming");

    // Cleanup
    pool.shutdown().await.unwrap();
}
