use kodegen_tools_citescrape::content_saver::markdown_converter::{convert_html_to_markdown_sync, ConversionOptions};

/// Test that code blocks preserve newlines instead of collapsing them to spaces.
///
/// This tests the fix for the html2md fork bug where checking parent_chain for "code"
/// before "pre" caused `inside_pre` to never be set, resulting in whitespace minification.
#[test]
fn test_code_block_preserves_newlines() {
    let html = include_str!("fixtures/code_block_newlines.html");
    let options = ConversionOptions::default();
    let markdown = convert_html_to_markdown_sync(html, &options).unwrap();

    // Debug: print the actual markdown output
    println!("Markdown output:\n{}", markdown);
    println!("\nChecking for newline preservation...");

    // Should have newlines between lines, not spaces
    // The code should be formatted like:
    // ```rust
    // pub fn main() -> io::Result<()> {
    //     init_panic_hook();
    //     ...
    // }
    // ```

    // Check that the code is in a fenced code block
    assert!(markdown.contains("```"), "Should have code fence markers");

    // Check that newlines are preserved (not all on one line)
    // If working correctly, there should be a newline after the opening brace
    assert!(
        markdown.contains("{\n    init_panic_hook();") ||
        markdown.contains("{\r\n    init_panic_hook();"),
        "Code should have newlines preserved, not: {:?}",
        markdown
    );

    // Verify the bug is fixed: should NOT be all on one line with spaces
    assert!(
        !markdown.contains("{    init_panic_hook();    let mut tui"),
        "Code should NOT be on one line with spaces instead of newlines"
    );
}

/// Test that regular pre tags (without code) also preserve whitespace
#[test]
fn test_pre_tag_preserves_whitespace() {
    let html = r#"<pre>Line 1
Line 2
    Indented line 3</pre>"#;

    let options = ConversionOptions::default();
    let markdown = convert_html_to_markdown_sync(html, &options).unwrap();

    println!("Pre tag markdown output:\n{}", markdown);

    // Should preserve the newlines
    assert!(
        markdown.contains("Line 1\nLine 2") || markdown.contains("Line 1\r\nLine 2"),
        "Pre tag should preserve newlines"
    );
    assert!(
        markdown.contains("    Indented line 3"),
        "Pre tag should preserve indentation"
    );
}
