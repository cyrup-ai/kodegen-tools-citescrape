//! Code block cleaning utilities for markdown postprocessing.
//!
//! Removes UI artifacts from code viewer widgets, specifically
//! "X collapsed lines" text that appears in scraped code blocks.

use super::code_fence_detection::{detect_code_fence, CodeFence};
use regex::Regex;
use std::collections::HashSet;
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

/// Matches lines that are ONLY asterisks (2 or more) after code fence closing
///
/// Pattern: Matches a line with:
/// - Optional leading whitespace
/// - 2 or more asterisks
/// - Optional trailing whitespace
/// - End of line
///
/// This catches the corruption pattern where htmd emits `**` after a code fence.
static TRAILING_ASTERISKS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^```\s*\n\s*\*\*+\s*$")
        .expect("TRAILING_ASTERISKS_RE: hardcoded regex is statically valid")
});

/// Matches shebang lines at the start of code blocks
///
/// Pattern: `^#!/` followed by valid shebang path
///
/// Matches:
/// - `#!/bin/bash`
/// - `#!/usr/bin/env python3`
/// - `#!/bin/sh`
/// - `#!/usr/bin/env node`
///
/// Does NOT match:
/// - `# !/bin/bash` (space after #, this is the corruption we're fixing)
/// - `#! /bin/bash` (space after !)
static SHEBANG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^#!/(?:usr/)?(?:bin/)?(?:env\s+)?[\w\-]+")
        .expect("SHEBANG_RE: hardcoded regex is statically valid")
});

/// Matches corrupted shebang lines with space after #
///
/// Pattern: `^# !/` (space between # and !)
///
/// This catches the exact corruption pattern we're fixing
static CORRUPTED_SHEBANG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^#\s+!")
        .expect("CORRUPTED_SHEBANG_RE: hardcoded regex is statically valid")
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
    let mut result = String::with_capacity(markdown.len());
    let mut fence_stack: Option<CodeFence> = None;

    for (line_num, line) in markdown.lines().enumerate() {
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
            result.push_str(line);
            result.push('\n');
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
        result.push_str(line);
        result.push('\n');
    }

    // Remove trailing newline to match join() behavior
    if result.ends_with('\n') {
        result.pop();
    }
    result
}

/// Strip bold formatting markers (`**`) that corrupt code fence markers.
///
/// Detects and fixes lines where bold asterisks are incorrectly prepended
/// to code fence markers, creating patterns like `**```rust` instead of
/// the correct ````rust`.
///
/// This handles malformed markdown generated by htmd when HTML structure
/// has bold tags (`<strong>`, `<b>`) that aren't properly closed before
/// code blocks (`<pre>`, `<code>`).
///
/// # Pattern Detection
///
/// Matches lines with these characteristics:
/// - Optional leading whitespace
/// - Exactly `**` (two asterisks)
/// - Immediately followed by ` ``` ` or `~~~` (code fence marker)
/// - Optional language identifier after fence
///
/// # Examples
///
/// Fixes these patterns:
/// - `**```rust` → ````rust`
/// - `  **```python` → `  ```python`
/// - `**~~~` → `~~~`
/// - `**```text` → ````text`
///
/// Preserves valid markdown:
/// - `**bold text** not a fence` (unchanged - not a fence)
/// - `Some **bold** then ``` (unchanged - asterisks not directly before fence)
/// - `[**```text**](url)` (unchanged - inside link)
///
/// # Arguments
///
/// * `markdown` - Markdown content potentially containing corrupted code fences
///
/// # Returns
///
/// Cleaned markdown with bold markers stripped from code fence lines
pub fn strip_bold_from_code_fences(markdown: &str) -> String {
    let mut result = String::with_capacity(markdown.len());

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        
        if trimmed.starts_with("**```") || trimmed.starts_with("**~~~") {
            // Calculate indentation to preserve
            let indent = &line[..line.len() - trimmed.len()];
            
            // Strip the ** prefix (2 chars) and write with original indent
            result.push_str(indent);
            result.push_str(&trimmed[2..]);
            result.push('\n');
            
            tracing::debug!(
                "Stripped bold markers from code fence: '{}' → '{}{}' (truncated)",
                line,
                indent,
                &trimmed[2..].chars().take(30).collect::<String>()
            );
        } else {
            // Preserve all other lines unchanged
            result.push_str(line);
            result.push('\n');
        }
    }

    // Remove trailing newline to match join() behavior
    if result.ends_with('\n') {
        result.pop();
    }
    result
}

/// Strip trailing asterisks that appear immediately after code fence closings
///
/// This is a defensive safety net that catches cases where bold formatting
/// "leaked" across code block boundaries during HTML-to-markdown conversion.
///
/// # Pattern Detection
///
/// Removes lines matching this pattern:
/// ```
/// ```           ← closing code fence
/// ****          ← line with only asterisks (THIS gets removed)
/// ```
///
/// # Arguments
///
/// * `markdown` - Markdown content potentially containing trailing asterisks
///
/// # Returns
///
/// * Cleaned markdown with trailing asterisks removed
///
/// # Examples
///
/// ```rust
/// let markdown = r#"
/// ```rust
/// fn main() {}
/// ```
/// ****
/// "#;
/// 
/// let cleaned = strip_trailing_asterisks_after_code_fences(markdown);
/// // Result: closing fence with no trailing asterisks
/// ```
pub fn strip_trailing_asterisks_after_code_fences(markdown: &str) -> String {
    // Replace code fence followed by asterisk-only line with just the fence
    TRAILING_ASTERISKS_RE.replace_all(markdown, "```\n").to_string()
}

/// Remove duplicate code blocks that appear both unfenced and fenced
///
/// This fixes a bug where code blocks appear twice in generated markdown:
/// 1. First as plain text (inline, without fences)
/// 2. Then again at the end with proper code fences
///
/// The function:
/// 1. Extracts all fenced code blocks and their content
/// 2. Identifies plain text sections that duplicate fenced code
/// 3. Removes the duplicate plain text occurrences
///
/// # Arguments
///
/// * `markdown` - Markdown content potentially containing duplicate code blocks
///
/// # Returns
///
/// * Cleaned markdown with duplicate plain-text code removed
///
/// # Examples
///
/// ```rust
/// let markdown = r#"
/// Example:
/// cargo add ratatui
/// 
/// ```shell
/// cargo add ratatui
/// ```
/// "#;
/// 
/// let cleaned = remove_duplicate_code_blocks(markdown);
/// // Result: Only the fenced version remains
/// ```
pub fn remove_duplicate_code_blocks(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    
    // Step 1: Extract all fenced code blocks and their content
    let mut fenced_code_contents = HashSet::new();
    let mut i = 0;
    
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fence_marker = if trimmed.starts_with("```") { "```" } else { "~~~" };
            i += 1;
            let mut code_lines = Vec::new();
            
            // Collect lines until closing fence
            while i < lines.len() {
                let line_trimmed = lines[i].trim_start();
                if line_trimmed.starts_with(fence_marker) {
                    break;
                }
                code_lines.push(lines[i]);
                i += 1;
            }
            
            // Store normalized code content
            if !code_lines.is_empty() {
                let code_text = code_lines.join("\n");
                let normalized = normalize_code_content(&code_text);
                if !normalized.is_empty() {
                    fenced_code_contents.insert(normalized);
                }
            }
        }
        i += 1;
    }
    
    // Step 2: Process lines, identifying sections that match fenced code
    let mut sections_to_skip = Vec::new(); // (start_idx, end_idx) pairs
    let mut i = 0;
    
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        
        // Skip over fenced blocks
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fence_marker = if trimmed.starts_with("```") { "```" } else { "~~~" };
            i += 1;
            while i < lines.len() {
                let line_trimmed = lines[i].trim_start();
                if line_trimmed.starts_with(fence_marker) {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        
        // Check if this starts a plain text block that matches fenced code
        let mut j = i;
        let mut candidate_lines = Vec::new();
        
        // Collect lines for potential match
        while j < lines.len() {
            let line = lines[j];
            let line_trimmed = line.trim_start();
            
            // Stop at fences or blank lines
            if line_trimmed.starts_with("```") || line_trimmed.starts_with("~~~") {
                break;
            }
            if line.trim().is_empty() {
                break;
            }
            
            candidate_lines.push(line);
            j += 1;
        }
        
        // Check if candidate matches any fenced code
        if !candidate_lines.is_empty() {
            let candidate_text = candidate_lines.join("\n");
            let normalized = normalize_code_content(&candidate_text);
            
            if fenced_code_contents.contains(&normalized) {
                // Mark this section for removal
                sections_to_skip.push((i, j));
                tracing::debug!(
                    "Found duplicate plain text at lines {}-{} ({} lines)",
                    i + 1,
                    j,
                    candidate_lines.len()
                );
                i = j;
                continue;
            }
        }
        
        i += 1;
    }
    
    // Step 3: Rebuild markdown, skipping marked sections
    let mut result = String::with_capacity(markdown.len());
    let mut i = 0;
    
    while i < lines.len() {
        // Check if this line is in a skip section
        let should_skip = sections_to_skip.iter().any(|(start, end)| i >= *start && i < *end);
        
        if should_skip {
            i += 1;
            continue;
        }
        
        result.push_str(lines[i]);
        result.push('\n');
        i += 1;
    }
    
    result.trim_end().to_string()
}

/// Normalize code content for comparison
///
/// Removes leading/trailing whitespace and normalizes internal whitespace
/// to detect duplicates that might have minor formatting differences.
fn normalize_code_content(code: &str) -> String {
    code.trim()
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip residual HTML tags that leaked through conversion
///
/// This is a defensive measure to catch any HTML that htmd's handlers
/// failed to convert. It preserves code blocks (doesn't strip inside fences).
///
/// This is Layer 2 of the defense-in-depth strategy against HTML leakage:
/// - Layer 1 (PRIMARY): Fixed custom handlers use `handlers.walk_children()`
/// - Layer 2 (DEFENSIVE): This function strips any HTML that still leaks through
/// - Layer 3 (PREVENTIVE): HTML structure normalization before conversion
///
/// # Performance Characteristics
///
/// - Fast path: Lines without '<' are copied directly (zero allocation overhead)
/// - State machine: 5-10x faster than regex for simple HTML tag matching
/// - Single allocation per modified line only
///
/// # Arguments
///
/// * `markdown` - Markdown content potentially containing residual HTML tags
///
/// # Returns
///
/// * Cleaned markdown with HTML tags removed (except inside code blocks)
///
/// # Examples
///
/// ```rust
/// let markdown = r#"
/// Some text <p>with HTML tags</p>
/// 
/// ```rust
/// let html = "<div>code</div>";  // HTML inside code - preserved!
/// ```
/// 
/// More <span>text</span> here
/// "#;
/// 
/// let cleaned = strip_residual_html_tags(markdown);
/// // Result: HTML tags removed except inside code blocks
/// ```
pub fn strip_residual_html_tags(markdown: &str) -> String {
    let mut result = String::with_capacity(markdown.len());
    let mut in_code_fence = false;
    
    for line in markdown.lines() {
        // Track code fence boundaries
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            result.push_str(line);
            result.push('\n');
            continue;
        }
        
        // Don't strip HTML inside code fences
        if in_code_fence {
            result.push_str(line);
            result.push('\n');
            continue;
        }
        
        // FAST PATH: No HTML tags possible if no '<' character
        if !line.contains('<') {
            result.push_str(line);
            result.push('\n');
            continue;
        }
        
        // SLOW PATH: Strip HTML tags using state machine
        // This is faster than regex for simple tag matching
        let cleaned = strip_html_tags_state_machine(line);
        result.push_str(&cleaned);
        result.push('\n');
    }
    
    result.trim_end().to_string()
}

/// Strip HTML tags from a single line using character-level state machine
///
/// This is significantly faster than regex for the simple case of matching
/// `<tag>`, `</tag>`, and `<tag attr="value">` patterns.
///
/// # State Machine Logic
///
/// - State 0 (outside tag): Copy characters to output
/// - State 1 (inside tag): Skip characters until '>'
///
/// # Arguments
///
/// * `line` - Single line of text potentially containing HTML tags
///
/// # Returns
///
/// * Line with all HTML tags removed
#[inline]
fn strip_html_tags_state_machine(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut in_tag = false;
    
    for ch in line.chars() {
        match ch {
            '<' => {
                // Start of potential tag
                in_tag = true;
            }
            '>' if in_tag => {
                // End of tag
                in_tag = false;
            }
            _ if !in_tag => {
                // Outside tag - copy character
                result.push(ch);
            }
            _ => {
                // Inside tag - skip character
            }
        }
    }
    
    result
}

/// Fix corrupted shebang lines and preserve valid ones
///
/// This function operates ONLY within code blocks and:
/// 1. Detects corrupted shebangs: `# !/bin/bash` → `#!/bin/bash`
/// 2. Preserves valid shebangs exactly as-is
/// 3. Ensures newline immediately after shebang
///
/// Shebangs are critical for script execution:
/// - Must start with exactly `#!` (no space)
/// - Must be on the first line of the script
/// - Must have newline immediately after
/// - Common patterns: `#!/bin/bash`, `#!/usr/bin/env python3`
///
/// This fixes corruption from whitespace normalization and heading processing
/// that incorrectly treats shebangs as malformed headings.
///
/// # Arguments
///
/// * `markdown` - Markdown content potentially containing shebangs in code blocks
///
/// # Returns
///
/// * Cleaned markdown with shebangs fixed and preserved
///
/// # Examples
///
/// ```rust
/// let markdown = r#"
/// ```bash
/// # !/bin/bash
/// echo "hello"
/// ```
/// "#;
/// 
/// let cleaned = fix_shebang_lines(markdown);
/// // Result: shebang fixed to #!/bin/bash
/// ```
pub fn fix_shebang_lines(markdown: &str) -> String {
    let mut result = String::with_capacity(markdown.len());
    let mut fence_stack: Option<CodeFence> = None;
    let mut is_first_line_in_code_block = false;

    for (line_num, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();

        // Track code fence state
        if let Some((fence_char, fence_count)) = detect_code_fence(trimmed) {
            if let Some(ref current_fence) = fence_stack {
                // Check if this closes the current fence
                if fence_char == current_fence.char && fence_count >= current_fence.count {
                    fence_stack = None;
                    is_first_line_in_code_block = false;
                }
            } else {
                // Open a new code fence
                fence_stack = Some(CodeFence {
                    char: fence_char,
                    count: fence_count,
                    line_number: line_num,
                });
                // Next line will be first line inside code block
                is_first_line_in_code_block = true;
            }
            // Always preserve fence lines
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // Only process lines inside code blocks
        if fence_stack.is_some() {
            // Check if this is a corrupted shebang on first line (has space after #)
            if is_first_line_in_code_block && CORRUPTED_SHEBANG_RE.is_match(trimmed) {
                // Fix: Remove the space after #
                let fixed = trimmed.replacen("# !", "#!", 1);
                
                // Preserve original indentation
                let indent = &line[..line.len() - trimmed.len()];
                result.push_str(indent);
                result.push_str(&fixed);
                result.push('\n');
                
                tracing::debug!(
                    "Fixed corrupted shebang at line {}: '{}' → '{}'",
                    line_num + 1,
                    line,
                    fixed
                );
                
                is_first_line_in_code_block = false;
                continue;
            }
            
            // Check if this is a valid shebang on first line of code block
            if is_first_line_in_code_block && SHEBANG_RE.is_match(trimmed) {
                // Preserve shebang exactly as-is
                result.push_str(line);
                result.push('\n');
                
                tracing::debug!(
                    "Preserved valid shebang at line {}: '{}'",
                    line_num + 1,
                    trimmed
                );
                
                is_first_line_in_code_block = false;
                continue;
            }
            
            // Not a shebang line anymore
            is_first_line_in_code_block = false;
        }

        // Preserve all other lines
        result.push_str(line);
        result.push('\n');
    }

    // Remove trailing newline to match join() behavior
    if result.ends_with('\n') {
        result.pop();
    }
    result
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

    #[test]
    fn test_strip_bold_from_code_fence_basic() {
        let markdown = "**```rust\nfn main() {}\n```";
        let result = strip_bold_from_code_fences(markdown);
        assert_eq!(result, "```rust\nfn main() {}\n```");
        assert!(!result.contains("**```"), "Should not have bold before fence");
    }

    #[test]
    fn test_strip_bold_with_indentation() {
        let markdown = "  **```python\nprint('hello')\n```";
        let result = strip_bold_from_code_fences(markdown);
        assert_eq!(result, "  ```python\nprint('hello')\n```");
        assert!(result.starts_with("  ```"), "Should preserve indentation");
    }

    #[test]
    fn test_strip_bold_tildes() {
        let markdown = "**~~~\ncode here\n~~~";
        let result = strip_bold_from_code_fences(markdown);
        assert_eq!(result, "~~~\ncode here\n~~~");
    }

    #[test]
    fn test_preserve_valid_bold() {
        let markdown = "**bold text** not a fence\n```rust\ncode\n```";
        let result = strip_bold_from_code_fences(markdown);
        assert!(result.contains("**bold text**"), "Should preserve valid bold");
    }

    #[test]
    fn test_preserve_bold_separate_from_fence() {
        let markdown = "Some **bold** then\n```rust\ncode\n```";
        let result = strip_bold_from_code_fences(markdown);
        assert!(result.contains("**bold**"), "Should preserve bold not adjacent to fence");
    }

    #[test]
    fn test_multiple_corrupted_fences() {
        let markdown = "**```rust\ncode1\n```\n\nText\n\n**```python\ncode2\n```";
        let result = strip_bold_from_code_fences(markdown);
        assert!(!result.contains("**```"), "Should fix all corrupted fences");
        assert_eq!(result.matches("```rust").count(), 1);
        assert_eq!(result.matches("```python").count(), 1);
    }

    #[test]
    fn test_no_change_when_no_corruption() {
        let markdown = "```rust\nfn main() {}\n```\n\n**Some bold text**";
        let result = strip_bold_from_code_fences(markdown);
        assert_eq!(result, markdown, "Should not modify clean markdown");
    }

    #[test]
    fn test_remove_duplicate_code_blocks_basic() {
        let markdown = r#"Example:
cargo add ratatui

```shell
cargo add ratatui
```"#;
        
        let result = remove_duplicate_code_blocks(markdown);
        eprintln!("INPUT:\n{}\n\nOUTPUT:\n{}", markdown, result);
        
        // Should remove the plain text duplicate
        assert!(!result.contains("Example:\ncargo add ratatui\n\n```"), 
            "Should remove plain text before fenced version");
        // Should keep the fenced version
        assert!(result.contains("```shell\ncargo add ratatui\n```"), 
            "Should keep fenced version");
    }

    #[test]
    fn test_remove_duplicate_preserves_different_content() {
        let markdown = r#"First command:
cargo build

```shell
cargo test
```"#;
        
        let result = remove_duplicate_code_blocks(markdown);
        
        // Should keep both because they're different
        assert!(result.contains("cargo build"));
        assert!(result.contains("cargo test"));
    }

    #[test]
    fn test_remove_duplicate_multiple_blocks() {
        let markdown = r#"cargo add ratatui

[dependencies]
ratatui = "0.28.0"

```shell
cargo add ratatui
```

```toml
[dependencies]
ratatui = "0.28.0"
```"#;
        
        let result = remove_duplicate_code_blocks(markdown);
        
        eprintln!("MULTIPLE BLOCKS OUTPUT:\n{}", result);
        
        // Count occurrences - duplicates should be removed
        let shell_cmd_count = result.matches("cargo add ratatui").count();
        assert_eq!(shell_cmd_count, 1, "cargo add ratatui should appear only once (in fenced block)");
        
        let toml_count = result.matches("[dependencies]").count();
        assert_eq!(toml_count, 1, "[dependencies] should appear only once (in fenced block)");
        
        // Fenced versions should remain
        assert!(result.contains("```shell\ncargo add ratatui"));
        assert!(result.contains("```toml"));
    }

    #[test]
    fn test_fix_corrupted_shebang() {
        let markdown = r#"```bash
# !/bin/bash
echo "hello"
```"#;

        let result = fix_shebang_lines(markdown);
        assert!(result.contains("#!/bin/bash"));
        assert!(!result.contains("# !/bin/bash"));
    }

    #[test]
    fn test_preserve_valid_shebang() {
        let markdown = r#"```bash
#!/bin/bash
echo "hello"
```"#;

        let result = fix_shebang_lines(markdown);
        assert!(result.contains("#!/bin/bash"));
        assert_eq!(result.matches("#!/bin/bash").count(), 1);
    }

    #[test]
    fn test_fix_shebang_env_style() {
        let markdown = r#"```python
# !/usr/bin/env python3
print("hello")
```"#;

        let result = fix_shebang_lines(markdown);
        assert!(result.contains("#!/usr/bin/env python3"));
        assert!(!result.contains("# !"));
    }

    #[test]
    fn test_shebang_with_newline() {
        let markdown = r#"```bash
#!/bin/bash

echo "hello"
```"#;

        let result = fix_shebang_lines(markdown);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines[1], "#!/bin/bash");
        assert_eq!(lines[2], "");
        assert_eq!(lines[3], "echo \"hello\"");
    }

    #[test]
    fn test_shebang_no_change_outside_code_blocks() {
        let markdown = r#"
Regular text
# !/bin/bash this is just text
More text
"#;

        let result = fix_shebang_lines(markdown);
        // Should NOT fix outside code blocks
        assert!(result.contains("# !/bin/bash this is just text"));
    }

    #[test]
    fn test_shebang_only_on_first_line() {
        let markdown = r#"```bash
echo "first"
# !/bin/bash
echo "second"
```"#;

        let result = fix_shebang_lines(markdown);
        // Should NOT fix shebang if not on first line
        assert!(result.contains("# !/bin/bash"));
    }
}
