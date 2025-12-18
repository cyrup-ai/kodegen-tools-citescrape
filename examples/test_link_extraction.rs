use scraper::Html;

fn main() {
    // Test with HTML containing links (data-citescrape-interactive attribute removed)
    let html = r#"<html><body><li>A <a class="link" href="https://claude.ai" rel="noreferrer" target="_blank">Claude.ai</a> (recommended) or <a class="link" href="https://console.anthropic.com/" rel="noreferrer" target="_blank">Claude Console</a> account</li></body></html>"#;
    
    println!("=== INPUT HTML ===");
    println!("{}", html);
    println!();
    
    // Parse with scraper (same library used by citescrape)
    let document = Html::parse_document(html);
    let result = document.root_element().html();
    
    println!("=== OUTPUT HTML (after scraper parse + serialize) ===");
    println!("{}", result);
    println!();
    
    // Check if link text is preserved
    if result.contains("Claude.ai") && result.contains("Claude Console") {
        println!("✅ SUCCESS: Link text preserved after scraper parse!");
    } else {
        println!("❌ FAILURE: Link text lost during scraper parse!");
        if result.contains("<a") {
            println!("   Links are present but text is missing");
        }
    }
}
