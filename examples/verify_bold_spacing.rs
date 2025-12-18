/// Standalone verification program for Task 014: Bold Marker Spacing Fix
///
/// This executable independently verifies that the regex patterns used in
/// `fix_bold_internal_spacing()` work correctly on all test cases.
///
/// Run with: cargo run --example verify_bold_spacing
use fancy_regex::Regex;

fn fix_bold_internal_spacing(markdown: &str) -> String {
    // Pattern 1: Strip internal spaces from bold markers
    let bold_internal_spacing = Regex::new(r"\*\*\s*(.+?)\s*\*\*")
        .expect("BOLD_INTERNAL_SPACING regex is valid");

    // Pattern 2: Remove space before punctuation after bold text
    let space_before_punctuation = Regex::new(r"(\*\*[^*]+\*\*)\s+([,:;.!?])")
        .expect("SPACE_BEFORE_PUNCTUATION regex is valid");

    let mut result = markdown.to_string();

    // First pass: Remove internal spaces from bold markers
    result = bold_internal_spacing
        .replace_all(&result, |caps: &fancy_regex::Captures| {
            let content = caps.get(1).unwrap().as_str().trim();
            format!("**{}**", content)
        })
        .to_string();

    // Second pass: Remove spaces before punctuation after bold text
    result = space_before_punctuation
        .replace_all(&result, "$1$2")
        .to_string();

    result
}

struct TestCase {
    input: &'static str,
    expected: &'static str,
    description: &'static str,
}

fn main() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║  Task 014: Bold Marker Spacing Fix - Verification Report      ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let test_cases = vec![
        TestCase {
            input: "- **Implement features from issue trackers **: \"Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub.\"",
            expected: "- **Implement features from issue trackers**: \"Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub.\"",
            description: "Example 1: Space before closing marker",
        },
        TestCase {
            input: "- ** Analyze monitoring data **: \"Check Sentry and Statsig to check the usage of the feature described in ENG-4521.\"",
            expected: "- **Analyze monitoring data**: \"Check Sentry and Statsig to check the usage of the feature described in ENG-4521.\"",
            description: "Example 2: Space after opening marker",
        },
        TestCase {
            input: "- ** Query databases **: \"Find emails of 10 random users...\"",
            expected: "- **Query databases**: \"Find emails of 10 random users...\"",
            description: "Example 3a: Query databases",
        },
        TestCase {
            input: "- ** Integrate designs **: \"Update our standard email template...\"",
            expected: "- **Integrate designs**: \"Update our standard email template...\"",
            description: "Example 3b: Integrate designs",
        },
        TestCase {
            input: "- ** Automate workflows **: \"Create Gmail drafts inviting these users...\"",
            expected: "- **Automate workflows**: \"Create Gmail drafts inviting these users...\"",
            description: "Example 3c: Automate workflows",
        },
        TestCase {
            input: "CopyAsk AI ** Understanding the \"—\" parameter:**",
            expected: "CopyAsk AI **Understanding the \"—\" parameter:**",
            description: "Example 4: Inline bold",
        },
        TestCase {
            input: "** text **",
            expected: "**text**",
            description: "Pattern: Spaces on both sides",
        },
        TestCase {
            input: "**text **",
            expected: "**text**",
            description: "Pattern: Space before closing",
        },
        TestCase {
            input: "** text**",
            expected: "**text**",
            description: "Pattern: Space after opening",
        },
        TestCase {
            input: "**text** :",
            expected: "**text**:",
            description: "Pattern: Space before colon",
        },
        TestCase {
            input: "**text** ,",
            expected: "**text**,",
            description: "Pattern: Space before comma",
        },
        TestCase {
            input: "**text** .",
            expected: "**text**.",
            description: "Pattern: Space before period",
        },
        TestCase {
            input: "**text** !",
            expected: "**text**!",
            description: "Pattern: Space before exclamation",
        },
        TestCase {
            input: "**text** ?",
            expected: "**text**?",
            description: "Pattern: Space before question mark",
        },
        TestCase {
            input: "**text** ;",
            expected: "**text**;",
            description: "Pattern: Space before semicolon",
        },
        TestCase {
            input: "** multiple word phrase **",
            expected: "**multiple word phrase**",
            description: "Edge case: Multi-word bold",
        },
        TestCase {
            input: "**  spaced  **",
            expected: "**spaced**",
            description: "Edge case: Excessive internal spaces",
        },
        TestCase {
            input: "** Works in your terminal ** :",
            expected: "**Works in your terminal**:",
            description: "Edge case: Combined internal + punctuation spacing",
        },
        TestCase {
            input: "**correct** formatting",
            expected: "**correct** formatting",
            description: "Edge case: Already correct (no change)",
        },
        TestCase {
            input: "**text: more**",
            expected: "**text: more**",
            description: "Edge case: Internal punctuation preserved",
        },
        TestCase {
            input: "** First ** and ** Second ** and ** Third **",
            expected: "**First** and **Second** and **Third**",
            description: "Edge case: Multiple bold spans",
        },
        TestCase {
            input: "** Version 1.2.3 **",
            expected: "**Version 1.2.3**",
            description: "Edge case: Bold with numbers",
        },
    ];

    let mut all_passed = true;
    let mut pass_count = 0;
    let mut fail_count = 0;

    println!("Running {} test cases...\n", test_cases.len());

    for test_case in &test_cases {
        let result = fix_bold_internal_spacing(test_case.input);
        let passed = result == test_case.expected;

        if passed {
            println!("  ✅ {}", test_case.description);
            pass_count += 1;
        } else {
            println!("  ❌ {}", test_case.description);
            println!("     Input:    {:?}", test_case.input);
            println!("     Expected: {:?}", test_case.expected);
            println!("     Got:      {:?}", result);
            println!();
            all_passed = false;
            fail_count += 1;
        }
    }

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║  Test Results Summary                                          ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\n  Total tests: {}", pass_count + fail_count);
    println!("  Passed:      {} ✅", pass_count);
    println!("  Failed:      {} ❌", fail_count);

    if all_passed {
        println!("\n  🎉 SUCCESS: All tests passed!");
        println!("  The bold marker spacing fix is working correctly.\n");
        std::process::exit(0);
    } else {
        println!("\n  ⚠️  FAILURE: Some tests failed. See details above.\n");
        std::process::exit(1);
    }
}
