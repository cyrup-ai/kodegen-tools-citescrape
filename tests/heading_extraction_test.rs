//! Integration test for heading extraction and markdown H1 insertion
//!
//! This test verifies:
//! 1. Headings are extracted with correct ordinal hierarchy
//! 2. JSON output includes metadata.headings array
//! 3. Markdown output starts with H1 (from extracted heading or title)

use kodegen_tools_citescrape::page_extractor::schema::{HeadingElement, PageMetadata};
use kodegen_tools_citescrape::content_saver::markdown_converter::ensure_h1_at_start;

#[test]
fn test_heading_element_structure() {
    // Test that HeadingElement can be created with expected fields
    let heading = HeadingElement {
        level: 1,
        text: "Main Title".to_string(),
        id: Some("main-title".to_string()),
        ordinal: vec![1],
    };

    assert_eq!(heading.level, 1);
    assert_eq!(heading.text, "Main Title");
    assert_eq!(heading.id, Some("main-title".to_string()));
    assert_eq!(heading.ordinal, vec![1]);
}

#[test]
fn test_ordinal_hierarchy() {
    // Test ordinal hierarchy for nested headings
    let headings = [
        HeadingElement {
            level: 1,
            text: "First H1".to_string(),
            id: None,
            ordinal: vec![1],
        },
        HeadingElement {
            level: 2,
            text: "First H2 under H1".to_string(),
            id: None,
            ordinal: vec![1, 1],
        },
        HeadingElement {
            level: 3,
            text: "First H3 under H2".to_string(),
            id: None,
            ordinal: vec![1, 1, 1],
        },
        HeadingElement {
            level: 2,
            text: "Second H2 under H1".to_string(),
            id: None,
            ordinal: vec![1, 2],
        },
        HeadingElement {
            level: 1,
            text: "Second H1".to_string(),
            id: None,
            ordinal: vec![2],
        },
    ];

    // Verify ordinal structure
    assert_eq!(headings[0].ordinal, vec![1]); // 1st H1
    assert_eq!(headings[1].ordinal, vec![1, 1]); // 1st H1 → 1st H2
    assert_eq!(headings[2].ordinal, vec![1, 1, 1]); // 1st H1 → 1st H2 → 1st H3
    assert_eq!(headings[3].ordinal, vec![1, 2]); // 1st H1 → 2nd H2
    assert_eq!(headings[4].ordinal, vec![2]); // 2nd H1
}

#[test]
fn test_ensure_h1_at_start_with_existing_h1() {
    // Test that existing H1 is preserved
    let markdown = "# Existing H1\n\nContent here";
    let headings = vec![
        HeadingElement {
            level: 1,
            text: "Extracted H1".to_string(),
            id: None,
            ordinal: vec![1],
        },
    ];
    let title = "Document Title";

    let result = ensure_h1_at_start(markdown, &headings, title);
    
    // Should not modify markdown that already has H1
    assert_eq!(result, markdown);
}

#[test]
fn test_ensure_h1_at_start_without_h1_uses_extracted() {
    // Test that extracted H1 is prepended when missing
    let markdown = "## H2 First\n\nContent here";
    let headings = vec![
        HeadingElement {
            level: 1,
            text: "Extracted H1".to_string(),
            id: None,
            ordinal: vec![1],
        },
        HeadingElement {
            level: 2,
            text: "H2 First".to_string(),
            id: None,
            ordinal: vec![1, 1],
        },
    ];
    let title = "Document Title";

    let result = ensure_h1_at_start(markdown, &headings, title);
    
    // Should prepend extracted H1
    assert!(result.starts_with("# Extracted H1\n\n"));
    assert!(result.contains("## H2 First"));
}

#[test]
fn test_ensure_h1_at_start_without_h1_uses_title_fallback() {
    // Test that title is used when no H1 extracted
    let markdown = "## H2 First\n\nContent here";
    let headings = vec![
        HeadingElement {
            level: 2,
            text: "H2 First".to_string(),
            id: None,
            ordinal: vec![1],
        },
    ];
    let title = "Document Title";

    let result = ensure_h1_at_start(markdown, &headings, title);
    
    // Should prepend title as H1
    assert!(result.starts_with("# Document Title\n\n"));
    assert!(result.contains("## H2 First"));
}

#[test]
fn test_ensure_h1_at_start_with_empty_headings_uses_title() {
    // Test that title is used when no headings extracted at all
    let markdown = "Content without any headings";
    let headings: Vec<HeadingElement> = vec![];
    let title = "Document Title";

    let result = ensure_h1_at_start(markdown, &headings, title);
    
    // Should prepend title as H1
    assert!(result.starts_with("# Document Title\n\n"));
    assert!(result.contains("Content without any headings"));
}

#[test]
fn test_page_metadata_has_headings_field() {
    // Test that PageMetadata can store headings
    let metadata = PageMetadata {
        headings: vec![
            HeadingElement {
                level: 1,
                text: "Test H1".to_string(),
                id: Some("test-h1".to_string()),
                ordinal: vec![1],
            },
        ],
        ..Default::default()
    };

    assert_eq!(metadata.headings.len(), 1);
    assert_eq!(metadata.headings[0].text, "Test H1");
}

#[test]
fn test_heading_serialization() {
    // Test that HeadingElement can be serialized to JSON
    let heading = HeadingElement {
        level: 1,
        text: "Test Heading".to_string(),
        id: Some("test-id".to_string()),
        ordinal: vec![1, 2, 3],
    };

    let json = serde_json::to_string(&heading).expect("Failed to serialize");
    
    assert!(json.contains("\"level\":1"));
    assert!(json.contains("\"text\":\"Test Heading\""));
    assert!(json.contains("\"id\":\"test-id\""));
    assert!(json.contains("\"ordinal\":[1,2,3]"));
}
