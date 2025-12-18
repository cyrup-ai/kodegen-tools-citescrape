//! Markdown processing and heading normalization functionality.
//!
//! Processes markdown content by normalizing headings and handling various markdown formats.

use super::code_fence_detection::{detect_code_fence, looks_like_code, CodeFence};
use super::heading_extraction::{extract_heading_level, normalize_heading_level, HEADING_PREFIXES};
use super::shell_syntax_repair::repair_shell_syntax;
use regex::Regex;
use std::sync::LazyLock;

// Safety net: Remove duplicated hash symbols in markdown headings
// Catches patterns like "## # Heading" or "### # Section"
// This handles edge cases that slip through HTML preprocessing
static DUPLICATE_HEADING_HASH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(#{1,6})\s+#\s+")
        .expect("DUPLICATE_HEADING_HASH: hardcoded regex is valid")
});

static MALFORMED_HR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-\s+-+$")
        .expect("SAFETY: hardcoded regex r\"^-\\s+-+$\" is statically valid")
});

// Remove malformed list markers mixed with heading syntax
// Catches patterns like "## * * Text" or "### - Content" 
// This handles edge cases where html2md outputs both heading and list syntax
static MALFORMED_LIST_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(#{1,6})\s+(?:[*\-]\s+)+")
        .expect("MALFORMED_LIST_HEADING: hardcoded regex is valid")
});

/// Matches "Section titled" anchor patterns in markdown (fallback safety net)
/// Removes any remaining anchor links that escaped HTML preprocessing
/// Pattern: [Section titled "..."](...) with optional newline
static SECTION_ANCHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\[Section titled "[^"]+"\]\(#[^)]+\)\n?"#)
        .expect("SECTION_ANCHOR_RE: hardcoded regex is valid")
});

/// Process markdown headings to normalize heading levels and handle different markdown styles
pub fn process_markdown_headings(markdown: &str) -> String {
    // Safety net: Remove any remaining "## # " patterns from markdown
    // This catches edge cases that escaped HTML preprocessing
    let markdown = DUPLICATE_HEADING_HASH.replace_all(markdown, "$1 ").to_string();

    // Remove malformed list markers mixed with heading syntax
    // This catches patterns like "## * * Text" where html2md produced both syntaxes
    let markdown = MALFORMED_LIST_HEADING.replace_all(&markdown, "$1 ").to_string();

    let lines: Vec<&str> = markdown.lines().collect();

    // FIRST PASS: Fix malformed horizontal rules (- ---------)
    let mut cleaned_lines = Vec::with_capacity(lines.len());
    for line in &lines {
        if MALFORMED_HR.is_match(line.trim()) {
            // Convert malformed HR to proper markdown HR
            cleaned_lines.push("---");
        } else {
            cleaned_lines.push(*line);
        }
    }
    
    // Convert back to lines for main processing
    let lines: Vec<&str> = cleaned_lines.to_vec();

    // SHELL SYNTAX REPAIR PASS
    // Apply BEFORE heading processing to ensure shell code is preserved correctly
    let markdown_after_hr = lines.join("\n");
    let markdown = repair_shell_syntax(&markdown_after_hr);
    let lines: Vec<&str> = markdown.lines().collect();

    // SECOND PASS: Process all lines
    let mut processed_lines = Vec::new();
    let mut fence_stack: Option<CodeFence> = None;

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // Track code blocks to avoid processing headings inside them
        if let Some((fence_char, fence_count)) = detect_code_fence(trimmed) {
            if let Some(ref current_fence) = fence_stack {
                // We're inside a code block - check if this closes it
                if fence_char == current_fence.char && fence_count >= current_fence.count {
                    // This closes the current fence
                    fence_stack = None;
                }
            } else {
                // This opens a new code fence
                fence_stack = Some(CodeFence {
                    char: fence_char,
                    count: fence_count,
                    line_number: i,
                });
            }
            processed_lines.push(line.to_string());
            i += 1;
            continue;
        }

        if fence_stack.is_none() {
            // Check if this is a setext-style heading (text followed by === or ---)
            if i + 1 < lines.len() {
                let next_line = lines[i + 1];
                let next_trimmed = next_line.trim();

                // Improved detection: Check for setext underlines
                // Must be at least 3 characters, all = or all -
                let is_setext_underline = !next_trimmed.is_empty()
                    && next_trimmed.len() >= 3
                    && (next_trimmed.chars().all(|c| c == '=')
                        || next_trimmed.chars().all(|c| c == '-'));

                // Additional check: Previous line should look like heading text
                // (not empty, not already a heading, reasonable length)
                let current_trimmed = line.trim();
                let looks_like_heading_text = !current_trimmed.is_empty()
                    && !current_trimmed.starts_with('#')
                    && current_trimmed.len() <= 200;

                if is_setext_underline && looks_like_heading_text {
                    // This is a setext heading - convert to ATX style
                    let level = if next_trimmed.chars().all(|c| c == '=') {
                        1
                    } else {
                        2
                    };
                    let normalized_level = normalize_heading_level(level);
                    let new_heading =
                        format!("{}{}", HEADING_PREFIXES[normalized_level - 1], current_trimmed);
                    processed_lines.push(new_heading);
                    i += 2; // Skip both the heading line and the underline
                    continue;
                }
            }

            // Check for ATX-style headings
            if let Some(heading) = extract_heading_level(trimmed) {
                let (level, content) = heading;
                let normalized_level = normalize_heading_level(level);
                let new_heading = format!("{}{}", HEADING_PREFIXES[normalized_level - 1], content);
                processed_lines.push(new_heading);
            } else {
                processed_lines.push(line.to_string());
            }
        } else {
            // Inside a code fence - preserve the line as-is
            processed_lines.push(line.to_string());
        }

        i += 1;
    }

    // Auto-close fence if still open (best-effort recovery)
    if let Some(fence) = fence_stack {
        tracing::debug!(
            "Unclosed code fence starting at line {} (char: '{}', count: {}), attempting recovery",
            fence.line_number,
            fence.char,
            fence.count
        );

        let closing = fence.char.to_string().repeat(fence.count);

        // Handle edge case: empty processed_lines
        if processed_lines.is_empty() {
            tracing::warn!(
                "Auto-closing fence on empty document (fence opened at line {})",
                fence.line_number
            );
            processed_lines.push(closing);
        } else {
            // Strategy: Look backwards from end to find last code-like line
            let mut last_code_idx = processed_lines.len() - 1;
            for (idx, line) in processed_lines.iter().enumerate().rev() {
                if looks_like_code(line) {
                    last_code_idx = idx;
                    break;
                }
            }

            // Safe insert: clamp to valid range
            let insert_pos = (last_code_idx + 1).min(processed_lines.len());
            processed_lines.insert(insert_pos, closing);

            tracing::debug!(
                "Auto-closed fence at line {} (after last code-like content at line {})",
                insert_pos + 1,
                last_code_idx + 1
            );
        }
    }

    let result = processed_lines.join("\n");

    // Remove any remaining "Section titled" anchor patterns (safety net)
    let result = SECTION_ANCHOR_RE.replace_all(&result, "");

    result.to_string()
}


/// Fix code fences that are merged with preceding text
///
/// Ensures code fences always start on a new line by inserting newlines
/// before any fence that's merged with text (e.g., "text```" → "text\n\n```")
///
/// This fixes the bug where code fences appear merged with preceding content
/// like "Set the following environment variables to enable Bedrock:```ruby"
pub fn fix_merged_code_fences(markdown: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    
    // Match text immediately followed by code fence (no newline)
    static MERGED_FENCE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"([^\n])```")
            .expect("MERGED_FENCE regex is valid")
    });
    
    // Insert newline before code fence
    MERGED_FENCE.replace_all(markdown, "$1\n\n```").to_string()
}
