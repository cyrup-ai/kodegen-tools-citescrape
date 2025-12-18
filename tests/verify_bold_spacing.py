#!/usr/bin/env python3
"""
Standalone verification script for Task 014: Bold Marker Spacing Fix

This Python script verifies that the regex patterns used in the Rust
implementation work correctly on all test cases from the task document.

The regex patterns are identical to those in:
  src/content_saver/markdown_converter/markdown_postprocessing/whitespace_normalization.rs

Run with: python3 tests/verify_bold_spacing.py
"""

import re
import sys


def fix_bold_internal_spacing(markdown: str) -> str:
    """
    Fix bold marker spacing using the same regex patterns as the Rust implementation.
    
    Two-pass approach:
    1. Strip internal spaces from bold markers: `** text **` → `**text**`
    2. Remove spaces before punctuation: `**text** :` → `**text**:`
    """
    # Pattern 1: Strip internal spaces from bold markers
    # Rust pattern: r"\*\*\s*(.+?)\s*\*\*"
    # Python equivalent (non-greedy):
    bold_internal_spacing = re.compile(r'\*\*\s*(.+?)\s*\*\*')
    
    # Pattern 2: Remove space before punctuation after bold text
    # Rust pattern: r"(\*\*[^*]+\*\*)\s+([,:;.!?])"
    space_before_punctuation = re.compile(r'(\*\*[^*]+\*\*)\s+([,:;.!?])')
    
    # First pass: Remove internal spaces from bold markers
    result = bold_internal_spacing.sub(lambda m: f"**{m.group(1).strip()}**", markdown)
    
    # Second pass: Remove spaces before punctuation after bold text
    result = space_before_punctuation.sub(r'\1\2', result)
    
    return result


def run_tests():
    """Run all test cases and report results."""
    
    test_cases = [
        # Examples from task document
        {
            "input": '- **Implement features from issue trackers **: "Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub."',
            "expected": '- **Implement features from issue trackers**: "Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub."',
            "description": "Example 1: Space before closing marker"
        },
        {
            "input": '- ** Analyze monitoring data **: "Check Sentry and Statsig to check the usage of the feature described in ENG-4521."',
            "expected": '- **Analyze monitoring data**: "Check Sentry and Statsig to check the usage of the feature described in ENG-4521."',
            "description": "Example 2: Space after opening marker"
        },
        {
            "input": '- ** Query databases **: "Find emails of 10 random users..."',
            "expected": '- **Query databases**: "Find emails of 10 random users..."',
            "description": "Example 3a: Query databases"
        },
        {
            "input": '- ** Integrate designs **: "Update our standard email template..."',
            "expected": '- **Integrate designs**: "Update our standard email template..."',
            "description": "Example 3b: Integrate designs"
        },
        {
            "input": '- ** Automate workflows **: "Create Gmail drafts inviting these users..."',
            "expected": '- **Automate workflows**: "Create Gmail drafts inviting these users..."',
            "description": "Example 3c: Automate workflows"
        },
        {
            "input": 'CopyAsk AI ** Understanding the "—" parameter:**',
            "expected": 'CopyAsk AI **Understanding the "—" parameter:**',
            "description": "Example 4: Inline bold"
        },
        # Pattern variations
        {
            "input": "** text **",
            "expected": "**text**",
            "description": "Pattern: Spaces on both sides"
        },
        {
            "input": "**text **",
            "expected": "**text**",
            "description": "Pattern: Space before closing"
        },
        {
            "input": "** text**",
            "expected": "**text**",
            "description": "Pattern: Space after opening"
        },
        {
            "input": "**text** :",
            "expected": "**text**:",
            "description": "Pattern: Space before colon"
        },
        {
            "input": "**text** ,",
            "expected": "**text**,",
            "description": "Pattern: Space before comma"
        },
        {
            "input": "**text** .",
            "expected": "**text**.",
            "description": "Pattern: Space before period"
        },
        {
            "input": "**text** !",
            "expected": "**text**!",
            "description": "Pattern: Space before exclamation"
        },
        {
            "input": "**text** ?",
            "expected": "**text**?",
            "description": "Pattern: Space before question mark"
        },
        {
            "input": "**text** ;",
            "expected": "**text**;",
            "description": "Pattern: Space before semicolon"
        },
        # Edge cases
        {
            "input": "** multiple word phrase **",
            "expected": "**multiple word phrase**",
            "description": "Edge case: Multi-word bold"
        },
        {
            "input": "**  spaced  **",
            "expected": "**spaced**",
            "description": "Edge case: Excessive internal spaces"
        },
        {
            "input": "** Works in your terminal ** :",
            "expected": "**Works in your terminal**:",
            "description": "Edge case: Combined internal + punctuation spacing"
        },
        {
            "input": "**correct** formatting",
            "expected": "**correct** formatting",
            "description": "Edge case: Already correct (no change)"
        },
        {
            "input": "**text: more**",
            "expected": "**text: more**",
            "description": "Edge case: Internal punctuation preserved"
        },
        {
            "input": "** First ** and ** Second ** and ** Third **",
            "expected": "**First** and **Second** and **Third**",
            "description": "Edge case: Multiple bold spans"
        },
        {
            "input": "** Version 1.2.3 **",
            "expected": "**Version 1.2.3**",
            "description": "Edge case: Bold with numbers"
        },
    ]
    
    print("\n" + "=" * 70)
    print("  Task 014: Bold Marker Spacing Fix - Verification Report")
    print("=" * 70 + "\n")
    print(f"Running {len(test_cases)} test cases...\n")
    
    all_passed = True
    pass_count = 0
    fail_count = 0
    
    for test_case in test_cases:
        result = fix_bold_internal_spacing(test_case["input"])
        passed = result == test_case["expected"]
        
        if passed:
            print(f"  ✅ {test_case['description']}")
            pass_count += 1
        else:
            print(f"  ❌ {test_case['description']}")
            print(f"     Input:    {repr(test_case['input'])}")
            print(f"     Expected: {repr(test_case['expected'])}")
            print(f"     Got:      {repr(result)}")
            print()
            all_passed = False
            fail_count += 1
    
    print("\n" + "=" * 70)
    print("  Test Results Summary")
    print("=" * 70)
    print(f"\n  Total tests: {pass_count + fail_count}")
    print(f"  Passed:      {pass_count} ✅")
    print(f"  Failed:      {fail_count} ❌")
    
    if all_passed:
        print("\n  🎉 SUCCESS: All tests passed!")
        print("  The bold marker spacing fix is working correctly.\n")
        return 0
    else:
        print("\n  ⚠️  FAILURE: Some tests failed. See details above.\n")
        return 1


if __name__ == "__main__":
    sys.exit(run_tests())
