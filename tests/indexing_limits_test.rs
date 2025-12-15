use anyhow::Result;
use futures::StreamExt;
use kodegen_tools_citescrape::config::CrawlConfig;
use kodegen_tools_citescrape::search::{
    engine::SearchEngine,
    indexer::{BatchConfig, IndexingLimits, MarkdownIndexer},
};
use std::fs;
use tempfile::TempDir;

/// Compress a string to gzip format
fn compress_string(content: &str) -> Result<Vec<u8>> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(content.as_bytes())?;
    Ok(encoder.finish()?)
}

/// Create a test file with specified content
fn create_test_file(dir: &TempDir, domain: &str, content: &str) -> Result<()> {
    let crawl_output = dir.path().join("crawl_output");
    let file_dir = crawl_output.join(domain);
    fs::create_dir_all(&file_dir)?;

    let file_path = file_dir.join("index.md.gz");
    let compressed = compress_string(content)?;
    fs::write(file_path, compressed)?;

    Ok(())
}

/// Generate realistic large markdown content with reasonable compression ratio.
///
/// Unlike repetitive patterns like `"# Test\n\n".repeat(N)`, this creates varied
/// content that compresses at typical markdown ratios (5:1 to 15:1), not
/// pathological ratios (100:1+) that trigger zip bomb detection.
///
/// Key technique: Unique section numbers break LZ77 sliding window pattern matching.
fn generate_realistic_markdown(target_size_bytes: usize) -> String {
    use std::fmt::Write;

    let mut content = String::with_capacity(target_size_bytes);
    let mut section = 0;

    // Sample paragraphs with varied content to prevent extreme compression
    let paragraphs = [
        "This section covers important implementation details that developers need to understand.",
        "The architecture follows established patterns for maintainability and performance.",
        "Error handling is implemented consistently across all modules in the system.",
        "Configuration options allow customization without modifying source code directly.",
        "Testing strategies include unit tests, integration tests, and end-to-end validation.",
        "Performance optimizations target the most frequently executed code paths.",
        "Security considerations are addressed at multiple layers of the application.",
        "Documentation is maintained alongside code to ensure accuracy and completeness.",
    ];

    while content.len() < target_size_bytes {
        section += 1;

        // Varied headings with unique numbers break LZ77 pattern matching
        writeln!(content, "# Section {}: Technical Documentation\n", section).unwrap();
        writeln!(content, "## Overview {}.1\n", section).unwrap();

        // Use different paragraphs in rotation with unique identifiers
        for (i, para) in paragraphs.iter().enumerate() {
            writeln!(
                content,
                "{} Reference: section-{}-item-{}\n",
                para, section, i
            )
            .unwrap();
        }

        // Add code blocks (common in real markdown)
        writeln!(
            content,
            "```rust\nfn example_{}() -> Result<(), Error> {{\n    // Implementation {}\n    Ok(())\n}}\n```\n",
            section, section
        )
        .unwrap();

        // Add a list with unique numbering
        writeln!(content, "### Key Points {}.2\n", section).unwrap();
        for i in 1..=5 {
            writeln!(content, "{}. Item {} in section {}", i, i, section).unwrap();
        }
        writeln!(content).unwrap();
    }

    content.truncate(target_size_bytes);
    content
}

#[tokio::test]
async fn test_default_indexing_limits() -> Result<()> {
    let limits = IndexingLimits::default();

    assert_eq!(limits.max_compressed_mb, 20);
    assert_eq!(limits.max_decompressed_mb, 100);
    assert_eq!(limits.max_compression_ratio, 20.0);

    Ok(())
}

#[tokio::test]
async fn test_compressed_file_size_limit() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create a large file that exceeds default 20MB compressed limit
    // Generate ~21MB of content (will compress to less, but still substantial)
    let large_content = "a".repeat(25_000_000); // 25MB of 'a' characters
    create_test_file(&temp_dir, "large.com", &large_content)?;

    let config = CrawlConfig::builder()
        .storage_dir(temp_dir.path().to_path_buf())
        .start_url("https://example.com")
        .build()
        .map_err(anyhow::Error::msg)?;

    let engine = SearchEngine::create(&config).await?;
    let indexer = MarkdownIndexer::new(engine);

    let crawl_output = temp_dir.path().join("crawl_output");

    // Use default limits (20MB compressed)
    let batch_config = BatchConfig::default();
    let (mut stream, _cancel_handle) = indexer.batch_index_directory(crawl_output, batch_config);

    let mut failed = 0;
    while let Some(result) = stream.next().await {
        if let Ok(progress) = result {
            failed = progress.failed;
        }
    }

    // File should be rejected due to compressed size limit
    assert_eq!(failed, 1, "Large compressed file should be rejected");

    Ok(())
}

#[tokio::test]
async fn test_decompressed_content_size_limit() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create content that will exceed default 100MB decompressed limit
    // Use repetitive content that compresses well
    let pattern = "This is a test pattern that will be repeated many times. ";
    let repetitions = 2_000_000; // ~110MB when decompressed
    let large_content = pattern.repeat(repetitions);
    create_test_file(&temp_dir, "huge.com", &large_content)?;

    let config = CrawlConfig::builder()
        .storage_dir(temp_dir.path().to_path_buf())
        .start_url("https://example.com")
        .build()
        .map_err(anyhow::Error::msg)?;

    let engine = SearchEngine::create(&config).await?;
    let indexer = MarkdownIndexer::new(engine);

    let crawl_output = temp_dir.path().join("crawl_output");

    // Use custom limits with higher compressed limit but default decompressed (100MB)
    let batch_config = BatchConfig {
        limits: IndexingLimits {
            max_compressed_mb: 50,    // Allow larger compressed files
            max_decompressed_mb: 100, // But keep decompressed limit at 100MB
            max_compression_ratio: 20.0,
        },
        ..Default::default()
    };
    let (mut stream, _cancel_handle) = indexer.batch_index_directory(crawl_output, batch_config);

    let mut failed = 0;
    while let Some(result) = stream.next().await {
        if let Ok(progress) = result {
            failed = progress.failed;
        }
    }

    // File should be rejected due to decompressed size limit
    assert_eq!(failed, 1, "Large decompressed content should be rejected");

    Ok(())
}

#[tokio::test]
async fn test_compression_ratio_limit() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create content with extremely high compression ratio (simulating zip bomb)
    // Use all zeros which compress extremely well
    let bomb_content = "\0".repeat(1_000_000); // 1MB of zeros -> extremely high compression
    create_test_file(&temp_dir, "bomb.com", &bomb_content)?;

    let config = CrawlConfig::builder()
        .storage_dir(temp_dir.path().to_path_buf())
        .start_url("https://example.com")
        .build()
        .map_err(anyhow::Error::msg)?;

    let engine = SearchEngine::create(&config).await?;
    let indexer = MarkdownIndexer::new(engine);

    let crawl_output = temp_dir.path().join("crawl_output");

    // Use strict compression ratio limit
    let batch_config = BatchConfig {
        limits: IndexingLimits {
            max_compressed_mb: 50,
            max_decompressed_mb: 100,
            max_compression_ratio: 10.0, // Strict ratio to catch this
        },
        ..Default::default()
    };
    let (mut stream, _cancel_handle) = indexer.batch_index_directory(crawl_output, batch_config);

    let mut failed = 0;
    while let Some(result) = stream.next().await {
        if let Ok(progress) = result {
            failed = progress.failed;
        }
    }

    // File should be rejected due to suspicious compression ratio
    assert_eq!(failed, 1, "Zip bomb should be detected and rejected");

    Ok(())
}

#[tokio::test]
async fn test_custom_limits_allow_larger_files() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Generate ~10MB of realistic markdown content with reasonable compression ratio.
    // Unlike "# Test\n\n".repeat(N), this has varied content that won't trigger
    // zip bomb detection while still being large enough to test size limits.
    //
    // Expected compression ratio: 5:1 to 15:1 (well under the 50.0 limit)
    let content = generate_realistic_markdown(10_000_000); // 10MB
    create_test_file(&temp_dir, "largedoc.com", &content)?;

    let config = CrawlConfig::builder()
        .storage_dir(temp_dir.path().to_path_buf())
        .start_url("https://example.com")
        .build()
        .map_err(anyhow::Error::msg)?;

    let engine = SearchEngine::create(&config).await?;
    let indexer = MarkdownIndexer::new(engine);

    let crawl_output = temp_dir.path().join("crawl_output");

    // Use generous limits for large files - these test SIZE limits, not compression ratio
    let batch_config = BatchConfig {
        limits: IndexingLimits {
            max_compressed_mb: 100,      // Allow large compressed files
            max_decompressed_mb: 500,    // Allow large decompressed files
            max_compression_ratio: 50.0, // Reasonable for real markdown content
        },
        ..Default::default()
    };
    let (mut stream, _cancel_handle) = indexer.batch_index_directory(crawl_output, batch_config);

    let mut processed = 0;
    let mut failed = 0;
    while let Some(result) = stream.next().await {
        if let Ok(progress) = result {
            processed = progress.processed;
            failed = progress.failed;
        }
    }

    // File should be successfully processed with generous limits
    assert_eq!(processed, 1, "File should be processed successfully");
    assert_eq!(failed, 0, "No files should fail with generous limits");

    Ok(())
}

#[tokio::test]
async fn test_normal_markdown_files_within_limits() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create normal-sized markdown files
    create_test_file(&temp_dir, "doc1.com", "# Document 1\n\nNormal content")?;
    create_test_file(&temp_dir, "doc2.com", "# Document 2\n\nMore normal content")?;
    create_test_file(&temp_dir, "doc3.com", "# Document 3\n\nEven more content")?;

    let config = CrawlConfig::builder()
        .storage_dir(temp_dir.path().to_path_buf())
        .start_url("https://example.com")
        .build()
        .map_err(anyhow::Error::msg)?;

    let engine = SearchEngine::create(&config).await?;
    let indexer = MarkdownIndexer::new(engine);

    let crawl_output = temp_dir.path().join("crawl_output");

    // Use default limits
    let batch_config = BatchConfig::default();
    let (mut stream, _cancel_handle) = indexer.batch_index_directory(crawl_output, batch_config);

    let mut processed = 0;
    let mut failed = 0;
    while let Some(result) = stream.next().await {
        if let Ok(progress) = result {
            processed = progress.processed;
            failed = progress.failed;
        }
    }

    // All files should be processed successfully
    assert_eq!(processed, 3, "All normal files should be processed");
    assert_eq!(failed, 0, "No normal files should fail");

    Ok(())
}
