//! Quick test to verify kuchiki preserves <a> tags during parse/serialize

#[cfg(test)]
mod tests {
    use kuchiki::traits::TendrilSink;

    #[test]
    fn test_kuchiki_preserves_anchor_tags() {
        let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<h1>Hello</h1>
<p>Check out <a href="https://example.com">this link</a> and <a href="/local">local link</a>.</p>
<a href="https://another.com" class="btn">Button Link</a>
</body>
</html>"#;

        // Parse and serialize with kuchiki
        let document = kuchiki::parse_html().one(html);
        
        let mut output = Vec::new();
        document.serialize(&mut output).expect("Failed to serialize");
        let result = String::from_utf8(output).expect("Invalid UTF-8");

        println!("=== INPUT ===\n{html}\n");
        println!("=== OUTPUT ===\n{result}\n");

        // Verify <a> tags are preserved
        // Note: kuchiki may reorder attributes alphabetically (class before href)
        assert!(result.contains("<a href=\"https://example.com\">"), "Missing first anchor tag");
        assert!(result.contains("<a href=\"/local\">"), "Missing second anchor tag");
        // Third anchor has class attribute - kuchiki reorders to class before href
        assert!(result.contains("href=\"https://another.com\""), "Missing third anchor tag href");
        assert!(result.contains("Button Link</a>"), "Missing third anchor tag content");
        assert!(result.contains("</a>"), "Missing closing anchor tags");
    }

    #[test]
    fn test_kuchiki_with_apply_all_replacements_style_flow() {
        // This simulates what apply_all_replacements does
        let html = r#"<!DOCTYPE html>
<html>
<head>
<link rel="stylesheet" href="/styles.css">
<title>Test</title>
</head>
<body>
<a href="https://example.com">Link 1</a>
<img src="/image.png">
<a href="https://example2.com">Link 2</a>
</body>
</html>"#;

        let document = kuchiki::parse_html().one(html);

        // Simulate CSS replacement (what apply_all_replacements does)
        let css_selector = "link[rel=\"stylesheet\"]";
        if let Ok(matches) = document.select(css_selector) {
            let matches: Vec<_> = matches.collect();
            for node_ref in matches {
                let node = node_ref.as_node();
                
                // Create style element
                let style_html = "<style type=\"text/css\">\nbody { color: red; }\n</style>";
                let style_fragment = kuchiki::parse_html().one(style_html);
                
                // Insert before link
                for child in style_fragment.children() {
                    node.insert_before(child);
                }
                
                // Remove link
                node.detach();
            }
        }

        // Serialize
        let mut output = Vec::new();
        document.serialize(&mut output).expect("Failed to serialize");
        let result = String::from_utf8(output).expect("Invalid UTF-8");

        println!("=== AFTER CSS REPLACEMENT ===\n{result}\n");

        // Check if <a> tags survived
        assert!(result.contains("<a href=\"https://example.com\">"), "Missing first anchor after CSS replacement");
        assert!(result.contains("<a href=\"https://example2.com\">"), "Missing second anchor after CSS replacement");
    }
}
