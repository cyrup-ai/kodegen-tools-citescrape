<div align="center">
  <img src="assets/img/banner.png" alt="Kodegen AI Banner" width="100%" />
</div>

# kodegen-tools-citescrape

[![License](https://img.shields.io/badge/license-Apache%202.0%20OR%20MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/rust-nightly-orange.svg)](https://www.rust-lang.org/)

> Memory-efficient, Blazing-Fast MCP tools for code generation agents

**kodegen-tools-citescrape** is a high-performance web crawling and search toolkit designed specifically for AI coding agents. It provides Model Context Protocol (MCP) tools that enable agents to crawl websites with stealth browser automation, extract content as markdown, and perform full-text search on crawled data.

## Features

- 🚀 **Blazing Fast**: Multi-threaded crawling with intelligent rate limiting and domain concurrency
- 🔍 **Full-Text Search**: Dual-index search powered by Tantivy (markdown + plaintext)
- 🥷 **Stealth Automation**: Advanced browser fingerprint evasion (kromekover) to avoid bot detection
- 📄 **Smart Extraction**: HTML → Markdown conversion with inline CSS and link rewriting
- 🎯 **MCP Native**: First-class Model Context Protocol support for AI agents
- 💾 **Memory Efficient**: Streaming architecture with optional gzip compression
- ⚡ **Production Ready**: Circuit breakers, retry logic, and automatic cleanup

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/cyrup-ai/kodegen-tools-citescrape.git
cd kodegen-tools-citescrape

# Build the project
cargo build --release
```

### Running the MCP Server

```bash
# Start the HTTP server (default port: 30445)
cargo run --release --bin kodegen-citescrape
```

The server will expose 4 MCP tools over HTTP transport, typically managed by the `kodegend` daemon.

### Using as a Library

```rust
use kodegen_tools_citescrape::{CrawlConfig, ChromiumoxideCrawler, Crawler};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure the crawler
    let config = CrawlConfig::builder()
        .start_url("https://docs.rs/tokio")?
        .storage_dir("./crawl_output")?
        .max_depth(3)
        .max_pages(100)
        .follow_external_links(false)
        .build();

    // Create and run crawler
    let crawler = ChromiumoxideCrawler::new(config);
    crawler.crawl().await?;

    Ok(())
}
```

## MCP Tools

The server provides four tools for AI agents:

### 1. `scrape_url`

Initiates a background web crawl with automatic search indexing.

**Arguments:**
- `url` (required): Starting URL to crawl
- `output_dir` (optional): Directory to save results (default: temp dir)
- `max_depth` (optional): Maximum link depth (default: 3)
- `max_pages` (optional): Maximum pages to crawl (default: 100)
- `follow_external_links` (optional): Crawl external domains (default: false)
- `enable_search` (optional): Enable full-text indexing (default: false)

**Returns:**
- `crawl_id`: UUID for tracking the crawl
- `output_dir`: Path where results are saved
- `status`: Initial status ("running")

**Example:**
```json
{
  "url": "https://docs.rs/tokio",
  "max_depth": 2,
  "max_pages": 50,
  "enable_search": true
}
```

### 2. `scrape_check_results`

Retrieves markdown content from a crawl session.

**Arguments:**
- `crawl_id` (required): UUID from `scrape_url`
- `offset` (optional): Pagination offset (default: 0)
- `limit` (optional): Max results to return (default: 10)
- `include_progress` (optional): Include crawl progress stats (default: false)

**Returns:**
- `status`: "running", "completed", or "failed"
- `results`: Array of markdown documents with metadata
- `total_pages`: Total pages crawled
- `progress` (if requested): Crawl statistics

### 3. `scrape_search_results`

Performs full-text search on indexed crawl content.

**Arguments:**
- `crawl_id` (required): UUID from `scrape_url`
- `query` (required): Search query string
- `limit` (optional): Max results (default: 10)
- `search_type` (optional): "markdown" or "plaintext" (default: "plaintext")

**Returns:**
- `results`: Ranked search results with snippets
- `total_hits`: Total matching documents

**Example:**
```json
{
  "crawl_id": "550e8400-e29b-41d4-a716-446655440000",
  "query": "async runtime",
  "limit": 5
}
```

### 4. `web_search`

Executes a web search using a stealth browser.

**Arguments:**
- `query` (required): Search query
- `engine` (optional): "google", "bing", or "duckduckgo" (default: "google")
- `max_results` (optional): Maximum results (default: 10)

**Returns:**
- `results`: Array of search results with titles, URLs, and snippets

## Architecture

### Core Components

- **Crawl Engine** (`src/crawl_engine/`): Multi-threaded crawler with rate limiting, circuit breakers, and domain concurrency control
- **Kromekover** (`src/kromekover/`): Browser stealth system that injects JavaScript to evade bot detection
- **Content Saver** (`src/content_saver/`): Pipeline for HTML preprocessing, markdown conversion, compression, and indexing
- **Search Engine** (`src/search/`): Tantivy-based dual-index system (markdown + plaintext)
- **MCP Tools** (`src/mcp/`): Tool implementations and session management

### Stealth Features

The kromekover module provides advanced browser fingerprint evasion:

- Navigator property spoofing (webdriver, vendor, platform)
- WebGL vendor/renderer override
- Canvas fingerprint noise injection
- CDP property cleanup (removes Chromium automation artifacts)
- Plugin and codec spoofing
- User-Agent data modernization (Chrome 129+)

## Configuration

### Crawl Configuration

The `CrawlConfig` builder provides extensive customization:

```rust
let config = CrawlConfig::builder()
    .start_url("https://example.com")?
    .storage_dir("./output")?
    .max_depth(5)
    .max_pages(500)
    .follow_external_links(true)
    .rate_limit_delay_ms(1000)
    .max_concurrent_requests_per_domain(2)
    .timeout_seconds(30)
    .enable_compression(true)
    .build();
```

### Rate Limiting

Three-layer rate limiting system:

1. **Per-domain delay**: Minimum time between requests to same domain (default: 1s)
2. **Domain concurrency**: Max simultaneous requests per domain (default: 2)
3. **Circuit breaker**: Pause domain after N errors (default: 5)

## Development

### Prerequisites

- Rust nightly toolchain
- Chrome/Chromium browser (automatically downloaded if not found)

### Building

```bash
# Development build
cargo build

# Release build
cargo build --release

# Check without building
cargo check
```

### Testing

```bash
# Run all tests with nextest (recommended)
cargo nextest run

# Run specific test
cargo nextest run test_name

# Standard cargo test
cargo test

# Run with output
cargo test test_name -- --nocapture
```

### Running Examples

```bash
# Basic crawl demo
cargo run --example citescrape_demo

# Interactive TUI crawler
cargo run --example direct_crawl_ratatui

# Web search example
cargo run --example direct_web_search
```

### Code Quality

```bash
# Format code
cargo fmt

# Lint
cargo clippy

# Check all warnings
cargo clippy -- -W clippy::all
```

## Project Structure

```
src/
├── browser_setup.rs       # Chrome launching and stealth setup
├── config/                # Type-safe config builder
├── content_saver/         # HTML/markdown saving pipeline
├── crawl_engine/          # Core crawling logic
├── crawl_events/          # Progress event streaming
├── kromekover/            # Browser stealth evasion
├── mcp/                   # MCP tool implementations
├── page_extractor/        # Content and link extraction
├── search/                # Tantivy full-text search
├── web_search/            # Browser manager for searches
└── main.rs                # HTTP server entry point
```

## Performance

- **Multi-threaded**: Rayon-based parallel processing
- **Streaming**: Memory-efficient content processing
- **Incremental indexing**: Background search index updates
- **Smart caching**: Bloom filters and LRU caches
- **Compressed storage**: Optional gzip compression

## Use Cases

- **Documentation Crawling**: Extract and index technical docs for AI context
- **Code Repository Mining**: Crawl source code hosting sites
- **Research Aggregation**: Gather and search domain-specific content
- **Competitive Analysis**: Monitor and analyze competitor websites
- **Content Archival**: Create offline markdown archives of websites

## Roadmap

- [ ] JavaScript rendering for SPAs
- [ ] PDF extraction support
- [ ] Sitemap.xml parsing
- [ ] robots.txt compliance modes
- [ ] Distributed crawling
- [ ] GraphQL API endpoint
- [ ] Real-time crawl streaming

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes with tests
4. Run `cargo fmt` and `cargo clippy`
5. Submit a pull request

## License

This project is dual-licensed under:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE.md))
- MIT License ([LICENSE-MIT](LICENSE.md))

You may choose either license for your use.

## Acknowledgments

Built with:
- [chromiumoxide](https://github.com/mattsse/chromiumoxide) - Chrome DevTools Protocol
- [tantivy](https://github.com/quickwit-oss/tantivy) - Full-text search engine
- [scraper](https://github.com/causal-agent/scraper) - HTML parsing
- [tokio](https://tokio.rs/) - Async runtime

## Links

- **Homepage**: [https://kodegen.ai](https://kodegen.ai)
- **Repository**: [https://github.com/cyrup-ai/kodegen-tools-citescrape](https://github.com/cyrup-ai/kodegen-tools-citescrape)
- **Issues**: [GitHub Issues](https://github.com/cyrup-ai/kodegen-tools-citescrape/issues)
- **Documentation**: See [CLAUDE.md](CLAUDE.md) for architecture details

---

Made with ❤️ by [KODEGEN.ᴀɪ](https://kodegen.ai) | Copyright © 2025 David Maple
