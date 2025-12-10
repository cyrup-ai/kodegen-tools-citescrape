//! Single URL fetch example for debugging markdown conversion
//!
//! Usage: cargo run --example single_url -- <URL>
//! Example: cargo run --example single_url -- https://code.claude.com/docs/en/headless

use kodegen_tools_citescrape::{CrawlConfig, crawl};
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
        .search_index_dir(None::<PathBuf>)
        .crawl_rate_rps(2.0)
        .allow_subdomains(false)
        .build()
        .expect("Failed to build config");

    // Execute crawl
    match crawl(config).await {
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
                println!("{}", content);
                log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            } else if path.extension().is_some_and(|ext| ext == "html") {
                log::info!("📄 HTML file: {}", path.display());
            }
        }
        Ok(())
    }

    find_files(&output_dir)?;

    Ok(())
}
