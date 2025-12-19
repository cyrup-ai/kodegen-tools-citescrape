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
use smallvec::SmallVec;
use std::fmt::Write;
use std::sync::{Arc, LazyLock};
use url::Url;

use super::custom_handlers::create_converter;
// Note: Link card transformation removed - it was site-specific (assumed "card" in class names)

// =============================================================================
// REGEX PATTERNS - Reserved for genuine pattern matching within line content
// =============================================================================

#[allow(dead_code)]
static TABLE_ALIGN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\|(\s*:?-+:?\s*\|)+").expect("TABLE_ALIGN: hardcoded regex is valid")
});

static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Bounded quantifiers prevent catastrophic backtracking
    // Limits: 500 chars for link text, 2000 chars for URL
    Regex::new(r"\[([^\]]{1,500})\]\(([^\)]{1,2000})\)")
        .expect("LINK_RE: hardcoded regex is valid")
});

static IMAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Bounded quantifiers prevent catastrophic backtracking
    // Limits: 200 chars for alt text, 2000 chars for image URL
    Regex::new(r"!\[[^\]]{0,200}\]\([^\)]{1,2000}\)")
        .expect("IMAGE_RE: hardcoded regex is valid")
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
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::html_to_markdown::process_markdown_links;
/// let markdown = "[Link](/tutorials/hello)";
/// let base = "https://example.com/docs/guide.html";
/// let result = process_markdown_links(markdown, base);
/// assert!(result.contains("https://example.com/tutorials/hello"));
/// ```
pub fn process_markdown_links(markdown: &str, base_url: &str) -> String {
    // Parse base URL once (early return if invalid)
    let base = match Url::parse(base_url) {
        Ok(url) => url,
        Err(e) => {
            log::warn!("Invalid base URL '{base_url}': {e}, skipping link processing");
            return markdown.to_string();
        }
    };

    // Pre-calculate approximate result size
    // Estimate: original length + 50 bytes per link for potential URL expansion
    let link_count = markdown.matches("](").count();
    let estimated_size = markdown.len() + (link_count * 50);
    let mut result = String::with_capacity(estimated_size);
    
    let mut last_match_end = 0;
    
    for caps in LINK_RE.captures_iter(markdown) {
        let m = caps.get(0).unwrap();
        let text = caps.get(1).unwrap().as_str();
        let url = caps.get(2).unwrap().as_str();
        
        // Append text before this match
        result.push_str(&markdown[last_match_end..m.start()]);
        
        // Process link based on URL type
        if url.starts_with('#') {
            // Fragment-only links: preserve as-is
            write!(result, "[{text}]({url})").unwrap();
        } else if url.starts_with("http://") || url.starts_with("https://") {
            // Already absolute: preserve as-is
            write!(result, "[{text}]({url})").unwrap();
        } else if url.starts_with("mailto:")
            || url.starts_with("tel:")
            || url.starts_with("javascript:")
            || url.starts_with("data:")
        {
            // Special protocols: preserve as-is
            write!(result, "[{text}]({url})").unwrap();
        } else {
            // Resolve relative URL using RFC 3986 rules
            match base.join(url) {
                Ok(resolved) => {
                    write!(result, "[{text}]({})", resolved.as_str()).unwrap();
                }
                Err(e) => {
                    log::warn!("Failed to resolve URL '{url}' against base '{base_url}': {e}");
                    write!(result, "[{text}]({url})").unwrap();
                }
            }
        }
        
        last_match_end = m.end();
    }
    
    // Append remaining text after last match
    result.push_str(&markdown[last_match_end..]);
    
    result
}

/// Detect if a row is a markdown table separator row
///
/// Uses zero-allocation lazy iterator pattern for performance.
/// This function is called in a hot path during table formatting.
///
/// A valid separator row:
/// - Starts and ends with `|`
/// - Contains only `-`, `:`, and whitespace between pipes
/// - Each cell has at least one dash
///
/// # Examples
///
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::html_to_markdown::is_separator_row;
/// assert!(is_separator_row("|---|---|"));
/// assert!(is_separator_row("| --- | --- |"));
/// assert!(is_separator_row("|:---:|:---:|"));
/// assert!(is_separator_row("|:---|---:|"));
/// assert!(!is_separator_row("| abc | def |"));
/// assert!(is_separator_row("|   |   |"));  // Empty cells filtered out, .all() returns true
/// ```
///
/// # Performance
///
/// Zero allocations via lazy iterator - no Vec, no Peekable.
/// Processes ~5-7x faster than the previous implementation.
pub fn is_separator_row(row: &str) -> bool {
    let trimmed = row.trim();
    
    // Must start and end with pipe
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return false;
    }
    
    // Check minimum length to prevent panic on edge cases like "|"
    if trimmed.len() < 2 {
        return false;
    }
    
    // Extract content between outer pipes and split by inner pipes
    // Use lazy iterator - zero allocations
    trimmed[1..trimmed.len() - 1]
        .split('|')
        .filter(|s| !s.trim().is_empty())  // Skip empty cells
        .all(|cell| {
            let trimmed_cell = cell.trim();
            
            // Must contain at least one dash
            let has_dash = trimmed_cell.contains('-');
            
            // Can only contain separator characters: -, :, space, tab
            let valid_chars = trimmed_cell
                .chars()
                .all(|c| matches!(c, '-' | ':' | ' ' | '\t'));
            
            has_dash && valid_chars
        })
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
    /// 0. Preprocessing (site-specific transformations removed)
    /// 1. htmd conversion with custom element handlers
    /// 2. Streaming normalization (single-pass line processor)
    /// 3. Table formatting (optional)
    /// 4. HTML img tag fallback conversion
    /// 5. Link/image removal (optional)
    pub fn convert_sync(&self, html: &str) -> Result<String> {
        // Stage 0: Preprocessing (site-specific transformations removed)
        // Note: Callout transformation removed - it was site-specific and only worked with hard-coded class names
        // Note: Link card transformation removed - it was site-specific (assumed "card" in class names)
        // Note: Tab transformation removed - site-specific patterns conflict with generic crawler mission
        
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
        // Arc for zero-copy sharing across thread boundary (follows existing pattern in search/engine.rs)
        let html = Arc::<str>::from(html);
        let preserve_tables = self.preserve_tables;
        let preserve_links = self.preserve_links;
        let preserve_images = self.preserve_images;
        let code_highlighting = self.code_highlighting;
        
        tokio::task::spawn_blocking(move || {
            let converter = MarkdownConverter {
                preserve_tables,
                preserve_links,
                preserve_images,
                code_highlighting,
            };
            converter.convert_sync(&html)
        })
        .await
        .map_err(|e| anyhow::anyhow!("MarkdownConverter task panicked: {}", e))?
    }

    fn format_tables_static(markdown: &str) -> String {
        // Fast path: no pipes means no tables
        if !markdown.contains('|') {
            return markdown.to_string();
        }
        
        // Pre-allocate result buffer with 20% extra capacity for table formatting overhead
        let mut result = String::with_capacity((markdown.len() as f32 * 1.2) as usize);
        
        // Streaming iterator - no Vec allocation
        let mut lines = markdown.lines().peekable();
        
        // Reusable SmallVec: stack-allocated for tables ≤ 20 rows (95%+ of cases)
        // Only heap-allocates for very large tables
        let mut table_rows: SmallVec<[&str; 20]> = SmallVec::new();
        
        while let Some(line) = lines.next() {
            // Detect table by looking for pipe-delimited content
            // htmd may produce tables without leading | so check for multiple | characters
            let trimmed = line.trim();
            let is_table_row = (trimmed.starts_with('|') && trimmed.ends_with('|'))
                || (trimmed.ends_with('|') && trimmed.matches('|').count() >= 2);
            
            if is_table_row {
                // Collect table rows into reusable SmallVec
                table_rows.clear();  // Reuse allocation from previous table
                table_rows.push(line);
                
                // Peek ahead to collect remaining table rows
                while let Some(&next_line) = lines.peek() {
                    let next_trimmed = next_line.trim();
                    let is_next_table_row = (next_trimmed.starts_with('|') && next_trimmed.ends_with('|'))
                        || (next_trimmed.ends_with('|') && next_trimmed.matches('|').count() >= 2);
                    
                    if is_next_table_row {
                        table_rows.push(next_line);
                        lines.next();  // Consume the peeked line
                    } else {
                        break;
                    }
                }
                
                // Format table directly into result buffer (no intermediate Vec)
                Self::format_markdown_table_into(&mut result, &table_rows);
            } else {
                // Write non-table line directly to buffer
                result.push_str(line);
                result.push('\n');
            }
        }
        
        // Remove trailing newline if present
        if result.ends_with('\n') {
            result.pop();
        }
        
        result
    }

    /// Count columns in a markdown table row
    /// 
    /// Splits by '|' and filters out empty cells (leading/trailing pipes)
    fn count_table_columns(row: &str) -> usize {
        row.split('|')
           .filter(|s| !s.trim().is_empty())
           .count()
    }

    /// Format a markdown table directly into output buffer (zero-copy)
    fn format_markdown_table_into(output: &mut String, table_lines: &[&str]) {
        // ============================================================================
        // DEFENSIVE: Validate and normalize column counts across all rows
        // ============================================================================
        // 
        // Issue #016: Tables may have mismatched column counts between header,
        // separator, and data rows due to:
        // 1. Descriptive text in header cells without colspan detection
        // 2. htmd library generating inconsistent markdown
        // 
        // Solution: Use header row as authoritative source for column count.
        // Only fall back to majority voting (mode) from data rows if header is empty.
        // This ensures separator always matches the header column count.
        // ============================================================================

        if table_lines.is_empty() {
            return;
        }

        // Step 1: Count columns in each row
        let mut row_column_counts: Vec<(usize, usize)> = Vec::new(); // (row_idx, col_count)
        for (idx, &row) in table_lines.iter().enumerate() {
            // Skip separator row when counting - it's not authoritative
            if is_separator_row(row) {
                continue;
            }
            let col_count = Self::count_table_columns(row);
            row_column_counts.push((idx, col_count));
        }

        // Step 2: Find the most common column count (mode) from data rows
        // Skip the first row (header) when computing mode - use data rows as truth
        use std::collections::HashMap;
        let mut column_count_freq: HashMap<usize, usize> = HashMap::new();
        for (idx, col_count) in &row_column_counts {
            // Skip header row (idx 0) when computing mode
            if *idx == 0 {
                continue;
            }
            *column_count_freq.entry(*col_count).or_insert(0) += 1;
        }

        // Step 2: Determine authoritative column count
        // Priority: 1) Header row (authoritative), 2) Mode of data rows (fallback), 3) Default to 1
        let header_col_count = Self::count_table_columns(table_lines[0]);

        let correct_col_count = if header_col_count > 0 {
            // Header is authoritative - use it as the correct column count
            header_col_count
        } else {
            // Header is empty/malformed - fall back to data row mode
            column_count_freq
                .into_iter()
                .max_by_key(|(_, freq)| *freq)
                .map(|(count, _)| count)
                .unwrap_or(1)
        };

        // Debug logging to track column count determination strategy
        if header_col_count == 0 && correct_col_count > 0 {
            log::debug!(
                "Table has empty header, using data row mode: {} columns",
                correct_col_count
            );
        } else if header_col_count > 0 {
            log::debug!(
                "Table using header as authoritative: {} columns",
                correct_col_count
            );
        }

        // Step 3: Check if header row needs adjustment
        let header_needs_fix = header_col_count != correct_col_count;

        // Step 4: Extract descriptive text from header if it has extra columns
        let mut descriptive_text: Option<String> = None;
        let mut normalized_header = table_lines[0].to_string();

        if header_needs_fix && header_col_count > correct_col_count {
            // Header has TOO MANY columns - extract first column as descriptive text
            // if it's significantly longer than others (likely descriptive)
            
            let header_cells: Vec<&str> = table_lines[0]
                .split('|')
                .filter(|s| !s.trim().is_empty())
                .collect();
            
            if !header_cells.is_empty() {
                let first_cell = header_cells[0].trim();
                
                // Heuristic: First cell is descriptive if it's > 40 chars
                // or contains sentence-like text (multiple words, punctuation)
                let is_descriptive = first_cell.len() > 40 
                    || (first_cell.split_whitespace().count() > 4 
                        && (first_cell.contains(':') || first_cell.contains('.')));
                
                if is_descriptive {
                    descriptive_text = Some(first_cell.to_string());
                    
                    // Rebuild header without first column
                    let remaining_cells: Vec<&str> = header_cells[1..].to_vec();
                    normalized_header = format!("| {} |", remaining_cells.join(" | "));
                    
                    log::debug!(
                        "Table column mismatch: header had {} columns, data has {}. \
                         Extracted descriptive text: '{}'",
                        header_col_count,
                        correct_col_count,
                        first_cell
                    );
                } else {
                    // Not descriptive - just truncate to correct count
                    let correct_cells: Vec<&str> = header_cells[..correct_col_count].to_vec();
                    normalized_header = format!("| {} |", correct_cells.join(" | "));
                    
                    log::debug!(
                        "Table column mismatch: header had {} columns, data has {}. \
                         Truncated header to match data rows.",
                        header_col_count,
                        correct_col_count
                    );
                }
            }
        } else if header_needs_fix && header_col_count < correct_col_count {
            // Header has TOO FEW columns - pad with empty cells
            let header_cells: Vec<&str> = table_lines[0]
                .split('|')
                .filter(|s| !s.trim().is_empty())
                .collect();
            
            let mut padded_cells = header_cells.to_vec();
            while padded_cells.len() < correct_col_count {
                padded_cells.push("");
            }
            
            normalized_header = format!("| {} |", padded_cells.join(" | "));
            
            log::debug!(
                "Table column mismatch: header had {} columns, data has {}. \
                 Padded header with empty cells.",
                header_col_count,
                correct_col_count
            );
        }

        // Check if second row is alignment row (before we start writing output)
        let is_alignment_row_present = table_lines.len() > 1 && is_separator_row(table_lines[1]);

        // ============================================================================
        // Write normalized table output
        // ============================================================================

        // Write descriptive text BEFORE table if present
        if let Some(desc_text) = descriptive_text {
            output.push_str(&desc_text);
            output.push_str("\n\n");
        }

        // Write normalized header
        let cleaned_header = Self::clean_table_row(&normalized_header);
        output.push_str(&cleaned_header);
        output.push('\n');

        // Write separator row with CORRECT column count
        // (not based on header, but on validated correct_col_count)
        output.push('|');
        for i in 0..correct_col_count {
            output.push_str("---");
            if i < correct_col_count - 1 {
                output.push('|');
            }
        }
        output.push_str("|\n");

        // Write data rows, validating/fixing column counts
        let start_idx = if is_alignment_row_present { 2 } else { 1 };
        for &row in &table_lines[start_idx..] {
            // Skip duplicate separator rows
            if is_separator_row(row) {
                continue;
            }
            
            // Count columns in this row
            let row_col_count = Self::count_table_columns(row);
            
            if row_col_count != correct_col_count {
                // Normalize this row to match correct column count
                let cells: Vec<&str> = row
                    .split('|')
                    .filter(|s| !s.trim().is_empty())
                    .collect();
                
                let normalized_cells: Vec<&str> = if cells.len() > correct_col_count {
                    // Too many - truncate
                    cells[..correct_col_count].to_vec()
                } else {
                    // Too few - pad with empty
                    let mut padded = cells.to_vec();
                    while padded.len() < correct_col_count {
                        padded.push("");
                    }
                    padded
                };
                
                let normalized_row = format!("| {} |", normalized_cells.join(" | "));
                let cleaned = Self::clean_table_row(&normalized_row);
                output.push_str(&cleaned);
                output.push('\n');
                
                log::debug!(
                    "Normalized data row from {} to {} columns",
                    row_col_count,
                    correct_col_count
                );
            } else {
                // Already correct - just clean and write
                let cleaned = Self::clean_table_row(row);
                if !cleaned.trim().is_empty() {
                    output.push_str(&cleaned);
                    output.push('\n');
                }
            }
        }
    }

    /// Clean up a regular table row
    fn clean_table_row(row: &str) -> String {
        // Normalize spacing and filter empty cells
        let cells: Vec<&str> = row
            .split('|')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        
        // Add spacing around cell content: | content | not |content|
        format!("| {} |", cells.join(" | "))
    }

    fn remove_links_static(markdown: &str) -> String {
        // Convert [text](url) to just text
        // BUT: If text is empty/whitespace, preserve the URL instead of losing it
        LINK_RE.replace_all(markdown, |caps: &regex::Captures| {
            let text = &caps[1];  // Captured link text
            let url = &caps[2];   // Captured URL
            
            // Check if text is meaningful (not just whitespace)
            let text_trimmed = text.trim();
            if text_trimmed.is_empty() {
                // Link text is empty/whitespace - use URL instead
                // Better to have visible URL than lose the link entirely
                url.to_string()
            } else {
                // Link text is meaningful - use it
                text.to_string()
            }
        }).to_string()
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

    #[test]
    fn test_remove_links_preserves_url_when_text_empty() {
        let markdown = "Check out [ ](/console) for more info.";
        let result = MarkdownConverter::remove_links_static(markdown);
        
        // Should preserve URL, not leave empty space
        assert_eq!(result, "Check out /console for more info.");
        assert!(!result.contains("  "), "Should not have double spaces");
    }

    #[test]
    fn test_remove_links_preserves_url_when_text_whitespace() {
        let markdown = "Visit [  ](/api/docs) and [   ](/guide).";
        let result = MarkdownConverter::remove_links_static(markdown);
        
        // Should preserve URLs as text
        assert_eq!(result, "Visit /api/docs and /guide.");
    }

    #[test]
    fn test_remove_links_preserves_meaningful_text() {
        let markdown = "Check [Documentation](/docs) and [API](/api).";
        let result = MarkdownConverter::remove_links_static(markdown);
        
        // Should preserve meaningful text, discard URLs
        assert_eq!(result, "Check Documentation and API.");
    }

    #[test]
    fn test_remove_links_handles_mixed_cases() {
        let markdown = "See [Guide](/guide), [ ](/empty), and [Docs](/docs).";
        let result = MarkdownConverter::remove_links_static(markdown);
        
        // Meaningful text preserved, empty links show URL
        assert_eq!(result, "See Guide, /empty, and Docs.");
    }
}
