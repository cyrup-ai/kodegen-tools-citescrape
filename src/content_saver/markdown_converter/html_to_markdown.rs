//! HTML to Markdown conversion functionality.
//!
//! This module wraps the html2md library and adds additional post-processing
//! to produce clean, well-formatted markdown output.

use anyhow::Result;
use html2md;
use regex::Regex;
use std::sync::LazyLock;
use url::Url;

use super::custom_handlers::create_custom_handlers;

// Compile regex patterns once at first use
// These are syntactically valid hardcoded patterns - if they fail, it's a compile-time bug
static EMPTY_LINES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\n{3,}")
        .expect("SAFETY: hardcoded regex r\"\\n{3,}\" is statically valid")
});

static SPACE_AFTER_LIST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(\s*[-*+])\s*").expect(
        "SAFETY: hardcoded regex r\"(?m)^(\\s*[-*+])\\s*\" is statically valid",
    )
});

static HEADING_SPACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(#+)([^ ])")
        .expect("SAFETY: hardcoded regex r\"(?m)^(#+)([^ ])\" is statically valid")
});

static TABLE_ALIGN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\|(\s*:?-+:?\s*\|)+").expect(
        "SAFETY: hardcoded regex r\"\\|(\\s*:?-+:?\\s*\\|)+\" is statically valid",
    )
});

static CODE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"```([a-zA-Z]*)\n").expect(
        "SAFETY: hardcoded regex r\"```([a-zA-Z]*)\\n\" is statically valid",
    )
});

static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[([^\]]+)\]\([^\)]+\)")
        .expect("SAFETY: hardcoded regex r\"\\[([^\\]]+)\\]\\([^\\)]+\\)\" is statically valid")
});

static IMAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\[[^\]]*\]\([^\)]+\)")
        .expect("SAFETY: hardcoded regex r\"!\\[[^\\]]*\\]\\([^\\)]+\\)\" is statically valid")
});

/// Matches remaining HTML img tags that html2md failed to convert
/// 
/// Captures:
/// - Group 1: src URL (required)
/// - Group 2: alt text (optional, may not exist)
/// 
/// The pattern handles all three cases:
/// - `<img src="url" alt="text">` → captures both
/// - `<img alt="text" src="url">` → captures both  
/// - `<img src="url">` → captures only src
static HTML_IMG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<img[^>]*?\ssrc="([^"]+)"(?:[^>]*?\salt="([^"]*)")?[^>]*?>"#)
        .expect("HTML_IMG_RE: hardcoded regex is valid")
});

/// Process markdown links by converting relative URLs to absolute URLs
///
/// This function uses the `LINK_RE` regex to find all markdown links `[text](url)`
/// and applies RFC 3986 URL resolution to convert relative URLs to absolute URLs.
///
/// # URL Resolution Rules
///
/// - **Fragment-only** (`#section`): Preserved as-is
/// - **Absolute URLs** (`https://...`, `http://...`): Preserved as-is  
/// - **Root-relative** (`/path`): Resolved against base URL's origin
/// - **Relative** (`../path`, `./path`, `path`): Resolved against base URL
///
/// # Arguments
///
/// * `markdown` - Markdown content containing links
/// * `base_url` - Base URL for resolving relative links
///
/// # Returns
///
/// Markdown with all relative URLs converted to absolute URLs
///
/// # Examples
///
/// ```
/// let markdown = "[Link](/tutorials/hello)";
/// let base = "https://example.com/docs/guide.html";
/// let result = process_markdown_links(markdown, base);
/// // result: "[Link](https://example.com/tutorials/hello)"
/// ```
pub(crate) fn process_markdown_links(markdown: &str, base_url: &str) -> String {
    // Parse base URL once (early return if invalid)
    let base = match Url::parse(base_url) {
        Ok(url) => url,
        Err(e) => {
            log::warn!("Invalid base URL '{base_url}': {e}, skipping link processing");
            return markdown.to_string();
        }
    };

    LINK_RE
        .replace_all(markdown, |caps: &regex::Captures| {
            let text = &caps[1];
            let url = &caps[2];

            // Fragment-only links: preserve as-is
            if url.starts_with('#') {
                return format!("[{text}]({url})");
            }

            // Already absolute: preserve as-is
            if url.starts_with("http://") || url.starts_with("https://") {
                return format!("[{text}]({url})");
            }

            // Special protocols: preserve as-is
            if url.starts_with("mailto:")
                || url.starts_with("tel:")
                || url.starts_with("javascript:")
                || url.starts_with("data:")
            {
                return format!("[{text}]({url})");
            }

            // Resolve relative URL using RFC 3986 rules
            match base.join(url) {
                Ok(resolved) => format!("[{text}]({})", resolved.as_str()),
                Err(e) => {
                    log::warn!("Failed to resolve URL '{url}' against base '{base_url}': {e}");
                    // Fallback: preserve original URL
                    format!("[{text}]({url})")
                }
            }
        })
        .to_string()
}

/// HTML to Markdown converter with configurable options
pub struct MarkdownConverter {
    preserve_tables: bool,
    preserve_links: bool,
    preserve_images: bool,
    code_highlighting: bool,
}

impl Default for MarkdownConverter {
    fn default() -> Self {
        Self {
            preserve_tables: true,
            preserve_links: true,
            preserve_images: true,
            code_highlighting: true,
        }
    }
}

impl MarkdownConverter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_preserve_tables(mut self, preserve: bool) -> Self {
        self.preserve_tables = preserve;
        self
    }

    #[must_use]
    pub fn with_preserve_links(mut self, preserve: bool) -> Self {
        self.preserve_links = preserve;
        self
    }

    #[must_use]
    pub fn with_preserve_images(mut self, preserve: bool) -> Self {
        self.preserve_images = preserve;
        self
    }

    #[must_use]
    pub fn with_code_highlighting(mut self, highlight: bool) -> Self {
        self.code_highlighting = highlight;
        self
    }

    /// Convert HTML to Markdown synchronously (with built-in fallback)
    pub fn convert_sync(&self, html: &str) -> Result<String> {
        // First pass: Convert HTML to markdown with custom handlers
        let custom_handlers = create_custom_handlers();
        let mut markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Clean up the markdown
        markdown = Self::clean_markdown_static(&markdown);

        // Handle code blocks
        if self.code_highlighting {
            markdown = CODE_BLOCK.replace_all(&markdown, "```$1\n").to_string();
        }

        // Clean up lists
        markdown = SPACE_AFTER_LIST.replace_all(&markdown, "$1 ").to_string();

        // Fix heading spacing
        markdown = HEADING_SPACE.replace_all(&markdown, "$1 $2").to_string();

        // Handle tables if enabled
        if self.preserve_tables {
            markdown = Self::format_tables_static(&markdown);
        }

        // Remove excessive newlines
        markdown = EMPTY_LINES.replace_all(&markdown, "\n\n").to_string();

        // Convert any remaining HTML img tags that html2md failed to convert
        markdown = convert_remaining_html_images(&markdown);

        // Handle links and images based on settings
        if !self.preserve_links {
            markdown = Self::remove_links_static(&markdown);
        }
        if !self.preserve_images {
            markdown = Self::remove_images_static(&markdown);
        }

        Ok(markdown.trim().to_string())
    }

    /// Convert HTML to Markdown asynchronously
    ///
    /// Performs the same conversion as `convert_sync()` but in an async context.
    /// Since the work is CPU-bound, this simply calls the sync version.
    ///
    /// # Arguments
    ///
    /// * `html` - Raw HTML content to convert
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Converted markdown
    /// * `Err(anyhow::Error)` - Conversion error
    pub async fn convert(&self, html: &str) -> Result<String> {
        self.convert_sync(html)
    }

    fn clean_markdown_static(markdown: &str) -> String {
        let mut cleaned = markdown.to_string();

        // Remove HTML comments
        cleaned = cleaned
            .lines()
            .filter(|line| !line.trim_start().starts_with("<!--"))
            .collect::<Vec<_>>()
            .join("\n");

        // Fix list indentation
        cleaned = cleaned
            .lines()
            .map(|line| {
                if line.trim_start().starts_with(['-', '*', '+']) {
                    let indent = line.chars().take_while(|c| c.is_whitespace()).count();
                    format!("{}{}", " ".repeat(indent), line.trim_start())
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        cleaned
    }

    fn format_tables_static(markdown: &str) -> String {
        let lines: Vec<&str> = markdown.lines().collect();
        let mut result = Vec::new();
        let mut i = 0;
        
        while i < lines.len() {
            let line = lines[i];
            
            // Detect table by looking for pipe-delimited content
            if line.trim().starts_with('|') && line.trim().ends_with('|') {
                // Collect the entire table
                let mut table_lines = vec![line];
                
                i += 1;
                while i < lines.len() {
                    let next_line = lines[i];
                    if next_line.trim().starts_with('|') && next_line.trim().ends_with('|') {
                        table_lines.push(next_line);
                        i += 1;
                    } else {
                        break;
                    }
                }
                
                // Process and format the table
                let formatted_table = Self::format_markdown_table(&table_lines);
                result.extend(formatted_table);
                
                continue;
            }
            
            result.push(line.to_string());
            i += 1;
        }
        
        result.join("\n")
    }

    /// Format a markdown table with proper alignment and spacing
    fn format_markdown_table(table_lines: &[&str]) -> Vec<String> {
        if table_lines.is_empty() {
            return vec![];
        }
        
        let mut formatted = Vec::new();
        let mut is_alignment_row_present = false;
        
        // Check if second row is alignment row
        if table_lines.len() > 1 {
            let second = table_lines[1].trim();
            is_alignment_row_present = TABLE_ALIGN.is_match(second);
        }
        
        // Process first row (header)
        formatted.push(table_lines[0].to_string());
        
        // Ensure alignment row exists
        if !is_alignment_row_present && table_lines.len() > 1 {
            // Insert default alignment row
            let col_count = table_lines[0]
                .split('|')
                .filter(|s| !s.trim().is_empty())
                .count();
            let alignment_row = format!("|{}|", vec!["---"; col_count].join("|"));
            formatted.push(alignment_row);
        } else if is_alignment_row_present {
            // Clean up existing alignment row
            let alignment_row = Self::clean_alignment_row(table_lines[1]);
            formatted.push(alignment_row);
        }
        
        // Process remaining rows (skip alignment row if present)
        let start_idx = if is_alignment_row_present { 2 } else { 1 };
        for &row in &table_lines[start_idx..] {
            let cleaned = Self::clean_table_row(row);
            if !cleaned.trim().is_empty() {
                formatted.push(cleaned);
            }
        }
        
        formatted
    }

    /// Clean up alignment row formatting
    fn clean_alignment_row(row: &str) -> String {
        let cells: Vec<&str> = row
            .split('|')
            .filter(|s| !s.trim().is_empty())
            .collect();
        
        let formatted_cells: Vec<String> = cells
            .iter()
            .map(|cell| {
                let trimmed = cell.trim();
                if trimmed.starts_with(':') && trimmed.ends_with(':') {
                    ":---:".to_string()
                } else if trimmed.starts_with(':') {
                    ":---".to_string()
                } else if trimmed.ends_with(':') {
                    "---:".to_string()
                } else {
                    "---".to_string()
                }
            })
            .collect();
        
        format!("|{}|", formatted_cells.join("|"))
    }

    /// Clean up a regular table row
    fn clean_table_row(row: &str) -> String {
        // Normalize spacing
        let cells: Vec<&str> = row.split('|').map(|s| s.trim()).collect();
        
        format!("|{}|", cells.join("|"))
    }

    fn remove_links_static(markdown: &str) -> String {
        // Convert [text](url) to just text
        LINK_RE.replace_all(markdown, "$1").to_string()
    }

    fn remove_images_static(markdown: &str) -> String {
        // Remove ![alt](url) completely
        IMAGE_RE.replace_all(markdown, "").to_string()
    }
}

/// Convert any remaining HTML img tags to markdown syntax
///
/// This is a fallback for images that html2md failed to convert.
/// Uses a single robust pattern with optional alt attribute and safe capture group access.
///
/// # Arguments
/// * `markdown` - Markdown string potentially containing HTML img tags
///
/// # Returns
/// * Markdown with all img tags converted to `![alt](src)` or `![](src)` syntax
///
/// # Implementation Notes
/// - Uses closure-based replacement for safe capture group access
/// - Handles images with or without alt attributes
/// - Never panics on missing groups (uses .map_or for safety)
fn convert_remaining_html_images(markdown: &str) -> String {
    HTML_IMG_RE.replace_all(markdown, |caps: &regex::Captures| {
        // Group 1 (src) is guaranteed to exist because pattern requires it
        let src = caps.get(1)
            .map_or("", |m| m.as_str());
        
        // Group 2 (alt) is optional - use empty string if not present
        let alt = caps.get(2)
            .map_or("", |m| m.as_str());
        
        // Generate markdown: ![alt](src) or ![](src)
        format!("![{alt}]({src})")
    }).into_owned()
}
