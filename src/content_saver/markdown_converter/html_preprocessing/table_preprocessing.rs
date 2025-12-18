//! HTML table preprocessing for robust markdown conversion.
//!
//! This module normalizes HTML tables before markdown conversion by:
//! - Detecting and handling layout tables (convert to plain text)
//! - Expanding colspan/rowspan into explicit cell grids
//! - Ensuring proper thead/tbody structure
//! - Cleaning up problematic table attributes
//!
//! Standard Markdown does not support colspan or rowspan. This module solves
//! that by duplicating cell content across spanned cells, ensuring the resulting
//! Markdown table has correct structure even if content is repeated.

use anyhow::Result;
use htmlentity::entity::{decode, ICodedDataTrait};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;
use std::sync::LazyLock;
use tracing::{debug, warn};

// ============================================================================
// Static Selectors (compiled once at first use)
// ============================================================================

static TABLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("table")
        .expect("BUG: hardcoded selector 'table' is statically valid")
});

static TR_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("tr")
        .expect("BUG: hardcoded selector 'tr' is statically valid")
});

static CELL_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("td, th")
        .expect("BUG: hardcoded selector 'td, th' is statically valid")
});

static TH_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("th")
        .expect("BUG: hardcoded selector 'th' is statically valid")
});

static THEAD_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("thead")
        .expect("BUG: hardcoded selector 'thead' is statically valid")
});

static CAPTION_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("caption")
        .expect("BUG: hardcoded selector 'caption' is statically valid")
});

static TABLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<table[^>]*>.*?</table>")
        .expect("BUG: hardcoded table regex is statically valid")
});

static PRE_TABLE_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(<span[^>]*data-as="p"[^>]*>)(.*?)(</span>)\s*(<(?:div[^>]*>)*\s*<table)"#)
        .expect("BUG: pre-table text regex is valid")
});

// ============================================================================
// Security Limits
// ============================================================================

/// Maximum grid rows to prevent memory exhaustion attacks
///
/// Real-world tables rarely exceed 500 rows. Data tables (financial reports,
/// spreadsheets) typically have < 100 rows. Setting limit at 1000 provides
/// generous headroom while preventing DoS.
const MAX_GRID_ROWS: usize = 1000;

/// Maximum grid columns to prevent memory exhaustion attacks  
///
/// Already enforced via colspan clamp (100). Real tables rarely exceed 20
/// columns. Explicit constant documents security boundary.
const MAX_GRID_COLS: usize = 100;

/// Maximum total cells allowed in a table grid to prevent memory exhaustion.
/// 
/// Limit: 100,000 cells (~8 MB with 80 bytes per GridCell)
/// 
/// This prevents DoS attacks via malicious HTML tables with excessive
/// colspan/rowspan combinations.
const MAX_TOTAL_CELLS: usize = 100_000;

// ============================================================================
// Data Structures
// ============================================================================

/// Cell in the expanded table grid
#[derive(Clone, Debug)]
struct GridCell {
    content: String,
    is_header: bool,
    align: Option<Alignment>,
}

impl GridCell {
    fn empty() -> Self {
        Self {
            content: String::new(),
            is_header: false,
            align: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Alignment {
    Left,
    Center,
    Right,
}

// ============================================================================
// Main Entry Point
// ============================================================================

/// Fix text elements before tables by ensuring proper paragraph wrapping
///
/// This function detects text elements (specifically `<span data-as="p">` elements)
/// that immediately precede tables and wraps them in proper `<p>` tags with appropriate
/// spacing to prevent them from being merged with table headers during markdown conversion.
///
/// # Arguments
/// * `html` - HTML string that may contain tables with preceding text elements
///
/// # Returns
/// * `Ok(String)` - HTML with proper element separation before tables
/// * `Err(anyhow::Error)` - If processing fails
pub fn fix_pre_table_text(html: &str) -> Result<String> {
    let result = PRE_TABLE_TEXT.replace_all(html, |caps: &regex::Captures| {
        let text_content = &caps[2];
        let table_start = &caps[4];
        
        // Convert span to paragraph with double newline before table
        format!("<p>{}</p>\n\n{}", text_content.trim(), table_start)
    });
    
    Ok(result.to_string())
}

/// Inject preceding text elements as table headers
///
/// This function detects text elements (p, h1-h6, div) that immediately precede tables
/// and appear to be table headers based on word count matching column count. It injects
/// these as proper `<thead><tr><th>` elements into the table and removes the preceding
/// text element.
///
/// # Detection Criteria
/// A preceding element is treated as table headers if ALL conditions are met:
/// 1. Table has NO `<thead>` elements AND NO `<th>` elements
/// 2. There is an IMMEDIATE preceding sibling element
/// 3. Element type is `<p>`, `<h1>-<h6>`, or `<div>`
/// 4. Element contains 2-5 words (split on whitespace, filtered for non-empty)
/// 5. Word count EXACTLY matches the table's column count (from first row)
///
/// # Arguments
/// * `html` - HTML string that may contain tables with preceding header text
///
/// # Returns
/// * `Ok(String)` - HTML with headers injected into tables
/// * `Err(anyhow::Error)` - If processing fails
///
/// # Examples
///
/// **Before:**
/// ```html
/// <p>File Purpose</p>
/// <table>
///   <tr><td>~/.claude/settings.json</td><td>User settings...</td></tr>
/// </table>
/// ```
///
/// **After:**
/// ```html
/// <table>
///   <thead><tr><th>File</th><th>Purpose</th></tr></thead>
///   <tbody>
///     <tr><td>~/.claude/settings.json</td><td>User settings...</td></tr>
///   </tbody>
/// </table>
/// ```
pub fn inject_preceding_headers(html: &str) -> Result<String> {
    // Strategy: Parse with scraper to analyze, then use string manipulation for replacement
    // This avoids DOM mutation complexity while leveraging scraper's analysis capabilities
    
    let document = Html::parse_document(html);
    let mut modifications: Vec<(usize, usize, String)> = Vec::new();
    
    // Find all table elements
    for table in document.select(&TABLE_SELECTOR) {
        // Check if table already has headers
        let has_thead = table.select(&THEAD_SELECTOR).next().is_some();
        let has_th = table.select(&TH_SELECTOR).next().is_some();
        
        if has_thead || has_th {
            continue;
        }
        
        // Count table columns (from first row)
        let first_row = table.select(&TR_SELECTOR).next();
        let column_count = match first_row {
            Some(row) => row.select(&CELL_SELECTOR).count(),
            None => continue,
        };
        
        if column_count == 0 {
            continue;
        }
        
        // Try to find preceding text element by searching backwards in HTML
        // Get the table's HTML to find its position
        let table_html = table.html();
        
        // Find where this table appears in the source HTML
        let table_pos = match html.find(&table_html) {
            Some(pos) => pos,
            None => continue,
        };
        
        // Look backwards for a text element
        let html_before = &html[..table_pos];
        
        // Try to match any of: </p>, </h1>, </h2>, </h3>, </h4>, </h5>, </h6>, </div>
        // followed by optional whitespace before the table
        let tag_patterns = ["</p>", "</h1>", "</h2>", "</h3>", "</h4>", "</h5>", "</h6>", "</div>"];
        
        let mut found_preceding: Option<(usize, usize, String)> = None;
        
        for tag_close in &tag_patterns {
            if let Some(close_pos) = html_before.rfind(tag_close) {
                // Check if this is immediately before the table (with optional whitespace)
                let between = &html[close_pos + tag_close.len()..table_pos];
                if !between.trim().is_empty() {
                    continue;
                }
                
                // Extract the tag name
                let tag_name = &tag_close[2..tag_close.len()-1]; // Remove </ and >
                let open_tag = format!("<{}", tag_name);
                
                // Find the opening tag
                let search_start = close_pos.saturating_sub(1000); // Look back up to 1000 chars
                let search_text = &html[search_start..close_pos];
                
                if let Some(rel_open_pos) = search_text.rfind(&open_tag) {
                    let open_pos = search_start + rel_open_pos;
                    
                    // Find where the opening tag ends
                    let tag_content_start = match html[open_pos..].find('>') {
                        Some(pos) => open_pos + pos + 1,
                        None => continue,
                    };
                    
                    // Extract text content
                    let text_content = &html[tag_content_start..close_pos];
                    
                    // Count words
                    let words: Vec<&str> = text_content.split_whitespace()
                        .filter(|w| !w.is_empty())
                        .collect();
                    let word_count = words.len();
                    
                    // Validate word count (2-5 is reasonable for table headers)
                    if !(2..=5).contains(&word_count) {
                        continue;
                    }
                    
                    // Check if word count matches column count
                    if word_count != column_count {
                        continue;
                    }
                    
                    // Build thead HTML
                    let mut thead_html = String::from("<thead><tr>");
                    for word in words {
                        thead_html.push_str("<th>");
                        // Simple HTML escaping
                        let escaped = word
                            .replace('&', "&amp;")
                            .replace('<', "&lt;")
                            .replace('>', "&gt;")
                            .replace('"', "&quot;");
                        thead_html.push_str(&escaped);
                        thead_html.push_str("</th>");
                    }
                    thead_html.push_str("</tr></thead>");
                    
                    // Record modification: remove preceding element, inject thead
                    found_preceding = Some((open_pos, close_pos + tag_close.len(), thead_html));
                    break;
                }
            }
        }
        
        if let Some((remove_start, remove_end, thead_html)) = found_preceding {
            // Find the table opening tag position
            let table_open_end = match html[table_pos..].find('>') {
                Some(pos) => table_pos + pos + 1,
                None => continue,
            };
            
            modifications.push((remove_start, remove_end, String::new())); // Remove preceding
            modifications.push((table_open_end, table_open_end, thead_html)); // Insert thead
        }
    }
    
    // Apply modifications in reverse order to maintain positions
    modifications.sort_by(|a, b| b.0.cmp(&a.0));
    
    let mut result = html.to_string();
    for (start, end, replacement) in modifications {
        result.replace_range(start..end, &replacement);
    }
    
    Ok(result)
}

/// Preprocess all tables in HTML
///
/// This is the main entry point for table preprocessing. It:
/// 1. Uses regex to find all `<table>...</table>` blocks (handles whitespace variations)
/// 2. Parses each table with scraper for analysis
/// 3. Classifies each as layout or data table
/// 4. Processes accordingly (text conversion vs normalization)
/// 5. Replaces tables in the HTML using regex substitution
///
/// # Arguments
/// * `html` - Raw HTML string containing tables
///
/// # Returns
/// * `Ok(String)` - HTML with preprocessed tables
/// * `Err(anyhow::Error)` - If processing fails
pub fn preprocess_tables(html: &str) -> Result<String> {
    // Use regex to replace all <table> elements
    let result = TABLE_REGEX.replace_all(html, |caps: &regex::Captures| {
        // Extract the matched table HTML
        let table_html = &caps[0];
        
        // Parse the table with scraper for analysis
        let fragment = Html::parse_fragment(table_html);
        
        // Find the table element in the parsed fragment
        let table = match fragment.select(&TABLE_SELECTOR).next() {
            Some(t) => t,
            None => {
                // If parsing fails, return original HTML unchanged
                return table_html.to_string();
            }
        };
        
        // Process the table based on its type
        if is_layout_table(&table) {
            // Convert layout tables to plain text
            extract_table_text(&table)
        } else {
            // Normalize data tables (expand colspan/rowspan)
            match normalize_data_table(&table) {
                Ok(normalized) => normalized,
                Err(_) => {
                    // If normalization fails, return original HTML
                    table_html.to_string()
                }
            }
        }
    });
    
    Ok(result.to_string())
}

// ============================================================================
// Layout Table Detection
// ============================================================================

/// Detect if a table is used for layout rather than data
///
/// Layout indicators:
/// 1. `role="presentation"` attribute
/// 2. Class names suggesting layout (layout, container)
/// 3. Only one column throughout entire table
/// 4. Large table (>5 rows) with no headers
fn is_layout_table(table: &ElementRef) -> bool {
    // Check role="presentation" attribute
    if table.value().attr("role") == Some("presentation") {
        return true;
    }
    
    // Check class names for layout indicators
    if let Some(class) = table.value().attr("class") {
        let class_lower = class.to_lowercase();
        if class_lower.contains("layout") || class_lower.contains("container") {
            return true;
        }
    }
    
    // Count columns in each row
    let rows: Vec<_> = table.select(&TR_SELECTOR).collect();
    if rows.is_empty() {
        return true;
    }
    
    // Check if all rows have <= 1 column
    let all_single_column = rows.iter().all(|row| {
        let cell_count: usize = row.select(&CELL_SELECTOR)
            .map(|cell| {
                cell.value().attr("colspan")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
            })
            .sum();
        cell_count <= 1
    });
    
    if all_single_column {
        return true;
    }
    
    // Check for large tables without headers
    let has_thead = table.select(&THEAD_SELECTOR).next().is_some();
    let has_th = table.select(&TH_SELECTOR).next().is_some();
    
    if !has_thead && !has_th && rows.len() > 5 {
        // Large table with no headers is likely layout
        return true;
    }
    
    false
}

/// Extract text content from layout table
fn extract_table_text(table: &ElementRef) -> String {
    table.text()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract caption text from table
///
/// Captions should be placed as regular text BEFORE the markdown table,
/// not embedded as a table cell.
///
/// # Arguments
/// * `table` - Table element to extract caption from
///
/// # Returns
/// * `Some(String)` - Caption text if found
/// * `None` - No caption element found
fn extract_table_caption(table: &ElementRef) -> Option<String> {
    table.select(&CAPTION_SELECTOR)
        .next()
        .map(|caption| {
            caption.text()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty())
}

/// Extract descriptive header rows from table
///
/// Descriptive headers are rows in `<thead>` where a single cell (or cells that
/// together) span the entire table width. These are not actual column headers
/// but explanatory text that should be placed BEFORE the table in markdown.
///
/// # Detection Logic
/// A row is considered descriptive if:
/// 1. It's in the `<thead>` section
/// 2. It has a single cell with `colspan >= max_cols`, OR
/// 3. All cells together sum to exactly `max_cols` but there's only 1-2 cells total
///    (indicating a full-width description split across cells)
///
/// # Arguments
/// * `table` - Table element to analyze
/// * `max_cols` - Maximum number of columns in the table
///
/// # Returns
/// * `Vec<String>` - List of descriptive text strings (empty if none found)
fn extract_descriptive_headers(table: &ElementRef, max_cols: usize) -> Vec<String> {
    let mut descriptive_texts = Vec::new();
    
    // Find thead section
    let thead = match table.select(&THEAD_SELECTOR).next() {
        Some(t) => t,
        None => return descriptive_texts,
    };
    
    // Examine each row in thead
    for row in thead.select(&TR_SELECTOR) {
        let cells: Vec<ElementRef> = row.select(&CELL_SELECTOR).collect();
        
        if cells.is_empty() {
            continue;
        }
        
        // Calculate total colspan for this row
        let total_colspan: usize = cells.iter()
            .map(|cell| {
                cell.value().attr("colspan")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
            })
            .sum();
        
        // Check if this is a descriptive row
        let is_descriptive = if cells.len() == 1 {
            // Single cell spanning full width or more
            total_colspan >= max_cols
        } else {
            // Two cells that together span full width (unusual but possible)
            // Multiple cells = actual header row
            cells.len() <= 2 && total_colspan >= max_cols
        };
        
        if is_descriptive {
            // Extract text from all cells
            let text = cells.iter()
                .flat_map(|cell| {
                    cell.text()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                })
                .collect::<Vec<_>>()
                .join(" ");
            
            if !text.is_empty() {
                descriptive_texts.push(text);
            }
        }
    }
    
    descriptive_texts
}

/// Identify the indices of descriptive header rows in the table
///
/// Returns the 0-based indices of rows (across the entire table, not just thead)
/// that should be skipped during grid parsing because they're descriptive text.
///
/// # Arguments
/// * `table` - Table element to analyze
/// * `max_cols` - Maximum number of columns in the table
///
/// # Returns
/// * `HashSet<usize>` - Set of row indices to skip (empty if none)
fn identify_descriptive_row_indices(
    table: &ElementRef, 
    max_cols: usize
) -> std::collections::HashSet<usize> {
    use std::collections::HashSet;
    
    let mut skip_indices = HashSet::new();
    
    // Find thead section
    let thead = match table.select(&THEAD_SELECTOR).next() {
        Some(t) => t,
        None => return skip_indices,
    };
    
    // Get all rows in the table (for global indexing)
    let all_rows: Vec<_> = table.select(&TR_SELECTOR).collect();
    
    // Get rows in thead (for analysis)
    let thead_rows: Vec<_> = thead.select(&TR_SELECTOR).collect();
    
    // Analyze each thead row
    for thead_row in &thead_rows {
        let cells: Vec<_> = thead_row.select(&CELL_SELECTOR).collect();
        
        if cells.is_empty() {
            continue;
        }
        
        let total_colspan: usize = cells.iter()
            .map(|cell| {
                cell.value().attr("colspan")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
            })
            .sum();
        
        let is_descriptive = if cells.len() == 1 {
            total_colspan >= max_cols
        } else {
            cells.len() <= 2 && total_colspan >= max_cols
        };
        
        if is_descriptive {
            // Find this row's index in the all_rows list
            if let Some(global_idx) = all_rows.iter().position(|r| {
                std::ptr::eq(r.value(), thead_row.value())
            }) {
                skip_indices.insert(global_idx);
            }
        }
    }
    
    skip_indices
}

// ============================================================================
// Data Table Normalization
// ============================================================================

/// Normalize data table by expanding colspan/rowspan and extracting descriptive elements
///
/// This function now:
/// 1. Extracts `<caption>` elements as text before the table
/// 2. Identifies and extracts descriptive header rows (full-width th cells)
/// 3. Parses the remaining table into a grid (skipping descriptive rows)
/// 4. Builds output as: descriptive text + "\n\n" + normalized table HTML
///
/// # Arguments
/// * `table` - Table element to normalize
///
/// # Returns
/// * `Ok(String)` - Normalized table with descriptive text placed before it
/// * `Err(anyhow::Error)` - If normalization fails
fn normalize_data_table(table: &ElementRef) -> Result<String> {
    // Step 1: Calculate max columns (needed for descriptive header detection)
    let rows: Vec<_> = table.select(&TR_SELECTOR).collect();
    if rows.is_empty() {
        return Ok(String::new());
    }
    let max_cols = calculate_max_columns(&rows)?;
    
    // Step 2: Extract caption
    let caption = extract_table_caption(table);
    
    // Step 3: Extract descriptive headers
    let descriptive_headers = extract_descriptive_headers(table, max_cols);
    
    // Step 4: Identify row indices to skip during grid parsing
    let skip_row_indices = identify_descriptive_row_indices(table, max_cols);
    
    // Step 5: Parse table into grid (excluding descriptive rows)
    let grid = parse_table_to_grid(table, &skip_row_indices)?;
    
    if grid.is_empty() {
        // If table is empty after removing descriptive rows, return just the descriptive text
        let mut output = String::new();
        if let Some(cap) = caption {
            output.push_str(&cap);
        }
        for desc in descriptive_headers {
            if !output.is_empty() {
                output.push(' ');
            }
            output.push_str(&desc);
        }
        return Ok(output);
    }
    
    // Step 6: Build output with descriptive text before table
    let mut output = String::new();
    
    // Add caption if present
    if let Some(cap) = caption {
        output.push_str(&cap);
        output.push_str("\n\n");
    }
    
    // Add descriptive headers if present
    for desc in descriptive_headers {
        output.push_str(&desc);
        output.push_str("\n\n");
    }
    
    // Add the normalized table HTML
    let table_html = build_table_from_grid(&grid);
    output.push_str(&table_html);
    
    Ok(output)
}

/// Parse HTML table into a 2D grid, expanding colspan/rowspan
///
/// This function now accepts a list of row indices to skip (descriptive header rows).
///
/// # Arguments
/// * `table` - Table element to parse
/// * `skip_row_indices` - Set of row indices (0-based) to skip during parsing
///
/// # Returns
/// * `Ok(Vec<Vec<GridCell>>)` - 2D grid of cells
/// * `Err(anyhow::Error)` - If table exceeds security limits
///
/// # Security
/// - MAX_GRID_ROWS: Prevents DoS via excessive rowspan accumulation
/// - MAX_GRID_COLS: Prevents DoS via excessive colspan
/// - Graceful truncation: Returns partial grid if limits exceeded
fn parse_table_to_grid(
    table: &ElementRef,
    skip_row_indices: &std::collections::HashSet<usize>
) -> Result<Vec<Vec<GridCell>>> {
    let rows: Vec<_> = table.select(&TR_SELECTOR).collect();
    
    if rows.is_empty() {
        return Ok(vec![]);
    }
    
    // Determine maximum columns needed
    let max_cols = calculate_max_columns(&rows)?;
    
    // SECURITY: Enforce maximum grid width
    if max_cols > MAX_GRID_COLS {
        return Err(anyhow::anyhow!(
            "Table too wide: {} columns (maximum allowed: {}). \
             This limit prevents memory exhaustion attacks.",
            max_cols,
            MAX_GRID_COLS
        ));
    }
    
    // Initialize grid and rowspan tracker
    let mut grid: Vec<Vec<GridCell>> = Vec::new();
    let mut rowspan_tracker: HashMap<(usize, usize), usize> = HashMap::new();
    let mut total_cells = 0usize;  // Track total cells
    
    // Track grid row index separately from source row index
    let mut grid_row_idx = 0;
    
    for (source_row_idx, row) in rows.iter().enumerate() {
        // SKIP DESCRIPTIVE ROWS
        if skip_row_indices.contains(&source_row_idx) {
            continue;
        }
        
        // SECURITY: Enforce maximum grid height
        if grid.len() >= MAX_GRID_ROWS {
            warn!(
                "Table exceeded maximum rows ({}), truncating at source row {}. \
                 Processed {} grid rows before limit.",
                MAX_GRID_ROWS,
                source_row_idx,
                grid.len()
            );
            break;  // Stop processing, return partial grid
        }
        
        // Ensure this row exists in grid
        while grid.len() <= grid_row_idx {
            grid.push(vec![GridCell::empty(); max_cols]);
        }
        
        let mut col_idx = 0;
        
        for cell in row.select(&CELL_SELECTOR) {
            // Skip columns occupied by rowspan from previous rows
            while col_idx < max_cols && rowspan_tracker.contains_key(&(grid_row_idx, col_idx)) {
                col_idx += 1;
            }
            
            if col_idx >= max_cols {
                break;
            }
            
            // Get colspan and rowspan, cap at reasonable limits
            let colspan = cell.value().attr("colspan")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 100);
            
            let rowspan = cell.value().attr("rowspan")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 100);
            
            // Use saturating multiplication for overflow safety
            let cells_in_span = colspan.saturating_mul(rowspan);
            
            // Check total cell limit BEFORE allocation
            let new_total = total_cells.saturating_add(cells_in_span);
            if new_total > MAX_TOTAL_CELLS {
                return Err(anyhow::anyhow!(
                    "Table too large: exceeds maximum {} cells (attempted {})",
                    MAX_TOTAL_CELLS,
                    new_total
                ));
            }
            total_cells = new_total;
            
            let is_header = cell.value().name() == "th";
            let align = extract_alignment(&cell);
            let content = extract_cell_content(&cell);
            
            // Fill grid cells for this span
            for r in 0..rowspan {
                let target_row = grid_row_idx.saturating_add(r);
                
                // SECURITY: Check if target_row would exceed maximum
                if target_row >= MAX_GRID_ROWS {
                    debug!(
                        "Rowspan at cell ({}, {}) would exceed max rows ({}), \
                         clamping expansion at row {}",
                        grid_row_idx, col_idx, MAX_GRID_ROWS, target_row
                    );
                    break;  // Stop expanding this cell vertically
                }
                
                for c in 0..colspan {
                    let target_col = col_idx.saturating_add(c);
                    
                    if target_col < max_cols {
                        // Mark cells as occupied for future rows (if rowspan > 1)
                        if r > 0 {
                            rowspan_tracker.insert((target_row, target_col), r);
                        }
                        
                        // Ensure grid has enough rows (now safe - already bounds-checked above)
                        while grid.len() <= target_row {
                            grid.push(vec![GridCell::empty(); max_cols]);
                        }
                        
                        // Set the cell content
                        grid[target_row][target_col] = GridCell {
                            content: content.clone(),
                            is_header,
                            align: align.clone(),
                        };
                    }
                }
            }
            
            col_idx = col_idx.saturating_add(colspan);
        }
        
        grid_row_idx += 1;
    }
    
    Ok(grid)
}

/// Calculate maximum columns needed for the table
///
/// This is now used by `normalize_data_table` before calling `parse_table_to_grid`
/// to determine the number of columns for descriptive header detection.
///
/// # Arguments
/// * `rows` - Slice of table row elements
///
/// # Returns
/// * `Ok(usize)` - Maximum number of columns across all rows
/// * `Err(anyhow::Error)` - If table exceeds security limits
fn calculate_max_columns(rows: &[ElementRef]) -> Result<usize> {
    let mut max_cols = 1usize;
    
    for row in rows {
        let mut row_cols = 0usize;
        
        for cell in row.select(&CELL_SELECTOR) {
            let colspan = cell.value().attr("colspan")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            
            // Use saturating addition to prevent overflow
            row_cols = row_cols.saturating_add(colspan);
            
            // Enforce per-row column limit
            if row_cols > MAX_TOTAL_CELLS {
                return Err(anyhow::anyhow!(
                    "Table row too wide: {} columns exceeds maximum {}",
                    row_cols,
                    MAX_TOTAL_CELLS
                ));
            }
        }
        
        max_cols = max_cols.max(row_cols);
    }
    
    Ok(max_cols)
}

/// Extract alignment from cell attributes
fn extract_alignment(cell: &ElementRef) -> Option<Alignment> {
    // Check align attribute
    if let Some(align) = cell.value().attr("align") {
        return match align.to_lowercase().as_str() {
            "left" => Some(Alignment::Left),
            "center" => Some(Alignment::Center),
            "right" => Some(Alignment::Right),
            _ => None,
        };
    }
    
    // Check style attribute for text-align
    if let Some(style) = cell.value().attr("style") {
        let style_lower = style.to_lowercase();
        if style_lower.contains("text-align:center") || style_lower.contains("text-align: center") {
            return Some(Alignment::Center);
        }
        if style_lower.contains("text-align:right") || style_lower.contains("text-align: right") {
            return Some(Alignment::Right);
        }
        if style_lower.contains("text-align:left") || style_lower.contains("text-align: left") {
            return Some(Alignment::Left);
        }
    }
    
    None
}

/// Extract text content from cell, preserving inline formatting but decoding HTML entities
///
/// This extracts the inner HTML of the cell, which allows html2md
/// to properly convert inline elements like <code>, <a>, <strong>, etc.
///
/// **Critical**: HTML entities MUST be decoded here because scraper's DOM
/// preserves the original HTML source. Even though clean_html_content() decoded
/// entities earlier in the pipeline, table preprocessing re-parses the HTML
/// and extracts the raw source which still contains entities.
///
/// Entities decoded: &#124; → |, &lt; → <, &gt; → >, &amp; → &, and all HTML5 named entities
fn extract_cell_content(cell: &ElementRef) -> String {
    let html = cell.html();
    
    // Find the opening tag end and closing tag start
    let start_tag_end = html.find('>').map(|pos| pos + 1).unwrap_or(0);
    let end_tag_start = html.rfind("</").unwrap_or(html.len());
    
    // Extract inner HTML
    let inner_html = html[start_tag_end..end_tag_start].trim();
    
    // Decode HTML entities before returning
    // This handles: &#124; → |, &lt; → <, &gt; → >, &amp; → &, etc.
    match decode(inner_html.as_bytes()).to_string() {
        Ok(decoded) => decoded,
        Err(e) => {
            // If decoding fails, log warning and return original
            // This ensures the conversion pipeline doesn't crash
            tracing::warn!(
                "Failed to decode HTML entities in table cell: {}. Using undecoded content.", 
                e
            );
            inner_html.to_string()
        }
    }
}

/// Build normalized HTML table from grid
///
/// Constructs a clean HTML table without colspan/rowspan attributes.
/// Determines header row based on whether cells are marked as headers.
fn build_table_from_grid(grid: &[Vec<GridCell>]) -> String {
    if grid.is_empty() {
        return String::new();
    }
    
    let mut html = String::from("<table>\n");
    
    // Determine if first row is header row
    let first_row_is_header = grid.first()
        .map(|row| row.iter().any(|cell| cell.is_header))
        .unwrap_or(false);
    
    // Build thead if first row is headers
    if first_row_is_header {
        html.push_str("<thead>\n<tr>\n");
        for cell in &grid[0] {
            // Add alignment if present
            if let Some(align) = &cell.align {
                let align_str = match align {
                    Alignment::Left => " align=\"left\"",
                    Alignment::Center => " align=\"center\"",
                    Alignment::Right => " align=\"right\"",
                };
                html.push_str(&format!("<th{}>{}</th>\n", align_str, cell.content));
            } else {
                html.push_str(&format!("<th>{}</th>\n", cell.content));
            }
        }
        html.push_str("</tr>\n</thead>\n");
    }
    
    // Build tbody
    html.push_str("<tbody>\n");
    let start_row = if first_row_is_header { 1 } else { 0 };
    
    for row in &grid[start_row..] {
        html.push_str("<tr>\n");
        for cell in row {
            // Add alignment if present
            if let Some(align) = &cell.align {
                let align_str = match align {
                    Alignment::Left => " align=\"left\"",
                    Alignment::Center => " align=\"center\"",
                    Alignment::Right => " align=\"right\"",
                };
                html.push_str(&format!("<td{}>{}</td>\n", align_str, cell.content));
            } else {
                html.push_str(&format!("<td>{}</td>\n", cell.content));
            }
        }
        html.push_str("</tr>\n");
    }
    
    html.push_str("</tbody>\n</table>");
    
    html
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_table() -> Result<()> {
        let html = r#"
            <table>
                <tr><th>A</th><th>B</th></tr>
                <tr><td>1</td><td>2</td></tr>
            </table>
        "#;
        
        let result = preprocess_tables(html)?;
        assert!(result.contains("<thead>"));
        assert!(result.contains("<tbody>"));
        Ok(())
    }

    #[test]
    fn test_colspan_expansion() -> Result<()> {
        let html = r#"
            <table>
                <tr><th colspan="2">Header</th></tr>
                <tr><td>A</td><td>B</td></tr>
            </table>
        "#;
        
        let result = preprocess_tables(html)?;
        // Should not contain colspan in output
        assert!(!result.contains("colspan"));
        // Should contain the header content twice
        let header_count = result.matches("Header").count();
        assert_eq!(header_count, 2);
        Ok(())
    }

    #[test]
    fn test_rowspan_expansion() -> Result<()> {
        let html = r#"
            <table>
                <tr><td rowspan="2">A</td><td>1</td></tr>
                <tr><td>2</td></tr>
            </table>
        "#;
        
        let result = preprocess_tables(html)?;
        // Should not contain rowspan in output
        assert!(!result.contains("rowspan"));
        // Should contain "A" twice (once per row)
        let a_count = result.matches(">A<").count();
        assert_eq!(a_count, 2);
        Ok(())
    }

    #[test]
    fn test_layout_table_detection() -> Result<()> {
        let html = r#"
            <table role="presentation">
                <tr><td>Just text content</td></tr>
            </table>
        "#;
        
        let result = preprocess_tables(html)?;
        // Layout tables should be converted to plain text
        assert!(!result.contains("<table"));
        assert!(result.contains("Just text content"));
        Ok(())
    }

    #[test]
    fn test_single_column_layout_table() -> Result<()> {
        let html = r#"
            <table>
                <tr><td>Line 1</td></tr>
                <tr><td>Line 2</td></tr>
                <tr><td>Line 3</td></tr>
            </table>
        "#;
        
        let result = preprocess_tables(html)?;
        // Single column tables should be treated as layout
        assert!(!result.contains("<table"));
        Ok(())
    }

    #[test]
    fn test_alignment_preservation() -> Result<()> {
        let html = r#"
            <table>
                <tr>
                    <th align="left">Left</th>
                    <th align="center">Center</th>
                    <th align="right">Right</th>
                </tr>
            </table>
        "#;
        
        let result = preprocess_tables(html)?;
        assert!(result.contains("align=\"left\""));
        assert!(result.contains("align=\"center\""));
        assert!(result.contains("align=\"right\""));
        Ok(())
    }

    #[test]
    fn test_empty_table() -> Result<()> {
        let html = r#"<table></table>"#;
        let result = preprocess_tables(html)?;
        // Empty tables should result in empty string
        assert!(result.is_empty() || result.trim().is_empty());
        Ok(())
    }

    #[test]
    fn test_complex_colspan_rowspan() -> Result<()> {
        let html = r#"
            <table>
                <tr>
                    <td colspan="2" rowspan="2">Big</td>
                    <td>C</td>
                </tr>
                <tr>
                    <td>D</td>
                </tr>
                <tr>
                    <td>E</td>
                    <td>F</td>
                    <td>G</td>
                </tr>
            </table>
        "#;
        
        let result = preprocess_tables(html)?;
        assert!(!result.contains("colspan"));
        assert!(!result.contains("rowspan"));
        // Big should appear 4 times (2 cols × 2 rows)
        let big_count = result.matches(">Big<").count();
        assert_eq!(big_count, 4);
        Ok(())
    }
}
