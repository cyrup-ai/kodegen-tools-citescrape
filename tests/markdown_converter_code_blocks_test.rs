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

/// Test shell commands preserve critical spaces around operators
/// This is for Task 019: Echo and Heredoc Shell Commands Mangled
#[test]
fn test_shell_commands_preserve_spaces() {
    // Test 1: Basic echo with redirect
    let html1 = r#"<pre><code class="language-bash">echo 'Hello World' > output.txt</code></pre>"#;
    let result1 = convert_html_to_markdown_sync(html1, &ConversionOptions::default()).unwrap();
    
    println!("Test 1 - Basic Echo:");
    println!("HTML: {}", html1);
    println!("Result:\n{}\n", result1);
    
    // Critical: Must have space after 'echo' and before '>'
    assert!(result1.contains("echo '"), "Should have space after 'echo', got: {}", result1);
    assert!(result1.contains("' >"), "Should have space before '>', got: {}", result1);
    assert!(!result1.contains("echo'"), "Should NOT have 'echo' merged with quote, got: {}", result1);
    assert!(!result1.contains("'>"), "Should NOT have quote merged with '>', got: {}", result1);
    
    // Test 2: Multi-line echo
    let html2 = r#"<pre><code class="language-bash">echo '---
name: test
description: Test agent
---
' > test.md</code></pre>"#;
    let result2 = convert_html_to_markdown_sync(html2, &ConversionOptions::default()).unwrap();
    
    println!("Test 2 - Multi-line Echo:");
    println!("Result:\n{}\n", result2);
    
    assert!(result2.contains("echo '"), "Multi-line echo should have space after 'echo'");
    assert!(result2.contains("' >"), "Multi-line echo should have space before '>'");
    
    // Test 3: Heredoc
    let html3 = r#"<pre><code class="language-bash">cat << EOF > config.yaml
key: value
nested:
  item: data
EOF</code></pre>"#;
    let result3 = convert_html_to_markdown_sync(html3, &ConversionOptions::default()).unwrap();
    
    println!("Test 3 - Heredoc:");
    println!("Result:\n{}\n", result3);
    
    assert!(result3.contains(" << "), "Heredoc should have spaces around '<<'");
    assert!(result3.contains(" > "), "Heredoc should have spaces around '>'");
    
    // Test 4: Pipeline with redirects
    let html4 = r#"<pre><code class="language-bash">echo 'data' | grep 'pattern' > results.txt</code></pre>"#;
    let result4 = convert_html_to_markdown_sync(html4, &ConversionOptions::default()).unwrap();
    
    println!("Test 4 - Pipeline:");
    println!("Result:\n{}\n", result4);
    
    assert!(result4.contains(" | "), "Pipeline should have spaces around '|'");
    assert!(result4.contains(" > "), "Pipeline should have space before '>'");
}
