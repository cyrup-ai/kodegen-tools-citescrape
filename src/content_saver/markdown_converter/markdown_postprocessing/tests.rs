//! Tests for markdown postprocessing modules.

use super::heading_extraction::extract_heading_level;
use super::processor::process_markdown_headings;

#[test]
fn test_extract_heading_level_with_closing_hashes() {
    // Normal case with closing hashes
    let result = extract_heading_level("## Title ##");
    assert_eq!(result, Some((2, "Title")));

    // H1 with closing hashes
    let result = extract_heading_level("# Main Heading #");
    assert_eq!(result, Some((1, "Main Heading")));

    // H3 with closing hashes
    let result = extract_heading_level("### Section ###");
    assert_eq!(result, Some((3, "Section")));
}

#[test]
fn test_extract_heading_level_unmatched_closing_hashes() {
    // More closing hashes than opening (valid per CommonMark)
    let result = extract_heading_level("## Title #####");
    assert_eq!(result, Some((2, "Title")));

    // Fewer closing hashes than opening
    let result = extract_heading_level("#### Title ##");
    assert_eq!(result, Some((4, "Title")));
}

#[test]
fn test_extract_heading_level_hash_in_content() {
    // Content ends with hash (e.g., C#)
    let result = extract_heading_level("## C# ##");
    assert_eq!(result, Some((2, "C#")));

    // Multiple hashes in content
    let result = extract_heading_level("## Use C# and F# ##");
    assert_eq!(result, Some((2, "Use C# and F#")));

    // Hash not at end
    let result = extract_heading_level("## Title#content ##");
    assert_eq!(result, Some((2, "Title#content")));
}

#[test]
fn test_extract_heading_level_no_closing_hashes() {
    // Normal heading without closing hashes
    let result = extract_heading_level("## Title");
    assert_eq!(result, Some((2, "Title")));

    // With extra spaces
    let result = extract_heading_level("###  Spaced Title  ");
    assert_eq!(result, Some((3, "Spaced Title  ")));
}

#[test]
fn test_extract_heading_level_edge_cases() {
    // Single character content
    let result = extract_heading_level("## A ##");
    assert_eq!(result, Some((2, "A")));

    // Empty content (just hashes)
    // Should return empty content, not None
    // Actually, with our implementation, rfind returns None for all-hash/whitespace
    let result = extract_heading_level("## ##");
    assert_eq!(result, Some((2, "")));

    // Maximum heading level
    let result = extract_heading_level("###### Level 6 ######");
    assert_eq!(result, Some((6, "Level 6")));
}

#[test]
fn test_extract_heading_level_no_space_before_closing() {
    // Per CommonMark, closing hashes MUST be preceded by whitespace
    // Without space, the trailing ## is part of the content
    let result = extract_heading_level("##Title##");
    assert_eq!(result, Some((2, "Title##")));
}

#[test]
fn test_extract_heading_level_invalid_headings() {
    // Not a heading
    let result = extract_heading_level("Regular text");
    assert_eq!(result, None);

    // Too many hashes (> 6)
    let result = extract_heading_level("####### Not a heading");
    assert_eq!(result, None);

    // Empty string
    let result = extract_heading_level("");
    assert_eq!(result, None);
}

#[test]
fn test_extract_heading_level_whitespace_variations() {
    // Multiple spaces before closing hashes
    let result = extract_heading_level("## Title     ##");
    assert_eq!(result, Some((2, "Title")));

    // Tab before closing hashes
    let result = extract_heading_level("## Title\t##");
    assert_eq!(result, Some((2, "Title")));

    // Mixed whitespace
    let result = extract_heading_level("##  Title  \t ##");
    assert_eq!(result, Some((2, "Title")));
}

#[test]
fn test_process_markdown_preserves_content_before_h1() {
    // Test that content before the first H1 is preserved (bug fix for HTML_CLEANER_15)
    let markdown = "This is an introduction paragraph.\n\nSome important information here.\n\n# First Heading\n\nArticle content...";
    let result = process_markdown_headings(markdown);

    // Verify introduction is preserved
    assert!(result.contains("This is an introduction paragraph."));
    assert!(result.contains("Some important information here."));
    assert!(result.contains("# First Heading"));
    assert!(result.contains("Article content..."));
}

#[test]
fn test_process_markdown_with_no_h1() {
    // Test that all content is preserved when there's no H1
    let markdown = "Some content\n\n## Subheading\n\nMore content";
    let result = process_markdown_headings(markdown);

    assert!(result.contains("Some content"));
    assert!(result.contains("## Subheading"));
    assert!(result.contains("More content"));
}

#[test]
fn test_process_markdown_with_setext_h1() {
    // Test that content before setext-style H1 is preserved
    let markdown = "Introduction text\n\nFirst Heading\n=============\n\nContent after heading";
    let result = process_markdown_headings(markdown);

    assert!(result.contains("Introduction text"));
    assert!(result.contains("# First Heading")); // Should be converted to ATX style
    assert!(result.contains("Content after heading"));
}

#[test]
fn test_tilde_code_fence_tracking() {
    // Test that headings inside tilde code fences are not processed
    let markdown = "# Real Heading\n\n~~~\n# This is not a heading, it's code\n## Also not a heading\n~~~\n\n# Another Real Heading";
    let result = process_markdown_headings(markdown);

    // Real headings should be normalized
    assert!(result.contains("# Real Heading"));
    assert!(result.contains("# Another Real Heading"));

    // Headings inside tilde fence should remain unchanged
    assert!(result.contains("# This is not a heading, it's code"));
    assert!(result.contains("## Also not a heading"));

    // The fence markers should be preserved
    assert!(result.contains("~~~"));
}

#[test]
fn test_mixed_backtick_and_tilde_fences() {
    // Test that backtick and tilde fences work independently
    let markdown = "# Heading\n\n```\n## Not a heading (backticks)\n```\n\n~~~\n### Not a heading (tildes)\n~~~\n\n## Real Heading 2";
    let result = process_markdown_headings(markdown);

    // Real headings should be present
    assert!(result.contains("# Heading"));
    assert!(result.contains("## Real Heading 2"));

    // Code inside fences should be unchanged
    assert!(result.contains("## Not a heading (backticks)"));
    assert!(result.contains("### Not a heading (tildes)"));
}

#[test]
fn test_tilde_fence_does_not_close_backtick_fence() {
    // Test that tilde fence inside backtick fence is treated as content
    let markdown = "```\n~~~\n# Not a heading\n~~~\n```\n\n# Real Heading";
    let result = process_markdown_headings(markdown);

    // The heading inside should remain unchanged because it's inside backtick fence
    assert!(result.contains("# Not a heading"));
    assert!(result.contains("# Real Heading"));

    // Both fence types should be preserved
    assert!(result.contains("```"));
    assert!(result.contains("~~~"));
}

#[test]
fn test_backtick_fence_does_not_close_tilde_fence() {
    // Test that backtick fence inside tilde fence is treated as content
    let markdown = "~~~\n```\n## Not a heading\n```\n~~~\n\n## Real Heading";
    let result = process_markdown_headings(markdown);

    // The heading inside should remain unchanged because it's inside tilde fence
    assert!(result.contains("## Not a heading"));
    assert!(result.contains("## Real Heading"));

    // Both fence types should be preserved
    assert!(result.contains("```"));
    assert!(result.contains("~~~"));
}

#[test]
fn test_unbalanced_code_fence_with_proper_tracking() {
    // Test the bug fix for HTML_CLEANER_17: unbalanced code fence should not break processing
    let markdown = "# Real Heading 1\n\nSome content\n\n```\nThis code block is never closed\n\n## Heading 2 - Should NOT be processed (inside fence)\n\n### Heading 3 - Should NOT be processed (inside fence)\n\nMore content";
    let result = process_markdown_headings(markdown);

    // First heading should be normalized
    assert!(result.contains("# Real Heading 1"));

    // Content should be preserved
    assert!(result.contains("Some content"));
    assert!(result.contains("This code block is never closed"));
    assert!(result.contains("More content"));

    // Headings inside unclosed fence should remain unchanged (not processed)
    assert!(result.contains("## Heading 2 - Should NOT be processed (inside fence)"));
    assert!(result.contains("### Heading 3 - Should NOT be processed (inside fence)"));

    // Opening fence should be preserved
    assert!(result.contains("```"));
}

#[test]
fn test_fence_count_matching() {
    // Test that closing fence must have at least as many characters as opening fence
    let markdown = "# Heading 1\n\n`````\nCode block with 5 backticks\n```\nThis does NOT close the fence (only 3 backticks)\n## Not a heading\n```\nStill not closed\n`````\nThis closes it (5 backticks)\n\n## Real Heading 2";
    let result = process_markdown_headings(markdown);

    // Real headings should be normalized
    assert!(result.contains("# Heading 1"));
    assert!(result.contains("## Real Heading 2"));

    // Heading inside the 5-backtick fence should not be processed
    assert!(result.contains("## Not a heading"));

    // All fence markers should be preserved
    assert!(result.contains("`````"));
}

#[test]
fn test_different_fence_types_do_not_match() {
    // Test that backticks don't close tilde fences and vice versa
    let markdown = "# Heading\n\n~~~\nOpened with tildes\n```\nThis does not close it (different char)\n## Not a heading\n```\n~~~\nNow it's closed\n\n## Real Heading";
    let result = process_markdown_headings(markdown);

    // Real headings should be normalized
    assert!(result.contains("# Heading"));
    assert!(result.contains("## Real Heading"));

    // Heading inside tilde fence should not be processed
    assert!(result.contains("## Not a heading"));
}

#[test]
fn test_unclosed_fence_recovery() {
    let markdown = r#"
# Introduction

```python
def example():
    print("Hello")
# Missing closing fence

## Section 2

Real content.
"#;

    let result = process_markdown_headings(markdown);

    // Should auto-close fence (check for closing fence added)
    let fence_count = result.matches("```").count();
    assert!(
        fence_count >= 2,
        "Should have both opening and auto-closed fence"
    );

    // NEW: Verify print statement stays inside fence
    let lines: Vec<&str> = result.lines().collect();
    let opening_idx = lines
        .iter()
        .position(|l| l.contains("```python"))
        .expect("Test operation should succeed");
    let closing_idx = lines
        .iter()
        .rposition(|l| l.trim() == "```")
        .expect("Test operation should succeed");
    let print_idx = lines
        .iter()
        .position(|l| l.contains("print("))
        .expect("Test operation should succeed");

    assert!(
        print_idx > opening_idx && print_idx < closing_idx,
        "print() line should be between fence markers, but fence closes at line {closing_idx} and print is at line {print_idx}"
    );

    // Should process subsequent headings
    assert!(result.contains("## Section 2"));

    // Content should be preserved
    assert!(result.contains("Real content."));
}

#[test]
fn test_unclosed_fence_at_end() {
    let markdown = "# Title\n\n```python\ncode\n";
    let result = process_markdown_headings(markdown);

    // Should auto-close fence at end
    let fence_count = result.matches("```").count();
    assert_eq!(
        fence_count, 2,
        "Should have both opening and auto-closed fence"
    );

    // Title should be preserved
    assert!(result.contains("# Title"));
}

#[test]
fn test_unclosed_fence_detects_various_code_patterns() {
    // Test that fence closes after different code patterns
    let test_cases = vec![
        ("```js\nconsole.log('test')\n\n## Heading", "function call"),
        ("```python\nx = 5\n\n## Heading", "assignment"),
        ("```js\narr[0] = 1\n\n## Heading", "array access"),
        ("```\n    indented line\n\n## Heading", "indented code"),
        ("```python\n# code comment\n\n## Heading", "comment"),
    ];

    for (markdown, description) in test_cases {
        let result = process_markdown_headings(markdown);
        let fence_count = result.matches("```").count();
        assert_eq!(
            fence_count, 2,
            "Should auto-close fence for {description} pattern"
        );
    }
}

#[test]
fn test_unclosed_fence_with_heading_after_code() {
    let markdown = r"
```python
def foo():
    return 42

# Next Section

This is prose text.
";

    let result = process_markdown_headings(markdown);

    // Should auto-close fence
    let fence_count = result.matches("```").count();
    assert_eq!(
        fence_count, 2,
        "Should have both opening and auto-closed fence"
    );

    // Verify heading is NOT inside the fence
    let lines: Vec<&str> = result.lines().collect();
    let closing_idx = lines
        .iter()
        .rposition(|l| l.trim() == "```")
        .expect("Test operation should succeed");
    let heading_idx = lines
        .iter()
        .position(|l| l.contains("# Next Section"))
        .expect("Test operation should succeed");

    assert!(
        heading_idx > closing_idx,
        "Heading should be AFTER closing fence (outside fence), but fence closes at line {closing_idx} and heading is at line {heading_idx}"
    );
}


// =============================================================================
// Shell Operator Spacing Tests
// =============================================================================

#[test]
fn test_pipe_operator_spacing() {
    use crate::content_saver::markdown_converter::markdown_postprocessing::repair_shell_syntax;
    
    let markdown = r#"```bash
curl-fsSLhttps://claude.ai/install.sh|bash
```"#;

    let result = repair_shell_syntax(markdown);
    
    eprintln!("INPUT:\n{}", markdown);
    eprintln!("OUTPUT:\n{}", result);
    
    // Should have proper spacing around pipe
    assert!(result.contains("| bash"), "Pipe operator should have spacing. Got: {}", result);
}

#[test]
fn test_all_shell_operators_spacing() {
    use crate::content_saver::markdown_converter::markdown_postprocessing::repair_shell_syntax;
    
    let markdown = r#"```bash
command1|command2
command1>output.txt
command1>>append.txt
command1<input.txt
command1&
command1&&command2
command1||command2
```"#;

    let result = repair_shell_syntax(markdown);
    
    eprintln!("OUTPUT:\n{}", result);
    
    // All operators should have proper spacing
    assert!(result.contains("command1 | command2"), "Pipe should have spacing");
    assert!(result.contains("command1 > output.txt"), "Redirect out should have spacing");
    assert!(result.contains("command1 >> append.txt"), "Redirect append should have spacing");
    assert!(result.contains("command1 < input.txt"), "Redirect in should have spacing");
    assert!(result.contains("command1 &"), "Background should have spacing (after command1)");
    assert!(result.contains("command1 && command2"), "Double ampersand should have spacing");
    assert!(result.contains("command1 || command2"), "Double pipe should have spacing");
}

#[test]
fn test_only_shell_blocks_affected() {
    use crate::content_saver::markdown_converter::markdown_postprocessing::repair_shell_syntax;
    
    // Non-shell code blocks should not be affected
    let markdown = r#"```rust
let result = items.iter().map(|x| x * 2).collect();
if x>5 { }
```"#;

    let result = repair_shell_syntax(markdown);
    
    eprintln!("OUTPUT:\n{}", result);
    
    // Rust code should NOT be modified (no language tag means not shell)
    assert!(result.contains("|x|"), "Rust closure syntax should be preserved");
    assert!(result.contains("x>5"), "Rust comparison should be preserved without spacing");
}

#[test]
fn test_shell_bracket_and_pipe_operators_together() {
    use crate::content_saver::markdown_converter::markdown_postprocessing::repair_shell_syntax;
    
    let markdown = r#"```bash
if[-f"file.txt"];then
    cat file.txt|grep"pattern"
fi
```"#;

    let result = repair_shell_syntax(markdown);
    
    eprintln!("OUTPUT:\n{}", result);
    
    // Should fix both bracket spacing AND pipe spacing
    assert!(result.contains("if [ -f \"file.txt\" ]"), "Bracket spacing should be fixed");
    assert!(result.contains("]; then"), "Semicolon spacing should be fixed");
    assert!(result.contains("| grep"), "Pipe spacing should be fixed");
}

#[test]
fn test_idempotent_shell_repair() {
    use crate::content_saver::markdown_converter::markdown_postprocessing::repair_shell_syntax;
    
    let markdown = r#"```bash
command1 | command2
command1 > file.txt
```"#;

    let result1 = repair_shell_syntax(markdown);
    let result2 = repair_shell_syntax(&result1);
    
    // Running twice should produce same result (idempotent)
    assert_eq!(result1, result2, "Repair should be idempotent");
}
