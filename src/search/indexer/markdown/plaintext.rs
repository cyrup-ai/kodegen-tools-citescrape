//! Markdown to plain text conversion with comprehensive edge case handling

use super::helpers::{
    is_closing_html_tag, is_horizontal_rule, is_html_block_tag, normalize_whitespace,
    remove_list_marker_inplace,
};
use super::inline::clean_inline_formatting;
use imstr::ImString;

/// Convert markdown to plain text with comprehensive edge case handling
#[inline]
pub(crate) fn markdown_to_plain_text_optimized(markdown: &str) -> ImString {
    if markdown.is_empty() {
        return ImString::new();
    }

    // Pre-allocate with better estimate
    let mut result = String::with_capacity((markdown.len() * 3) / 4);

    // State tracking for context-aware parsing
    let mut in_code_block = false;
    let mut code_fence = String::new();
    let mut in_table = false;
    let mut in_html_block = false;
    let mut in_math_block = false;
    let mut list_depth = 0;
    let mut last_was_blank = true;
    let mut html_block_line_count = 0;
    const MAX_HTML_BLOCK_LINES: usize = 50;

    // Process line by line with reusable buffer (Option 3: Hybrid Approach)
    let mut line_buffer = String::with_capacity(256);

    for line in markdown.lines() {
        let trimmed = line.trim();
        let indent_level = line.len() - line.trim_start().len();

        // Handle code blocks (both ``` and ~~~)
        if !in_code_block && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            in_code_block = true;
            
            // Capture full fence: character type and count
            let fence_char = trimmed.chars().next().unwrap(); // '`' or '~'
            let fence_count = trimmed.chars().take_while(|&c| c == fence_char).count();
            
            // Store the exact fence string for matching (e.g., "```" or "`````")
            code_fence = fence_char.to_string().repeat(fence_count);
            continue;
        } else if in_code_block && !code_fence.is_empty() {
            // Closing fence must have same char type and equal or greater count
            let fence_char = code_fence.chars().next().unwrap();
            let fence_count = code_fence.len();
            let line_fence_count = trimmed.chars().take_while(|&c| c == fence_char).count();
            
            // Valid closing: same type, count >= opening, at least 3 chars
            if line_fence_count >= fence_count && line_fence_count >= 3 {
                in_code_block = false;
                code_fence.clear();
                continue;
            }
        }

        if in_code_block {
            // Preserve code block content with proper spacing
            if !last_was_blank || !trimmed.is_empty() {
                result.push_str(trimmed);
                result.push('\n');
                last_was_blank = trimmed.is_empty();
            }
            continue;
        }

        // Handle math blocks
        if trimmed == "$$" {
            in_math_block = !in_math_block;
            continue;
        }

        if in_math_block {
            result.push_str(trimmed);
            result.push(' ');
            last_was_blank = false;
            continue;
        }

        // Handle HTML blocks
        if trimmed.starts_with('<') && !trimmed.starts_with("<!--") && is_html_block_tag(trimmed) {
            in_html_block = true;
            html_block_line_count = 0;
        }

        if in_html_block {
            html_block_line_count += 1;
            
            // Check all escape conditions: proper closing tag, consecutive empty lines, or line limit
            if (trimmed.ends_with('>') && is_closing_html_tag(trimmed))
                || (trimmed.is_empty() && last_was_blank)
                || (html_block_line_count > MAX_HTML_BLOCK_LINES)
            {
                in_html_block = false;
                html_block_line_count = 0;
            }
            
            continue;
        }

        // Skip horizontal rules
        if is_horizontal_rule(trimmed) {
            if !last_was_blank {
                result.push(' ');
            }
            last_was_blank = true;
            continue;
        }

        // Process the line using reusable buffer
        line_buffer.clear();
        process_markdown_line_inplace(
            &mut line_buffer,
            line,
            &mut in_table,
            &mut list_depth,
            indent_level,
        );

        // Clean up the processed line (UTF-8 safe allocating version)
        line_buffer = clean_inline_formatting(line_buffer);

        // Add to result with proper spacing
        let cleaned = line_buffer.trim();
        if cleaned.is_empty() {
            last_was_blank = true;
        } else {
            if !last_was_blank && !result.is_empty() {
                result.push(' ');
            }
            result.push_str(cleaned);
            last_was_blank = false;
        }
    }

    // Final cleanup: normalize whitespace
    ImString::from(normalize_whitespace(&result))
}

/// Process a single markdown line in-place into buffer (zero-allocation)
#[inline]
fn process_markdown_line_inplace(
    buffer: &mut String,
    line: &str,
    in_table: &mut bool,
    list_depth: &mut usize,
    indent_level: usize,
) {
    let trimmed = line.trim();

    // Handle tables
    if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 2 {
        let inner = &trimmed[1..trimmed.len() - 1];

        // Check if this is a separator row
        if inner.chars().all(|c| "-|: \t".contains(c)) {
            *in_table = true;
            return; // Empty buffer
        }

        if *in_table || inner.contains('|') {
            *in_table = true;
            // Process table cells into buffer
            let mut first = true;
            for cell in inner.split('|') {
                let cell_trimmed = cell.trim();
                if !cell_trimmed.is_empty() {
                    if !first {
                        buffer.push(' ');
                    }
                    buffer.push_str(cell_trimmed);
                    first = false;
                }
            }
            return;
        }
    }

    // Not a table line - reset table state if it was set
    if *in_table {
        *in_table = false;
    }

    // Start with the line content
    buffer.push_str(line);

    // Remove headers (ATX style)
    if let Some(pos) = buffer.find(|c: char| c != '#') {
        // ATX-style header: 1-6 '#' characters followed by space
        if (1..=6).contains(&pos) && buffer.len() > pos {
            // Safety: find() returns valid UTF-8 boundary, and pos < len is verified
            if buffer[pos..].starts_with(' ') {
                // Remove header markers including the trailing space
                buffer.drain(..=pos);
            }
        }
    }

    // Handle lists (ordered and unordered) - in-place
    remove_list_marker_inplace(buffer, indent_level, list_depth);

    // Remove blockquote markers (nested) - in-place
    while buffer.trim_start().starts_with('>') {
        // Find where '>' is and remove it along with following whitespace
        if let Some(pos) = buffer.find('>') {
            buffer.drain(..=pos);
            // Remove leading whitespace after '>' - O(n) single drain
            let ws_end = buffer
                .chars()
                .position(|c| !c.is_whitespace())
                .unwrap_or(buffer.len());
            if ws_end > 0 {
                buffer.drain(..ws_end);
            }
        } else {
            break;
        }
    }

    // Handle definition lists
    if buffer.starts_with(": ") {
        buffer.drain(..2);
    }
}
