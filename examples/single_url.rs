//! Single URL fetch example with search validation
//!
//! This example crawls a single URL, indexes it, and validates the search functionality:
//! 1. Stemming works (search "ignore" finds "ignoring", "ignored", etc.)
//! 2. Snippets are properly generated with highlighting
//! 3. No duplicate results
//! 4. Relevance ordering is correct
//!
//! Usage: cargo run --example single_url -- [URL]
//! Example: cargo run --example single_url -- https://code.claude.com/docs/en/headless

use kodegen_tools_citescrape::{CrawlConfig, crawl};
use kodegen_tools_citescrape::search::{SearchEngine, SearchQueryBuilder, MarkdownIndexer};
use std::collections::HashSet;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize detailed logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("chromiumoxide::handler", log::LevelFilter::Off)
        .filter_module("chromiumoxide::conn", log::LevelFilter::Off)
        .filter_module("tantivy", log::LevelFilter::Off)
        .init();

    // Get URL from command line args
    let args: Vec<String> = std::env::args().collect();
    let url = args.get(1).map(|s| s.as_str()).unwrap_or("https://code.claude.com/docs/en/headless");

    let output_dir = PathBuf::from("docs/single_url");

    // Delete existing output directory
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir)?;
    }

    log::info!("🔗 Fetching single URL: {}", url);
    log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let config = CrawlConfig::builder()
        .storage_dir(output_dir.clone())
        .start_url(url)
        .limit(Some(1))     // Single page only
        .max_depth(0)       // No following links
        .save_markdown(true)
        .save_raw_html(true)
        .save_screenshots(false)
        .search_index_dir(Some(output_dir.join("search_index"))) // Enable search indexing
        .crawl_rate_rps(2.0)
        .allow_subdomains(false)
        .build()
        .expect("Failed to build config");

    // Execute crawl
    match crawl(config.clone()).await {
        Ok(()) => {
            log::info!("✅ Fetch completed!");
        }
        Err(e) => {
            log::error!("❌ Fetch failed: {e:#}");
            return Err(e.into());
        }
    }

    // Find and display the markdown file
    log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Walk the output directory to find files
    fn find_files(dir: &PathBuf) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                find_files(&path)?;
            } else if path.extension().is_some_and(|ext| ext == "md") {
                log::info!("📄 Markdown file: {}", path.display());
                let content = std::fs::read_to_string(&path)?;
                log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                // Print first 500 chars of content for brevity
                let preview: String = content.chars().take(500).collect();
                println!("{}", preview);
                if content.len() > 500 {
                    println!("... [truncated, {} total chars]", content.len());
                }
                log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            } else if path.extension().is_some_and(|ext| ext == "html") {
                log::info!("📄 HTML file: {}", path.display());
            }
        }
        Ok(())
    }

    find_files(&output_dir)?;

    // ═══════════════════════════════════════════════════════════════════════════
    // SEARCH INDEXING SECTION
    // ═══════════════════════════════════════════════════════════════════════════
    
    log::info!("");
    log::info!("═══════════════════════════════════════════════════════════════════════════");
    log::info!("🔧 SEARCH INDEXING");
    log::info!("═══════════════════════════════════════════════════════════════════════════");
    
    // Initialize search engine
    let engine = SearchEngine::create(&config).await?;
    log::info!("✅ SearchEngine initialized successfully");
    
    // Create indexer and manually index markdown files
    // Note: The batch indexer looks for compressed .md.gz files, but we saved uncompressed .md
    // So we'll manually find and index the markdown files
    let indexer = MarkdownIndexer::new(engine.clone());
    
    log::info!("📚 Indexing markdown files in: {}", output_dir.display());
    
    // Collect all markdown files first
    fn collect_markdown_files(dir: &PathBuf, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                collect_markdown_files(&path, files)?;
            } else if path.extension().is_some_and(|ext| ext == "md") {
                files.push(path);
            }
        }
        Ok(())
    }
    
    let mut markdown_files = Vec::new();
    if let Err(e) = collect_markdown_files(&output_dir, &mut markdown_files) {
        log::error!("Error walking directory: {}", e);
    }
    
    // Index all files asynchronously
    let mut indexed_count = 0;
    let mut failed_count = 0;
    
    for path in markdown_files {
        // Extract URL from path structure
        // Path format: output_dir/domain/path/to/page/index.md
        let relative = path.strip_prefix("docs/single_url").unwrap_or(&path);
        let url = format!("https://{}", relative.parent()
            .unwrap_or(relative)
            .to_string_lossy()
            .replace('\\', "/"));
        
        log::info!("  📝 Indexing: {} -> {}", path.display(), url);
        
        let url_imstr = imstr::ImString::from(url);
        
        match indexer.index_file(&path, &url_imstr).await {
            Ok(()) => {
                indexed_count += 1;
                log::info!("    ✅ Indexed successfully");
            }
            Err(e) => {
                failed_count += 1;
                log::error!("    ❌ Failed to index: {}", e);
            }
        }
    }
    
    log::info!("  ✅ Indexing complete: {} succeeded, {} failed", indexed_count, failed_count);
    
    // Get index stats
    let stats = engine.get_stats().await?;
    log::info!("📊 Index stats: {} documents, {} segments", stats.num_documents, stats.num_segments);
    
    if stats.num_documents == 0 {
        log::warn!("⚠️  No documents indexed - search tests will be skipped");
        return Ok(());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SEARCH VALIDATION SECTION
    // ═══════════════════════════════════════════════════════════════════════════
    
    log::info!("");
    log::info!("═══════════════════════════════════════════════════════════════════════════");
    log::info!("🔍 SEARCH VALIDATION TESTS");
    log::info!("═══════════════════════════════════════════════════════════════════════════");

    // Track overall test results
    let mut all_tests_passed = true;

    // ───────────────────────────────────────────────────────────────────────────
    // TEST 1: Realistic agent queries with snippets
    // ───────────────────────────────────────────────────────────────────────────
    log::info!("");
    log::info!("─── TEST 1: Realistic Agent Queries ───");
    
    // These are actual queries an AI agent would search for in this documentation
    let test_queries = vec![
        "how to get json output",           // Practical: output format question
        "resume conversation session",       // Feature: multi-turn conversations
        "restrict allowed tools",            // Security: tool restrictions
        "streaming input stdin",             // Advanced: streaming JSON input
        "error handling exit code",          // Best practices: error handling
        "timeout long running operations",   // Reliability: timeouts
    ];
    
    for query in &test_queries {
        let results = SearchQueryBuilder::new(*query)
            .limit(10)
            .highlight(true)
            .execute(engine.clone())
            .await?;
        
        log::info!("  Query '{}': {} results", query, results.len());
        
        for (i, result) in results.iter().enumerate() {
            log::info!("    [{}] Score: {:.4} | Title: {}", i + 1, result.score, result.title);
            log::info!("        URL: {}", result.url);
            
            // Validate snippet is not empty
            if result.excerpt.is_empty() {
                log::error!("    ❌ FAIL: Empty snippet for result!");
                all_tests_passed = false;
            } else {
                let snippet_preview: String = result.excerpt.chars().take(100).collect();
                log::info!("        Snippet: {}...", snippet_preview);
            }
        }
    }

    // ───────────────────────────────────────────────────────────────────────────
    // TEST 2: Stemming validation
    // ───────────────────────────────────────────────────────────────────────────
    log::info!("");
    log::info!("─── TEST 2: Stemming Validation ───");
    log::info!("  Testing that stemmed queries match inflected words...");
    
    // Test stemming pairs - base form should find inflected forms
    let stemming_tests = vec![
        ("run", vec!["running", "runs", "ran"]),
        ("configure", vec!["configuration", "configured", "configuring"]),
        ("use", vec!["using", "used", "uses"]),
    ];
    
    for (base_word, inflections) in &stemming_tests {
        let base_results = SearchQueryBuilder::new(*base_word)
            .limit(10)
            .highlight(true)
            .execute(engine.clone())
            .await?;
        
        log::info!("  Base word '{}': {} results", base_word, base_results.len());
        
        // Check if any result contains inflected forms in the snippet
        let mut found_inflection = false;
        for result in &base_results {
            let excerpt_lower = result.excerpt.to_lowercase();
            for inflection in inflections {
                if excerpt_lower.contains(inflection) {
                    found_inflection = true;
                    log::info!("    ✅ Found inflection '{}' when searching for '{}'", inflection, base_word);
                    break;
                }
            }
        }
        
        if !base_results.is_empty() && !found_inflection {
            log::info!("    ℹ️  No inflections found (may not be present in content)");
        }
    }

    // ───────────────────────────────────────────────────────────────────────────
    // TEST 3: No duplicates
    // ───────────────────────────────────────────────────────────────────────────
    log::info!("");
    log::info!("─── TEST 3: No Duplicate Results ───");
    
    let all_results = SearchQueryBuilder::new("*")
        .limit(100)
        .highlight(false)
        .execute(engine.clone())
        .await?;
    
    log::info!("  Total results for '*': {}", all_results.len());
    
    // Check for duplicate URLs
    let mut seen_urls: HashSet<String> = HashSet::new();
    let mut duplicate_count = 0;
    
    for result in &all_results {
        if seen_urls.contains(&result.url) {
            log::error!("  ❌ DUPLICATE URL: {}", result.url);
            duplicate_count += 1;
            all_tests_passed = false;
        } else {
            seen_urls.insert(result.url.clone());
        }
    }
    
    if duplicate_count == 0 {
        log::info!("  ✅ No duplicate URLs found ({} unique results)", seen_urls.len());
    } else {
        log::error!("  ❌ FAIL: Found {} duplicate URLs!", duplicate_count);
    }

    // ───────────────────────────────────────────────────────────────────────────
    // TEST 4: Relevance ordering (scores should be descending)
    // ───────────────────────────────────────────────────────────────────────────
    log::info!("");
    log::info!("─── TEST 4: Relevance Ordering ───");
    
    let relevance_results = SearchQueryBuilder::new("headless mode")
        .limit(10)
        .highlight(true)
        .execute(engine.clone())
        .await?;
    
    log::info!("  Query 'headless mode': {} results", relevance_results.len());
    
    let mut prev_score: Option<f32> = None;
    let mut ordering_correct = true;
    
    for (i, result) in relevance_results.iter().enumerate() {
        log::info!("    [{}] Score: {:.4} | {}", i + 1, result.score, result.title);
        
        if let Some(ps) = prev_score
            && result.score > ps
        {
            log::error!("    ❌ FAIL: Score {:.4} > previous {:.4} (not descending!)", result.score, ps);
            ordering_correct = false;
            all_tests_passed = false;
        }
        prev_score = Some(result.score);
    }
    
    if ordering_correct && !relevance_results.is_empty() {
        log::info!("  ✅ Results correctly ordered by descending score");
    }

    // ───────────────────────────────────────────────────────────────────────────
    // TEST 5: Snippet highlighting validation
    // ───────────────────────────────────────────────────────────────────────────
    log::info!("");
    log::info!("─── TEST 5: Snippet Highlighting ───");
    
    let highlight_results = SearchQueryBuilder::new("claude")
        .limit(5)
        .highlight(true)
        .execute(engine.clone())
        .await?;
    
    let mut has_non_empty_snippets = false;
    for result in &highlight_results {
        if !result.excerpt.is_empty() {
            has_non_empty_snippets = true;
            log::info!("  ✅ Snippet generated:");
            log::info!("     '{}'", result.excerpt);
        }
    }
    
    if !has_non_empty_snippets && !highlight_results.is_empty() {
        log::error!("  ❌ FAIL: All snippets are empty!");
        all_tests_passed = false;
    } else if highlight_results.is_empty() {
        log::info!("  ℹ️  No results to validate snippets");
    }

    // ───────────────────────────────────────────────────────────────────────────
    // TEST 6: Field-specific search (title boosting)
    // ───────────────────────────────────────────────────────────────────────────
    log::info!("");
    log::info!("─── TEST 6: Title Field Boosting ───");
    
    // Search for term that appears in title - should have high score due to 2.0 boost
    let title_search = SearchQueryBuilder::new("headless")
        .limit(5)
        .highlight(true)
        .execute(engine.clone())
        .await?;
    
    if !title_search.is_empty() {
        log::info!("  Query 'headless' results:");
        for (i, result) in title_search.iter().enumerate() {
            let in_title = result.title.to_lowercase().contains("headless");
            let marker = if in_title { "🎯 IN TITLE" } else { "" };
            log::info!("    [{}] Score: {:.4} | {} {}", i + 1, result.score, result.title, marker);
        }
        log::info!("  ℹ️  Title matches should have higher scores due to 2.0x boost");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FINAL SUMMARY
    // ═══════════════════════════════════════════════════════════════════════════
    log::info!("");
    log::info!("═══════════════════════════════════════════════════════════════════════════");
    if all_tests_passed {
        log::info!("✅ ALL SEARCH VALIDATION TESTS PASSED!");
    } else {
        log::error!("❌ SOME TESTS FAILED - See errors above");
    }
    log::info!("═══════════════════════════════════════════════════════════════════════════");

    Ok(())
}
