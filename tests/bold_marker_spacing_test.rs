/// Test suite for Task 014: Bold Marker Spacing Fix
///
/// This test verifies that the `fix_bold_internal_spacing()` function correctly
/// handles all the problematic bold marker spacing patterns identified in real
/// crawled content from code.claude.com.
///
/// Test coverage:
/// - Spaces on both sides: `** text **` → `**text**`
/// - Space before closing: `**text **` → `**text**`
/// - Space after opening: `** text**` → `**text**`
/// - Space before punctuation: `**text** :` → `**text**:`
/// - All specific examples from the task document
/// - Edge cases: multi-word bold, internal punctuation, list items
use kodegen_tools_citescrape::content_saver::markdown_converter::markdown_postprocessing;

#[test]
fn test_spaces_on_both_sides() {
    let input = "This is ** text ** with spaces.";
    let expected = "This is **text** with spaces.";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix spaces on both sides");
}

#[test]
fn test_space_before_closing_marker() {
    let input = "This is **text ** with space before closing.";
    let expected = "This is **text** with space before closing.";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix space before closing marker");
}

#[test]
fn test_space_after_opening_marker() {
    let input = "This is ** text** with space after opening.";
    let expected = "This is **text** with space after opening.";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix space after opening marker");
}

#[test]
fn test_space_before_punctuation_colon() {
    let input = "**text** : followed by colon";
    let expected = "**text**: followed by colon";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to remove space before colon");
}

#[test]
fn test_space_before_punctuation_comma() {
    let input = "**text** , followed by comma";
    let expected = "**text**, followed by comma";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to remove space before comma");
}

#[test]
fn test_space_before_punctuation_period() {
    let input = "**text** . followed by period";
    let expected = "**text**. followed by period";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to remove space before period");
}

#[test]
fn test_example_1_from_task() {
    // Example 1: Space before closing bold marker
    // - **Implement features from issue trackers **: "Add the feature..."
    let input = "- **Implement features from issue trackers **: \"Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub.\"";
    let expected = "- **Implement features from issue trackers**: \"Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub.\"";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix Example 1 from task");
}

#[test]
fn test_example_2_from_task() {
    // Example 2: Space after opening bold marker
    // - ** Analyze monitoring data **: "Check Sentry and Statsig..."
    let input = "- ** Analyze monitoring data **: \"Check Sentry and Statsig to check the usage of the feature described in ENG-4521.\"";
    let expected = "- **Analyze monitoring data**: \"Check Sentry and Statsig to check the usage of the feature described in ENG-4521.\"";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix Example 2 from task");
}

#[test]
fn test_example_3_multiple_instances() {
    // Example 3: Multiple instances with spaces both after opening and before closing
    let input = r#"- ** Query databases **: "Find emails of 10 random users..."
- ** Integrate designs **: "Update our standard email template..."
- ** Automate workflows **: "Create Gmail drafts inviting these users...""#;
    
    let expected = r#"- **Query databases**: "Find emails of 10 random users..."
- **Integrate designs**: "Update our standard email template..."
- **Automate workflows**: "Create Gmail drafts inviting these users...""#;
    
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix Example 3 from task");
}

#[test]
fn test_example_4_inline_bold() {
    // Additional example: Space after opening ** in the middle of a line
    // CopyAsk AI ** Understanding the "—" parameter:**
    let input = "CopyAsk AI ** Understanding the \"—\" parameter:**";
    let expected = "CopyAsk AI **Understanding the \"—\" parameter:**";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix Example 4 from task");
}

#[test]
fn test_multi_word_bold_with_spaces() {
    let input = "This is ** multiple word phrase ** in bold.";
    let expected = "This is **multiple word phrase** in bold.";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix multi-word bold with spaces");
}

#[test]
fn test_excessive_internal_spaces() {
    let input = "This is **  spaced  ** with multiple spaces.";
    let expected = "This is **spaced** with multiple spaces.";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix excessive internal spaces");
}

#[test]
fn test_bold_with_internal_punctuation_preserved() {
    // Internal punctuation should be preserved
    let input = "**text: more content** is fine.";
    let expected = "**text: more content** is fine.";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Internal punctuation should be preserved");
}

#[test]
fn test_bold_in_list_items() {
    let input = r#"- ** First item **
- ** Second item **
- ** Third item **"#;
    
    let expected = r#"- **First item**
- **Second item**
- **Third item**"#;
    
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix bold in list items");
}

#[test]
fn test_bold_at_line_boundaries() {
    let input = "** Bold at start**\nMiddle **bold** here\n**Bold at end **";
    let expected = "**Bold at start**\nMiddle **bold** here\n**Bold at end**";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix bold at line boundaries");
}

#[test]
fn test_multiple_bold_spans_same_line() {
    let input = "** First ** and ** second ** and ** third **";
    let expected = "**First** and **second** and **third**";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix multiple bold spans");
}

#[test]
fn test_bold_with_numbers() {
    let input = "** Version 1.2.3 ** released";
    let expected = "**Version 1.2.3** released";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix bold with numbers");
}

#[test]
fn test_all_punctuation_types() {
    let inputs_and_expected = vec![
        ("**text** :", "**text**:"),
        ("**text** ,", "**text**,"),
        ("**text** ;", "**text**;"),
        ("**text** .", "**text**."),
        ("**text** !", "**text**!"),
        ("**text** ?", "**text**?"),
    ];
    
    for (input, expected) in inputs_and_expected {
        let result = markdown_postprocessing::fix_bold_internal_spacing(input);
        assert_eq!(result, expected, "Failed to remove space before punctuation: {}", input);
    }
}

#[test]
fn test_combined_internal_and_punctuation_spacing() {
    // Both internal spaces AND space before punctuation
    let input = "** Works in your terminal ** :";
    let expected = "**Works in your terminal**:";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix combined spacing issues");
}

#[test]
fn test_no_change_for_correct_formatting() {
    // Already correctly formatted - should not change
    let input = "This is **correct** formatting.";
    let expected = "This is **correct** formatting.";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Should not change already correct formatting");
}

#[test]
fn test_empty_string() {
    let input = "";
    let expected = "";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Empty string should remain empty");
}

#[test]
fn test_no_bold_markers() {
    let input = "This text has no bold markers at all.";
    let expected = "This text has no bold markers at all.";
    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Text without bold markers should not change");
}

#[test]
fn test_real_world_mcp_example() {
    // Real example from code.claude.com/docs/en/mcp/index.md
    let input = r#"What can Claude do with MCP?

With MCP, Claude can:

- ** Implement features from issue trackers **: "Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub."
- ** Analyze monitoring data **: "Check Sentry and Statsig to check the usage of the feature described in ENG-4521."
- ** Query databases **: "Find emails of 10 random users who signed up in the last week."
- ** Integrate designs **: "Update our standard email template using the design from Figma."
- ** Automate workflows **: "Create Gmail drafts inviting these users to our upcoming webinar.""#;

    let expected = r#"What can Claude do with MCP?

With MCP, Claude can:

- **Implement features from issue trackers**: "Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub."
- **Analyze monitoring data**: "Check Sentry and Statsig to check the usage of the feature described in ENG-4521."
- **Query databases**: "Find emails of 10 random users who signed up in the last week."
- **Integrate designs**: "Update our standard email template using the design from Figma."
- **Automate workflows**: "Create Gmail drafts inviting these users to our upcoming webinar.""#;

    let result = markdown_postprocessing::fix_bold_internal_spacing(input);
    assert_eq!(result, expected, "Failed to fix real-world MCP example");
}
#[cfg(test)]
mod tests {
    use kodegen_tools_citescrape::content_saver::markdown_converter::markdown_postprocessing::fix_bold_internal_spacing;

    #[test]
    fn test_fix_bold_internal_spacing_actual_broken_output() {
        let input = "- ** Analyze monitoring data **: \"Check Sentry...\"";
        let result = fix_bold_internal_spacing(input);
        println!("Input:  {}", input);
        println!("Result: {}", result);
        assert_eq!(result, "- **Analyze monitoring data**: \"Check Sentry...\"");
    }

    #[test]
    fn test_fix_bold_internal_spacing_second_example() {
        let input = "** Understanding the parameter:**";
        let result = fix_bold_internal_spacing(input);
        println!("Input:  {}", input);
        println!("Result: {}", result);
        assert_eq!(result, "**Understanding the parameter:**");
    }

    #[test]
    fn test_fix_bold_internal_spacing_third_example() {
        let input = "** How plugin MCP servers work **:";
        let result = fix_bold_internal_spacing(input);
        println!("Input:  {}", input);
        println!("Result: {}", result);
        assert_eq!(result, "**How plugin MCP servers work**:");
    }
}
