//! Markdown processing and heading normalization functionality.
//!
//! Processes markdown content by normalizing headings and handling various markdown formats.

use super::code_fence_detection::{detect_code_fence, looks_like_code, CodeFence};
use super::heading_extraction::{extract_heading_level, normalize_heading_level, HEADING_PREFIXES};

/// Process markdown headings to normalize heading levels and handle different markdown styles
pub fn process_markdown_headings(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();

    // Process all lines
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

                if !line.trim().is_empty()
                    && (next_trimmed.chars().all(|c| c == '=')
                        || next_trimmed.chars().all(|c| c == '-'))
                    && !next_trimmed.is_empty()
                {
                    // This is a setext heading - convert to ATX style
                    let level = if next_trimmed.chars().all(|c| c == '=') {
                        1
                    } else {
                        2
                    };
                    let normalized_level = normalize_heading_level(level);
                    let new_heading =
                        format!("{}{}", HEADING_PREFIXES[normalized_level - 1], line.trim());
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
        tracing::warn!(
            "Unclosed code fence starting at line {} (char: '{}', count: {}), attempting recovery",
            fence.line_number,
            fence.char,
            fence.count
        );

        // Strategy: Look backwards from end to find last code-like line
        let mut last_code_idx = processed_lines.len().saturating_sub(1);
        for (idx, line) in processed_lines.iter().enumerate().rev() {
            if looks_like_code(line) {
                last_code_idx = idx;
                break;
            }
        }

        // Insert closing fence after last code line
        let closing = fence.char.to_string().repeat(fence.count);
        processed_lines.insert(last_code_idx + 1, closing);

        tracing::info!(
            "Auto-closed fence at line {} (after last code-like content)",
            last_code_idx + 1
        );
    }

    processed_lines.join("\n")
}
