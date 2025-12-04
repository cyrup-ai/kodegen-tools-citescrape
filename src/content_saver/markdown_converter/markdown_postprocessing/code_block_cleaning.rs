//! Code block cleaning utilities for markdown postprocessing.
//!
//! Removes UI artifacts from code viewer widgets, specifically
//! "X collapsed lines" text that appears in scraped code blocks.

use super::code_fence_detection::{detect_code_fence, CodeFence};
use regex::Regex;
use std::sync::LazyLock;

/// Matches "X collapsed lines" or "X collapsed line" text
/// 
/// Pattern: `^\s*\d+ collapsed lines?$`
/// 
/// Matches:
/// - "26 collapsed lines"
/// - "1 collapsed line"
/// - "  100 collapsed lines" (with leading whitespace)
/// 
/// Does NOT match:
/// - "// 26 collapsed lines" (has code-like prefix)
/// - "collapsed lines" (no number)
/// - "26 collapsed" (incomplete)
static COLLAPSED_LINES_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\d+ collapsed lines?$")
        .expect("COLLAPSED_LINES_RE: hardcoded regex is statically valid")
});

/// Filter out "X collapsed lines" text from code blocks
///
/// This removes UI artifacts from code viewer widgets that get captured
/// during HTML-to-markdown conversion. The function:
/// 
/// 1. Tracks code fence state (opening/closing ```)
/// 2. When inside a code block, filters lines matching the pattern
/// 3. Preserves all other lines unchanged
/// 4. Works with both triple-backtick and triple-tilde fences
///
/// # Arguments
///
/// * `markdown` - Markdown content potentially containing collapsed line indicators
///
/// # Returns
///
/// * Cleaned markdown with collapsed line indicators removed from code blocks
///
/// # Examples
///
/// ```rust
/// let markdown = r#"
/// Some text
/// ```rust
/// 26 collapsed lines
/// fn main() {
///     println!("Hello");
/// }
/// ```
/// "#;
/// 
/// let cleaned = filter_collapsed_lines(markdown);
/// // Result: code block without "26 collapsed lines"
/// ```
pub fn filter_collapsed_lines(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut filtered_lines = Vec::with_capacity(lines.len());
    let mut fence_stack: Option<CodeFence> = None;

    for (line_num, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();

        // Track code fence state
        if let Some((fence_char, fence_count)) = detect_code_fence(trimmed) {
            if let Some(ref current_fence) = fence_stack {
                // Check if this closes the current fence
                if fence_char == current_fence.char && fence_count >= current_fence.count {
                    fence_stack = None;
                }
            } else {
                // Open a new code fence
                fence_stack = Some(CodeFence {
                    char: fence_char,
                    count: fence_count,
                    line_number: line_num,
                });
            }
            // Always preserve fence lines
            filtered_lines.push(line.to_string());
            continue;
        }

        // Filter logic: Only filter inside code blocks
        if fence_stack.is_some() {
            // Inside a code block - check if line matches pattern
            if COLLAPSED_LINES_RE.is_match(line.trim()) {
                // Skip this line (it's a collapsed lines indicator)
                tracing::debug!(
                    "Filtered collapsed lines indicator at line {}: '{}'",
                    line_num + 1,
                    line.trim()
                );
                continue;
            }
        }

        // Preserve all other lines
        filtered_lines.push(line.to_string());
    }

    filtered_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_single_collapsed_line() {
        let markdown = r#"```rust
26 collapsed lines
fn main() {}
```"#;

        let result = filter_collapsed_lines(markdown);
        assert!(!result.contains("26 collapsed lines"));
        assert!(result.contains("fn main() {}"));
    }

    #[test]
    fn test_preserve_code_with_similar_text() {
        let markdown = r#"```rust
// This mentions 26 collapsed lines in a comment
fn main() {}
```"#;

        let result = filter_collapsed_lines(markdown);
        // Should preserve because it doesn't match exact pattern
        assert!(result.contains("// This mentions 26 collapsed lines"));
    }

    #[test]
    fn test_filter_multiple_blocks() {
        let markdown = r#"
Text before

```rust
10 collapsed lines
fn foo() {}
```

More text

```python
5 collapsed lines
def bar():
    pass
```
"#;

        let result = filter_collapsed_lines(markdown);
        assert!(!result.contains("10 collapsed lines"));
        assert!(!result.contains("5 collapsed lines"));
        assert!(result.contains("fn foo() {}"));
        assert!(result.contains("def bar():"));
    }

    #[test]
    fn test_no_change_outside_code_blocks() {
        let markdown = r#"
Regular text
26 collapsed lines
More text
"#;

        let result = filter_collapsed_lines(markdown);
        // Should preserve because it's outside code blocks
        assert!(result.contains("26 collapsed lines"));
    }
}
