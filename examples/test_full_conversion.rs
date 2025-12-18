use kodegen_tools_citescrape::content_saver::markdown_converter::{
    convert_html_to_markdown, ConversionOptions,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read the actual HTML file that has the problem
    let html = std::fs::read_to_string("docs/code.claude.com/code.claude.com/docs/en/index.html")?;
    
    // Extract just the problematic line (line 1005)
    let problem_line = html.lines().nth(1004).unwrap_or("");
    
    println!("=== PROBLEMATIC HTML LINE (line 1005) ===");
    println!("{}", problem_line);
    println!();
    
    // Create a minimal test HTML with just this line
    let test_html = format!(r#"<html><body>{}</body></html>"#, problem_line);
    
    // Run through the full conversion pipeline
    let options = ConversionOptions {
        base_url: Some("https://code.claude.com/docs/en/index".to_string()),
        ..ConversionOptions::default()
    };
    
    let markdown = convert_html_to_markdown(&test_html, &options).await?;
    
    println!("=== CONVERTED MARKDOWN ===");
    println!("{}", markdown);
    println!();
    
    // Check if link text is preserved
    if markdown.contains("Claude.ai") && markdown.contains("Claude Console") {
        println!("✅ SUCCESS: Link text preserved in full conversion!");
    } else {
        println!("❌ FAILURE: Link text lost in full conversion!");
        println!();
        println!("Expected to find: 'Claude.ai' and 'Claude Console'");
        println!("Actual markdown: {}", markdown);
    }
    
    Ok(())
}
