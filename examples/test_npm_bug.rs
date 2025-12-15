use kodegen_tools_citescrape::content_saver::markdown_converter::{convert_html_to_markdown_sync, ConversionOptions};

fn main() {
    // Simulate the exact HTML structure from code.claude.com
    let html = r#"
        <html>
        <body>
            <article>
                <h2>Install Claude Code</h2>
                <pre><code class="language-bash">npm install -g @anthropic-ai/claude-code</code></pre>
                <pre><code class="language-bash">curl -fsSL https://claude.ai/install.sh | bash</code></pre>
            </article>
        </body>
        </html>
    "#;
    
    let options = ConversionOptions::default();
    let markdown = convert_html_to_markdown_sync(html, &options).unwrap();
    
    println!("=== OUTPUT MARKDOWN ===");
    println!("{}", markdown);
    println!("======================");
    
    // Check for the bug
    if markdown.contains("npminstall") {
        println!("❌ BUG CONFIRMED: Spaces stripped from 'npm install'");
    } else if markdown.contains("npm install") {
        println!("✅ PASS: Spaces preserved in 'npm install'");
    } else {
        println!("⚠️  UNKNOWN: 'npm install' not found in output");
    }
    
    if markdown.contains("curl-fsSL") {
        println!("❌ BUG CONFIRMED: Spaces stripped from 'curl -fsSL'");
    } else if markdown.contains("curl -fsSL") {
        println!("✅ PASS: Spaces preserved in 'curl -fsSL'");
    } else {
        println!("⚠️  UNKNOWN: 'curl' not found in output");
    }
}
