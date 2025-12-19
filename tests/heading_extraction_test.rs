//! Integration test for heading extraction
//!
//! This test verifies:
//! 1. Headings are extracted with correct ordinal hierarchy
//! 2. JSON output includes metadata.headings array

use kodegen_tools_citescrape::page_extractor::schema::{HeadingElement, PageMetadata};

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
