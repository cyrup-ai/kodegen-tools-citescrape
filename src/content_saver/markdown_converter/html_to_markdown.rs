//! HTML to Markdown conversion with streaming post-processing.
//!
//! # Architecture
//!
//! The conversion pipeline has two stages:
//!
//! 1. **htmd conversion**: Transforms HTML to markdown using custom element handlers
//! 2. **Streaming normalization**: Single-pass line processor for clean formatting
//!
//! # Design Philosophy
//!
//! Post-processing uses deterministic line-by-line streaming rather than regex.
//! This avoids unintended pattern matches (e.g., `*` in `**bold**` being treated
//! as a list marker) and enables O(n) processing with minimal allocations.
//!
//! Each line is classified by its semantic type, enabling context-aware formatting
//! without complex lookahead/lookbehind patterns that can corrupt inline formatting.

use anyhow::Result;
use regex::Regex;
use std::sync::LazyLock;
use url::Url;

use super::custom_handlers::create_converter;

// =============================================================================
// REGEX PATTERNS - Reserved for genuine pattern matching within line content
// =============================================================================

static TABLE_ALIGN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\|(\s*:?-+:?\s*\|)+").expect("TABLE_ALIGN: hardcoded regex is valid")
});

static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[([^\]]+)\]\(([^\)]+)\)").expect("LINK_RE: hardcoded regex is valid")
});

static IMAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\[[^\]]*\]\([^\)]+\)").expect("IMAGE_RE: hardcoded regex is valid")
});


// =============================================================================
// LINE TYPE CLASSIFICATION
// =============================================================================

/// Semantic classification of a markdown line.
///
/// Enables context-aware formatting decisions without regex pattern matching
/// that could have unintended side effects on inline formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineType {
    Blank,
    Heading,
    UnorderedList,
    OrderedList,
    CodeFence,
    Blockquote,
    TableRow,
    HorizontalRule,
    HtmlComment,
    EmptyListMarkers,
    Paragraph,
}

impl LineType {
    /// Classify a line by its markdown semantics.
    ///
    /// Classification is based on leading characters after accounting for
    /// indentation. The critical distinction for lists is requiring a space
    /// after the marker to avoid matching `**bold**` as a list.
    fn classify(line: &str) -> Self {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            return Self::Blank;
        }

        if trimmed.starts_with("<!--") {
            return Self::HtmlComment;
        }

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            return Self::CodeFence;
        }

        // Heading: # followed by space or EOL, max 6 levels
        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if hash_count <= 6 {
                let rest = &trimmed[hash_count..];
                if rest.is_empty() || rest.starts_with(' ') {
                    return Self::Heading;
                }
            }
        }

        if trimmed.starts_with('>') {
            return Self::Blockquote;
        }

        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            return Self::TableRow;
        }

        if Self::is_horizontal_rule(trimmed) {
            return Self::HorizontalRule;
        }

        if Self::is_empty_list_markers(trimmed) {
            return Self::EmptyListMarkers;
        }

        // Unordered list: -, *, + followed by SPACE
        // Critical: space check prevents matching **bold** as list
        if let Some(first) = trimmed.chars().next()
            && matches!(first, '-' | '*' | '+') && trimmed.chars().nth(1) == Some(' ') {
            return Self::UnorderedList;
        }

        if Self::is_ordered_list(trimmed) {
            return Self::OrderedList;
        }

        Self::Paragraph
    }

    /// Check if line is a horizontal rule (---, ***, ___)
    fn is_horizontal_rule(s: &str) -> bool {
        if s.len() < 3 {
            return false;
        }
        let first = match s.chars().next() {
            Some(c) if matches!(c, '-' | '*' | '_') => c,
            _ => return false,
        };
        let count = s.chars().filter(|&c| c == first).count();
        count >= 3 && s.chars().all(|c| c == first || c == ' ')
    }

    /// Check if line is an ordered list item (1. or 1))
    fn is_ordered_list(s: &str) -> bool {
        let mut chars = s.chars().peekable();
        let mut has_digit = false;
        while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            has_digit = true;
            chars.next();
        }
        has_digit && matches!((chars.next(), chars.next()), (Some('.' | ')'), Some(' ')))
    }

    /// Check if line contains only empty list markers (e.g., "* * *", "* ")
    fn is_empty_list_markers(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        // Single marker check
        if s.len() == 1 {
            return matches!(s.chars().next(), Some('*' | '-' | '+'));
        }
        // Multiple markers separated by whitespace
        let tokens: Vec<&str> = s.split_whitespace().collect();
        !tokens.is_empty()
            && tokens.iter().all(|t| {
                t.len() == 1 && matches!(t.chars().next(), Some('*' | '-' | '+'))
            })
    }

    /// Does this line type require a blank line before it?
    const fn needs_blank_before(self) -> bool {
        matches!(
            self,
            Self::Heading | Self::CodeFence | Self::HorizontalRule
        )
    }
}

// =============================================================================
// STREAMING MARKDOWN NORMALIZER
// =============================================================================

/// Stateful streaming normalizer - single pass, pre-allocated buffer.
///
/// Processes markdown line-by-line, writing directly to an output buffer.
/// Tracks state to handle:
/// - Consecutive blank line collapsing (max 2)
/// - Code fence passthrough (no processing inside fences)
/// - Block element spacing (blank lines before headings, etc.)
/// - Heading normalization (`##Text` → `## Text`)
struct MarkdownNormalizer {
    output: String,
    prev_type: LineType,
    consecutive_blanks: u8,
    in_code_fence: bool,
}

impl MarkdownNormalizer {
    /// Normalize markdown in a single pass with pre-allocated buffer.
    fn normalize(input: &str) -> String {
        let mut this = Self {
            output: String::with_capacity(input.len()),
            prev_type: LineType::Blank,
            consecutive_blanks: 0,
            in_code_fence: false,
        };

        for line in input.lines() {
            this.emit(line);
        }

        this.output
    }

    /// Process and emit a single line.
    fn emit(&mut self, line: &str) {
        // Inside code fence: pass through verbatim
        if self.in_code_fence {
            if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                self.in_code_fence = false;
            }
            self.write_line(line);
            return;
        }

        let line_type = LineType::classify(line);

        // Toggle code fence state on entry
        if line_type == LineType::CodeFence {
            self.in_code_fence = true;
        }

        // Skip HTML comments and empty list markers
        if matches!(line_type, LineType::HtmlComment | LineType::EmptyListMarkers) {
            return;
        }

        // Blank line handling: max 2 consecutive
        if line_type == LineType::Blank {
            self.consecutive_blanks += 1;
            if self.consecutive_blanks <= 2 {
                self.write_line(line);
            }
            self.prev_type = line_type;
            return;
        }

        // Ensure blank line before block elements
        if line_type.needs_blank_before() && self.prev_type != LineType::Blank {
            self.write_line("");
        }

        // Process and emit
        match line_type {
            LineType::Heading => self.write_line(&Self::normalize_heading(line)),
            _ => self.write_line(line),
        }

        self.consecutive_blanks = 0;
        self.prev_type = line_type;
    }

    /// Write a line to the output buffer.
    #[inline]
    fn write_line(&mut self, line: &str) {
        if !self.output.is_empty() {
            self.output.push('\n');
        }
        self.output.push_str(line);
    }

    /// Ensure space after # in headings: `##Text` → `## Text`
    fn normalize_heading(line: &str) -> String {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
        let rest = &trimmed[hash_count..];

        if rest.is_empty() || rest.starts_with(' ') {
            line.to_string()
        } else {
            format!("{}{} {}", indent, "#".repeat(hash_count), rest)
        }
    }
}

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

    /// Convert HTML to Markdown synchronously.
    ///
    /// Pipeline:
    /// 1. htmd conversion with custom element handlers
    /// 2. Streaming normalization (single-pass line processor)
    /// 3. Table formatting (optional)
    /// 4. HTML img tag fallback conversion
    /// 5. Link/image removal (optional)
    pub fn convert_sync(&self, html: &str) -> Result<String> {
        // Stage 1: htmd conversion with custom handlers
        let converter = create_converter();
        let raw_markdown = converter.convert(html)?;

        // Stage 2: Streaming normalization (single pass)
        // Handles: blank line collapsing, heading spacing, code fence passthrough,
        // HTML comment removal, empty list marker removal
        let mut markdown = MarkdownNormalizer::normalize(&raw_markdown);

        // Stage 3: Table formatting (line-based, already efficient)
        if self.preserve_tables {
            markdown = Self::format_tables_static(&markdown);
        }

        // Stage 4: Optional link/image removal
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_converter_bold_text() {
        let converter = MarkdownConverter::new();
        let html = r#"<span data-as="p"><strong>Homebrew (macOS, Linux):</strong></span>"#;
        let result = converter.convert_sync(html).unwrap();

        // Should have proper bold formatting
        assert!(
            result.contains("**Homebrew (macOS, Linux):**"),
            "Should have proper bold. Got: '{}'",
            result
        );
    }

    #[test]
    fn test_process_markdown_links_basic() {
        let markdown = "[Home](/index.html)";
        let base = "https://example.com/docs/guide.html";
        let result = process_markdown_links(markdown, base);
        assert_eq!(result, "[Home](https://example.com/index.html)");
    }

    #[test]
    fn test_process_markdown_links_relative() {
        let markdown = "[Next](./tutorial.html)";
        let base = "https://example.com/docs/guide.html";
        let result = process_markdown_links(markdown, base);
        assert_eq!(result, "[Next](https://example.com/docs/tutorial.html)");
    }

    #[test]
    fn test_process_markdown_links_fragment() {
        let markdown = "[Section](#heading)";
        let base = "https://example.com/page.html";
        let result = process_markdown_links(markdown, base);
        // Fragment-only links should be preserved as-is
        assert_eq!(result, "[Section](#heading)");
    }

    #[test]
    fn test_process_markdown_links_absolute() {
        let markdown = "[External](https://other.com/page)";
        let base = "https://example.com/page.html";
        let result = process_markdown_links(markdown, base);
        // Absolute URLs should be preserved as-is
        assert_eq!(result, "[External](https://other.com/page)");
    }

    #[test]
    fn test_process_markdown_links_multiple() {
        let markdown = "[Home](/) and [About](/about) and [External](https://other.com)";
        let base = "https://example.com/docs/guide.html";
        let result = process_markdown_links(markdown, base);
        assert!(result.contains("[Home](https://example.com/)"));
        assert!(result.contains("[About](https://example.com/about)"));
        assert!(result.contains("[External](https://other.com)"));
    }

    #[test]
    fn test_link_re_captures_both_text_and_url() {
        // This test specifically verifies the regex bug fix:
        // LINK_RE must have 2 capture groups (text and URL)
        let test_link = "[Click here](/path/to/page)";

        let caps = LINK_RE.captures(test_link).expect("Should match");

        // Group 0 is the full match
        assert_eq!(caps.get(0).unwrap().as_str(), "[Click here](/path/to/page)");

        // Group 1 should be the link text
        assert_eq!(caps.get(1).unwrap().as_str(), "Click here");

        // Group 2 should be the URL (this was the bug - group 2 didn't exist)
        assert_eq!(caps.get(2).unwrap().as_str(), "/path/to/page");
    }
}
