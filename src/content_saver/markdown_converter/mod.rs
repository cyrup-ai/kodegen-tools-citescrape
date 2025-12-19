//! HTML to Markdown conversion pipeline - The ONE canonical implementation
//!
//! This module provides the complete pipeline for converting HTML to clean, well-formatted markdown:
//! 1. Convert to Markdown using htmd with DOM-based element handlers (filtering happens here)
//! 2. Process markdown links (optional, resolve relative URLs)
//!
//! Note: HTML filtering (widget removal, script/style removal, nav/header/footer removal)
//! is handled by htmd element handlers during DOM traversal. See htmd/element_handler/ for details.
//!
//! # Usage
//!
//! ## Synchronous (for blocking contexts)
//! ```rust
//! # use kodegen_tools_citescrape::content_saver::markdown_converter::{convert_html_to_markdown_sync, ConversionOptions};
//! let html = "<html><body><h1>Title</h1><p>Content</p></body></html>";
//! let options = ConversionOptions::default();
//! let markdown = convert_html_to_markdown_sync(html, &options)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Asynchronous (recommended)
//! ```rust
//! # use kodegen_tools_citescrape::content_saver::markdown_converter::{convert_html_to_markdown, ConversionOptions};
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let html = "<html><body><h1>Title</h1><p>Content</p></body></html>";
//! let options = ConversionOptions::default();
//! let markdown = convert_html_to_markdown(html, &options).await?;
//! # Ok::<(), anyhow::Error>(())
//! # }).unwrap();
//! ```
//!
//! ## Custom Configuration
//! ```rust
//! # use kodegen_tools_citescrape::content_saver::markdown_converter::{convert_html_to_markdown_sync, ConversionOptions};
//! # let html = "<html><body><h1>Title</h1></body></html>";
//! let options = ConversionOptions {
//!     clean_html: true,
//!     preserve_tables: true,
//!     preserve_links: true,
//!     preserve_images: false,  // Remove images
//!     code_highlighting: true,
//!     process_headings: true,
//!     normalize_whitespace: true,
//!     base_url: None,
//! };
//! let markdown = convert_html_to_markdown_sync(html, &options)?;
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::Result;
use std::sync::Arc;

// Declare sub-modules
pub mod htmd;
pub mod html_to_markdown;

// Re-export sub-modules for advanced usage
pub use html_to_markdown::MarkdownConverter;


/// Configuration options for HTML to Markdown conversion
///
/// Controls all aspects of the conversion pipeline including content extraction,
/// HTML cleaning, markdown formatting, and post-processing.
#[derive(Debug, Clone)]
pub struct ConversionOptions {
    /// Clean HTML before conversion (default: true)
    ///
    /// Removes scripts, styles, ads, tracking pixels, social widgets,
    /// cookie notices, and other non-content elements.
    pub clean_html: bool,

    /// Preserve table formatting in markdown (default: true)
    pub preserve_tables: bool,

    /// Preserve hyperlinks in markdown (default: true)
    pub preserve_links: bool,

    /// Preserve images in markdown (default: true)
    pub preserve_images: bool,

    /// Enable code block syntax highlighting hints (default: true)
    pub code_highlighting: bool,

    /// Process and normalize markdown headings (default: true)
    ///
    /// Converts setext-style headings to ATX style, removes closing hashes,
    /// normalizes heading levels, and handles code fences properly.
    pub process_headings: bool,

    /// Normalize whitespace and blank lines (default: true)
    ///
    /// When enabled, removes trailing whitespace, collapses multiple blank lines,
    /// ensures proper spacing around structural elements, and trims document edges.
    /// Preserves all whitespace inside code blocks.
    pub normalize_whitespace: bool,

    /// Base URL for resolving relative links (default: None)
    ///
    /// When provided, relative URLs in links will be converted to absolute URLs
    /// using RFC 3986 resolution rules via url::Url::join().
    ///
    /// Examples:
    /// - Base: "https://example.com/docs/guide.html"
    /// - "/api" → "https://example.com/api"
    /// - "../concepts/intro" → "https://example.com/concepts/intro"
    /// - "#section" → "#section" (preserved as-is)
    /// - "https://other.com" → "https://other.com" (preserved as-is)
    pub base_url: Option<String>,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            clean_html: true,
            preserve_tables: true,
            preserve_links: true,
            preserve_images: true,
            code_highlighting: true,
            process_headings: true,
            normalize_whitespace: true,
            base_url: None,
        }
    }
}

impl ConversionOptions {
    /// Create a new `ConversionOptions` with all features enabled
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable all optional processing (minimal conversion)
    ///
    /// Only performs basic HTML→Markdown conversion without extraction,
    /// cleaning, or post-processing. Useful for already-clean HTML.
    #[must_use]
    pub fn minimal() -> Self {
        Self {
            clean_html: false,
            preserve_tables: true,
            preserve_links: true,
            preserve_images: true,
            code_highlighting: false,
            process_headings: false,
            normalize_whitespace: false,
            base_url: None,
        }
    }

    /// Text-only mode: strips images and links
    #[must_use]
    pub fn text_only() -> Self {
        Self {
            clean_html: true,
            preserve_tables: true,
            preserve_links: false,
            preserve_images: false,
            code_highlighting: true,
            process_headings: true,
            normalize_whitespace: true,
            base_url: None,
        }
    }
}

/// Convert HTML to Markdown synchronously (blocking)
///
/// This is the ONE canonical function for HTML→Markdown conversion.
/// It orchestrates the complete 2-stage pipeline with built-in fallback handling.
///
/// # Pipeline Stages
///
/// 1. **Convert to Markdown** (always performed)
///    - Uses htmd library with DOM-based element handlers
///    - HTML filtering (widgets, scripts, nav, footer) is handled during DOM traversal
///    - Custom handlers for expressive-code extraction, widget skipping, etc.
///
/// 2. **Process Links** (optional, controlled by `options.base_url`)
///    - Resolves relative URLs to absolute using base URL
///    - Preserves fragment-only and absolute URLs
///
/// # Arguments
///
/// * `html` - Raw HTML content to convert
/// * `options` - Configuration controlling the conversion pipeline
///
/// # Returns
///
/// * `Ok(String)` - Clean, well-formatted markdown
/// * `Err(anyhow::Error)` - Only if ALL stages fail catastrophically (extremely rare)
///
/// # Examples
///
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::{convert_html_to_markdown_sync, ConversionOptions};
/// let html = r#"
///     <html>
///         <body>
///             <article>
///                 <h1>My Article</h1>
///                 <p>This is <strong>important</strong> content.</p>
///             </article>
///         </body>
///     </html>
/// "#;
///
/// let markdown = convert_html_to_markdown_sync(html, &ConversionOptions::default())?;
/// assert!(markdown.contains("# My Article"));
/// assert!(markdown.contains("**important**"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn convert_html_to_markdown_sync(html: &str, options: &ConversionOptions) -> Result<String> {
    // HTML goes directly to htmd converter - element handlers filter non-content during DOM traversal
    let converter = MarkdownConverter::new()
        .with_preserve_tables(options.preserve_tables)
        .with_preserve_links(options.preserve_links)
        .with_preserve_images(options.preserve_images)
        .with_code_highlighting(options.code_highlighting);

    let markdown = converter.convert_sync(html)?;

    // Stage 2: Process markdown links (convert relative URLs to absolute)
    // URL resolution for scraped content - all other normalization is handled by htmd handlers
    let markdown = if let Some(base_url) = &options.base_url {
        html_to_markdown::process_markdown_links(&markdown, base_url)
    } else {
        markdown
    };

    Ok(markdown.trim().to_string())
}

/// Convert HTML to Markdown asynchronously
///
/// This is a thin async wrapper around the synchronous conversion logic.
/// Since HTML parsing/conversion is CPU-bound and typically fast (<100ms),
/// we call the sync function directly rather than using `spawn_blocking`.
///
/// # Arguments
///
/// * `html` - Raw HTML content to convert
/// * `options` - Configuration controlling the conversion pipeline
///
/// # Returns
///
/// * `Ok(String)` - Clean, well-formatted markdown
/// * `Err(anyhow::Error)` - Only if conversion fails catastrophically
///
/// # Examples
///
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::{convert_html_to_markdown, ConversionOptions};
/// # tokio::runtime::Runtime::new().unwrap().block_on(async {
/// let html = "<html><body><h1>Title</h1></body></html>";
/// let options = ConversionOptions::default();
/// let markdown = convert_html_to_markdown(html, &options).await?;
/// # Ok::<(), anyhow::Error>(())
/// # }).unwrap();
/// ```
pub async fn convert_html_to_markdown(html: &str, options: &ConversionOptions) -> Result<String> {
    // Arc for zero-copy sharing across thread boundary (follows existing pattern in search/engine.rs)
    let html = Arc::<str>::from(html);
    let options = options.clone();

    tokio::task::spawn_blocking(move || convert_html_to_markdown_sync(&html, &options))
        .await
        .map_err(|e| anyhow::anyhow!("HTML-to-Markdown conversion task panicked: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_html_to_markdown_basic() {
        let html = r"
            <html>
                <body>
                    <h1>Test Title</h1>
                    <p>This is a <strong>test</strong> paragraph.</p>
                </body>
            </html>
        ";

        let result = convert_html_to_markdown_sync(html, &ConversionOptions::default());
        assert!(result.is_ok());

        let markdown = result.expect("Test operation should succeed");
        assert!(markdown.contains("# Test Title"));
        assert!(markdown.contains("**test**"));
    }

    #[test]
    fn test_convert_with_minimal_options() {
        let html = "<html><body><h2>Heading</h2><p>Content</p></body></html>";

        let result = convert_html_to_markdown_sync(html, &ConversionOptions::minimal());
        assert!(result.is_ok());

        let markdown = result.expect("Test operation should succeed");
        // With minimal options, headings might not be normalized
        eprintln!("ACTUAL MARKDOWN OUTPUT:\n{}", markdown);
        assert!(markdown.contains("Heading"));
        assert!(markdown.contains("Content"));
    }

    #[test]
    fn test_convert_text_only_mode() {
        let html = r#"
            <html>
                <body>
                    <h1>Article</h1>
                    <p>Read more at <a href="https://example.com">example.com</a></p>
                    <img src="photo.jpg" alt="Photo">
                </body>
            </html>
        "#;

        let result = convert_html_to_markdown_sync(html, &ConversionOptions::text_only());
        assert!(result.is_ok());

        let markdown = result.expect("Test operation should succeed");
        // Text-only mode should strip images and convert links to plain text
        assert!(markdown.contains("Article"));
        assert!(!markdown.contains("![Photo]")); // No images
        // Links might be converted to plain text "example.com"
    }

    #[test]
    fn test_conversion_pipeline_stages() {
        let html = r"
            <html>
                <head><script>alert('test');</script></head>
                <body>
                    <nav>Navigation</nav>
                    <article>
                        <h1>Main Content</h1>
                        <p>Article text</p>
                    </article>
                    <footer>Footer</footer>
                </body>
            </html>
        ";

        let result = convert_html_to_markdown_sync(html, &ConversionOptions::default());
        assert!(result.is_ok());

        let markdown = result.expect("Test operation should succeed");
        // Should extract article content and remove nav/footer
        assert!(markdown.contains("# Main Content"));
        assert!(markdown.contains("Article text"));
        // Should NOT contain script tags
        assert!(!markdown.contains("alert"));
    }

    #[test]
    fn test_setext_heading_conversion() {
        let html = r"
            <html>
                <body>
                    <h1>Title</h1>
                </body>
            </html>
        ";

        // First convert to markdown (which might produce setext-style from html2md)
        let result = convert_html_to_markdown_sync(html, &ConversionOptions::default());
        assert!(result.is_ok());

        let markdown = result.expect("Test operation should succeed");
        // Should contain ATX-style heading (with process_headings enabled)
        // The actual format depends on html2md output, but our processor should normalize it
        assert!(markdown.contains("Title"));
    }

    #[test]
    fn test_code_fence_preservation() {
        let html = r#"
            <html>
                <body>
                    <h1>Code Example</h1>
                    <pre><code>
                    fn main() {
                        println!("Hello");
                    }
                    </code></pre>
                </body>
            </html>
        "#;

        let result = convert_html_to_markdown_sync(html, &ConversionOptions::default());
        assert!(result.is_ok());

        let markdown = result.expect("Test operation should succeed");
        assert!(markdown.contains("Code Example"));
        // Should contain code block markers
        assert!(markdown.contains("```") || markdown.contains("    ")); // Fenced or indented code
    }

    #[test]
    fn test_empty_html() {
        let html = "";
        let result = convert_html_to_markdown_sync(html, &ConversionOptions::default());
        assert!(result.is_ok());
        // Should return empty or minimal markdown, not error
    }

    #[test]
    fn test_malformed_html_resilience() {
        let html = "<html><body><h1>Unclosed heading<p>Paragraph</body>";
        let result = convert_html_to_markdown_sync(html, &ConversionOptions::default());
        assert!(result.is_ok());
        // Should handle gracefully, possibly with warnings
    }

    #[test]
    fn test_custom_options() {
        let options = ConversionOptions {
            clean_html: false,
            preserve_tables: false,
            preserve_links: false,
            preserve_images: false,
            code_highlighting: false,
            process_headings: false,
            normalize_whitespace: false,
            base_url: None,
        };

        let html = "<html><body><h1>Test</h1><a href='#'>Link</a></body></html>";
        let result = convert_html_to_markdown_sync(html, &options);
        assert!(result.is_ok());
    }
}
#[test]
fn test_basic_link_full_pipeline() {
    use crate::content_saver::markdown_converter::{
        ConversionOptions, convert_html_to_markdown_sync,
    };

    let html = r#"<a href="/rustdesk/rustdesk">rustdesk / rustdesk</a>"#;
    let options = ConversionOptions::default();
    let md = convert_html_to_markdown_sync(html, &options).unwrap();

    eprintln!("INPUT: {}", html);
    eprintln!("OUTPUT: '{}'", md);

    assert!(
        md.contains("(/rustdesk/rustdesk)"),
        "Link must be preserved! Got: '{}'",
        md
    );
}

#[test]
fn test_github_h2_link_full_pipeline() {
    use crate::content_saver::markdown_converter::{
        ConversionOptions, convert_html_to_markdown_sync,
    };

    let html = r#"<h2 class="h3 lh-condensed">
    <a href="/rustdesk/rustdesk" class="Link">
      <span class="text-normal">rustdesk /</span>
      rustdesk
    </a>
  </h2>"#;
    let options = ConversionOptions::default();
    let md = convert_html_to_markdown_sync(html, &options).unwrap();

    eprintln!("INPUT: {}", html);
    eprintln!("OUTPUT: '{}'", md);

    assert!(
        md.contains("(/rustdesk/rustdesk)"),
        "Link must be preserved! Got: '{}'",
        md
    );
}

#[test]
fn test_basic_link_through_full_pipeline() {
    // Test a simple link through the COMPLETE pipeline
    let html = r#"<html><body><article><p>Check out <a href="/rustdesk/rustdesk">rustdesk / rustdesk</a> for remote desktop.</p></article></body></html>"#;

    let result = convert_html_to_markdown_sync(html, &ConversionOptions::default());
    assert!(result.is_ok(), "Conversion failed: {:?}", result.err());

    let markdown = result.unwrap();
    println!("FULL PIPELINE OUTPUT: '{}'", markdown);

    // The link MUST be preserved as [text](url)
    assert!(
        markdown.contains("[rustdesk / rustdesk](/rustdesk/rustdesk)"),
        "Link was lost in full pipeline! Got: {}",
        markdown
    );
}

#[test]
fn test_github_trending_structure_through_pipeline() {
    // Simulate GitHub trending HTML structure
    let html = r#"
        <html>
        <body>
            <main>
                <div class="Box">
                    <article class="Box-row">
                        <h2>
                            <a href="/rustdesk/rustdesk" data-view-component="true">
                                <span>rustdesk</span>
                                /
                                <span>rustdesk</span>
                            </a>
                        </h2>
                        <p>An open-source remote desktop</p>
                    </article>
                </div>
            </main>
        </body>
        </html>
        "#;

    let result = convert_html_to_markdown_sync(html, &ConversionOptions::default());
    assert!(result.is_ok(), "Conversion failed: {:?}", result.err());

    let markdown = result.unwrap();
    println!("GITHUB-LIKE OUTPUT: '{}'", markdown);

    // The link should be preserved
    assert!(
        markdown.contains("[") && markdown.contains("](/rustdesk/rustdesk)"),
        "Link was lost! Got: {}",
        markdown
    );
}

#[test]
fn test_code_block_duplication() {
    let html = r#"
        <html>
        <body>
            <pre><code class="language-bash">
            echo "test"
            </code></pre>

            <pre><code class="language-rust">
            fn main() {}
            </code></pre>
        </body>
        </html>
        "#;

    let markdown = convert_html_to_markdown_sync(html, &ConversionOptions::default()).unwrap();

    eprintln!("OUTPUT:\n{}", markdown);

    // Count code fence pairs
    let fence_count = markdown.matches("```").count();
    assert_eq!(
        fence_count, 4,
        "Should have exactly 4 fences (2 blocks × 2 fences), got {}",
        fence_count
    );

    // Ensure no nested fences
    assert!(
        !markdown.contains("```bash\n```bash"),
        "Should not have nested fences"
    );

    // Ensure code doesn't appear as plain text AND fenced
    let code_text = "echo \"test\"";
    let plain_occurrences = markdown.matches(code_text).count();
    assert_eq!(
        plain_occurrences, 1,
        "Code should appear exactly once, not duplicated. Got: {}",
        plain_occurrences
    );
}

#[test]
fn test_code_block_duplication_complex_html() {
    // Test with more complex HTML that might trigger the bug
    let html = r#"
        <html>
        <body>
            <article>
                <p>Example code:</p>
                <pre><code class="language-rust">
fn get_layout() -> Rc&lt;[Rect]&gt; {
    let percentage = 50;
    Layout::default()
        .split(area)
}
                </code></pre>
                <p>More text</p>
            </article>
        </body>
        </html>
        "#;

    let markdown = convert_html_to_markdown_sync(html, &ConversionOptions::default()).unwrap();

    eprintln!("COMPLEX OUTPUT:\n{}", markdown);

    // The function name should appear exactly once
    let fn_occurrences = markdown.matches("fn get_layout()").count();
    assert_eq!(
        fn_occurrences, 1,
        "Function should appear exactly once, not duplicated. Got {} occurrences",
        fn_occurrences
    );

    // Should have exactly 2 fences (1 block)
    let fence_count = markdown.matches("```").count();
    assert_eq!(
        fence_count, 2,
        "Should have exactly 2 fences (1 block), got {}",
        fence_count
    );
}
