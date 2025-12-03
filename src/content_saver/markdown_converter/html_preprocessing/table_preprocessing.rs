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
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;
use std::sync::LazyLock;

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

static TABLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<table[^>]*>.*?</table>")
        .expect("BUG: hardcoded table regex is statically valid")
});

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

// ============================================================================
// Data Table Normalization
// ============================================================================

/// Normalize data table by expanding colspan/rowspan
///
/// This function:
/// 1. Parses the table into a 2D grid
/// 2. Expands cells with colspan/rowspan by duplicating content
/// 3. Rebuilds clean HTML without colspan/rowspan attributes
fn normalize_data_table(table: &ElementRef) -> Result<String> {
    // Parse table into grid with colspan/rowspan expansion
    let grid = parse_table_to_grid(table)?;
    
    if grid.is_empty() {
        return Ok(String::new());
    }
    
    // Rebuild HTML table from grid
    let normalized_html = build_table_from_grid(&grid);
    
    Ok(normalized_html)
}

/// Parse HTML table into a 2D grid, expanding colspan/rowspan
///
/// Algorithm:
/// 1. Determine maximum columns by examining all rows
/// 2. Create a 2D grid to hold all logical cells
/// 3. Track cells occupied by rowspan from previous rows
/// 4. For each cell, fill the grid positions it spans
/// 5. Duplicate content across spanned cells
fn parse_table_to_grid(table: &ElementRef) -> Result<Vec<Vec<GridCell>>> {
    let rows: Vec<_> = table.select(&TR_SELECTOR).collect();
    
    if rows.is_empty() {
        return Ok(vec![]);
    }
    
    // Determine maximum columns needed
    let max_cols = calculate_max_columns(&rows);
    
    // Initialize grid and rowspan tracker
    let mut grid: Vec<Vec<GridCell>> = Vec::new();
    let mut rowspan_tracker: HashMap<(usize, usize), usize> = HashMap::new();
    
    for (row_idx, row) in rows.iter().enumerate() {
        // Ensure this row exists in grid
        while grid.len() <= row_idx {
            grid.push(vec![GridCell::empty(); max_cols]);
        }
        
        let mut col_idx = 0;
        
        for cell in row.select(&CELL_SELECTOR) {
            // Skip columns occupied by rowspan from previous rows
            while col_idx < max_cols && rowspan_tracker.contains_key(&(row_idx, col_idx)) {
                col_idx += 1;
            }
            
            if col_idx >= max_cols {
                break;
            }
            
            // Get colspan and rowspan, cap at reasonable limits
            let colspan = cell.value().attr("colspan")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 100); // Cap at 100 to prevent explosion
            
            let rowspan = cell.value().attr("rowspan")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 100); // Cap at 100 to prevent explosion
            
            let is_header = cell.value().name() == "th";
            let align = extract_alignment(&cell);
            let content = extract_cell_content(&cell);
            
            // Fill grid cells for this span
            for r in 0..rowspan {
                for c in 0..colspan {
                    let target_row = row_idx + r;
                    let target_col = col_idx + c;
                    
                    if target_col < max_cols {
                        // Mark cells as occupied for future rows (if rowspan > 1)
                        if r > 0 {
                            rowspan_tracker.insert((target_row, target_col), r);
                        }
                        
                        // Ensure grid has enough rows
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
            
            col_idx += colspan;
        }
    }
    
    Ok(grid)
}

/// Calculate maximum columns needed for the table
fn calculate_max_columns(rows: &[ElementRef]) -> usize {
    rows.iter()
        .map(|row| {
            row.select(&CELL_SELECTOR)
                .map(|cell| {
                    cell.value().attr("colspan")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(1)
                        .max(1)
                })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(1)
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

/// Extract text content from cell, preserving inline formatting
///
/// This extracts the inner HTML of the cell, which allows html2md
/// to properly convert inline elements like <code>, <a>, <strong>, etc.
fn extract_cell_content(cell: &ElementRef) -> String {
    let html = cell.html();
    
    // Find the opening tag end and closing tag start
    let start_tag_end = html.find('>').map(|pos| pos + 1).unwrap_or(0);
    let end_tag_start = html.rfind("</").unwrap_or(html.len());
    
    // Extract and trim content
    html[start_tag_end..end_tag_start].trim().to_string()
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
