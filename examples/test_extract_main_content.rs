use kodegen_tools_citescrape::content_saver::markdown_converter::html_preprocessing::extract_main_content;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test with HTML containing links (data-citescrape-interactive attribute removed)
    let html = r#"<html><body><li>A <a class="link" href="https://claude.ai" rel="noreferrer" target="_blank">Claude.ai</a> (recommended) or <a class="link" href="https://console.anthropic.com/" rel="noreferrer" target="_blank">Claude Console</a> account</li></body></html>"#;
    
    println!("=== INPUT HTML ===");
    println!("{}", html);
    println!();
    
    // Call the actual extract_main_content function
    let result = extract_main_content(html)?;
    
    println!("=== OUTPUT HTML (after extract_main_content) ===");
    println!("{}", result);
    println!();
    
    // Check if link text is preserved
    if result.contains("Claude.ai") && result.contains("Claude Console") {
        println!("✅ SUCCESS: Link text preserved by extract_main_content!");
    } else {
        println!("❌ FAILURE: Link text lost in extract_main_content!");
        if result.contains("<a") {
            println!("   Links are present but text is missing");
            println!();
            println!("=== DEBUGGING: Show what's in the <a> tags ===");
            // Find and display the anchor tags
            for (i, line) in result.lines().enumerate() {
                if line.contains("<a") {
                    println!("Line {}: {}", i, line);
                }
            }
        }
    }
    
    Ok(())
}
