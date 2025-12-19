//! Bracket unescaping for orphan escaped brackets in markdown output.
//!
//! The htmd library escapes brackets `[` and `]` in text to prevent them from
//! being interpreted as markdown links. However, when text that looks like
//! `[`code`]` is NOT actually a link (e.g., broken/empty anchor tags), the
//! escaped `\[...\]` output is incorrect.
//!
//! This module provides `unescape_orphan_brackets` which detects and fixes
//! these patterns while preserving actual link syntax.

use fancy_regex::Regex;
use std::sync::LazyLock;

/// Matches escaped brackets NOT followed by a link URL opening paren.
/// 
/// Pattern breakdown:
/// - `\\[` - literal backslash followed by opening bracket
/// - `([^\]]*?)` - capture group: bracket content (non-greedy, no closing brackets)
/// - `\\]` - literal backslash followed by closing bracket
/// - `(?!\()` - negative lookahead: NOT followed by opening paren (link URL)
///
/// This ensures we only unescape brackets that are NOT part of a valid
/// markdown link structure like `[text](url)`.
static ORPHAN_ESCAPED_BRACKETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(\[)([^\]]*?)\\(\])(?!\()")
        .expect("ORPHAN_ESCAPED_BRACKETS: hardcoded regex is valid")
});

/// Unescape orphan brackets that htmd incorrectly escaped.
///
/// This function processes markdown line-by-line, tracking code fence state
/// to avoid modifying content inside code blocks.
///
/// # Transformations
///
/// - `\[text\]` → `[text]` (when NOT followed by `(url)`)
/// - `\[`code`\]` → `[`code`]` (inline code with brackets)
///
/// # Preserved Patterns
///
/// - `[text](url)` - actual links remain unchanged
/// - Content inside code fences - preserved verbatim
/// - Other escaped chars (`\*`, `\_`, etc.) - unchanged
///
/// # Arguments
///
/// * `markdown` - Markdown content potentially containing orphan escaped brackets
///
/// # Returns
///
/// Markdown with orphan brackets unescaped.
pub fn unescape_orphan_brackets(markdown: &str) -> String {
    // Fast path: if no escaped brackets, return as-is
    if !markdown.contains("\\[") {
        return markdown.to_string();
    }
    
    let mut result = String::with_capacity(markdown.len());
    let mut in_code_fence = false;
    let mut lines = markdown.lines().peekable();
    
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        
        // Track code fence state
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            result.push_str(line);
            // Add newline unless this was the last line
            if lines.peek().is_some() || markdown.ends_with('\n') {
                result.push('\n');
            }
            continue;
        }
        
        if in_code_fence {
            // Inside code fence - preserve as-is
            result.push_str(line);
        } else {
            // Outside code fence - unescape orphan brackets
            // The regex replaces \[content\] with [content]
            let processed = ORPHAN_ESCAPED_BRACKETS.replace_all(line, "[$2]");
            result.push_str(&processed);
        }
        
        // Add newline unless this was the last line
        if lines.peek().is_some() || markdown.ends_with('\n') {
            result.push('\n');
        }
    }
    
    result
}
