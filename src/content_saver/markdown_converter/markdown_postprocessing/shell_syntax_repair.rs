//! Shell syntax repair for markdown code blocks.
//!
//! Fixes shell if statements that lose required spacing around brackets and operators
//! during HTML-to-Markdown conversion. This is a defensive fix for websites that serve
//! minified or malformed shell code.

use regex::Regex;
use std::sync::LazyLock;

// Pattern 1: Space after opening bracket with keywords (if[, while[, elif[, until[)
// Matches: if[, while[, elif[, until[
// Replaces with: if [, while [, elif [, until [
static SPACE_AFTER_BRACKET_KEYWORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(if|while|elif|until)\[")
        .expect("SPACE_AFTER_BRACKET_KEYWORD: hardcoded regex is valid")
});

// Pattern 2: Space before closing bracket
// Matches: "value"] or $VAR] or }]
// Replaces with: "value" ] or $VAR ] or } ]
static SPACE_BEFORE_CLOSING_BRACKET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(["\w\$\}])\]"#)
        .expect("SPACE_BEFORE_CLOSING_BRACKET: hardcoded regex is valid")
});

// Pattern 3: Space after opening bracket (general case)
// Matches: ["text or [$VAR or [word or [-flag
// Replaces with: [ "text or [ $VAR or [ word or [ -flag
static SPACE_AFTER_OPENING_BRACKET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\[([-"\$\w])"#)
        .expect("SPACE_AFTER_OPENING_BRACKET: hardcoded regex is valid")
});

// Pattern 4: Spaces around operators (=, !=)
// Matches: "value"="other" or $VAR!="value" or word=word
// Replaces with: "value" = "other" or $VAR != "value" or word = word
static SPACE_AROUND_OPERATORS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(["\w\}])(=|!=)(["\w\$])"#)
        .expect("SPACE_AROUND_OPERATORS: hardcoded regex is valid")
});

// Pattern 5: Space after bracket semicolon (];then, ];do, ];else)
// Matches: ];then or ];do or ];else
// Replaces with: ]; then or ]; do or ]; else
static SPACE_AFTER_BRACKET_SEMICOLON: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\];(then|do|else)")
        .expect("SPACE_AFTER_BRACKET_SEMICOLON: hardcoded regex is valid")
});

// Pattern 6: Spaces around numeric test operators
// Matches: $NUM-gt5 or $VAR-eq10 or $X-ne$Y
// Replaces with: $NUM -gt 5 or $VAR -eq 10 or $X -ne $Y
static SPACE_AROUND_TEST_OPERATORS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$(\w+)(-eq|-ne|-lt|-le|-gt|-ge)(\d+|\$\w+)")
        .expect("SPACE_AROUND_TEST_OPERATORS: hardcoded regex is valid")
});

// Pattern 6.5: Space after single-letter test flags before quotes/variables
// Matches: -f"file" or -d$VAR or -n"string"
// Replaces with: -f "file" or -d $VAR or -n "string"
static SPACE_AFTER_TEST_FLAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"-([a-z])(["\$])"#)
        .expect("SPACE_AFTER_TEST_FLAG: hardcoded regex is valid")
});

// Pattern 6.6: Space before quoted arguments after commands
// Matches: grep"pattern" or echo"hello" (letter immediately followed by opening quote)
// Replaces with: grep "pattern" or echo "hello"
// Note: Requires word char after quote to ensure it's an opening quote, not closing
static SPACE_BEFORE_QUOTED_ARG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([a-zA-Z])(["\'])(\w)"#)
        .expect("SPACE_BEFORE_QUOTED_ARG: hardcoded regex is valid")
});

// Pattern 7: Space around pipe operator (|)
// Matches: word|word or command|command
// Replaces with: word | word or command | command
static SPACE_AROUND_PIPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\S)\|(\S)")
        .expect("SPACE_AROUND_PIPE: hardcoded regex is valid")
});

// Pattern 8: Space around double pipe (||)
// Matches: word||word
// Replaces with: word || word
static SPACE_AROUND_DOUBLE_PIPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\S)\|\|(\S)")
        .expect("SPACE_AROUND_DOUBLE_PIPE: hardcoded regex is valid")
});

// Pattern 9: Space around double ampersand (&&)
// Matches: word&&word
// Replaces with: word && word
static SPACE_AROUND_DOUBLE_AMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\S)&&(\S)")
        .expect("SPACE_AROUND_DOUBLE_AMP: hardcoded regex is valid")
});

// Pattern 10: Space around redirect append (>>)
// Matches: word>>file
// Replaces with: word >> file
static SPACE_AROUND_REDIRECT_APPEND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\S)>>(\S)")
        .expect("SPACE_AROUND_REDIRECT_APPEND: hardcoded regex is valid")
});

// Pattern 11: Space around redirect out (>)
// Matches: word>file
// Replaces with: word > file
// Note: Applied AFTER >> to avoid matching the second > in >>
static SPACE_AROUND_REDIRECT_OUT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\S)>(\S)")
        .expect("SPACE_AROUND_REDIRECT_OUT: hardcoded regex is valid")
});

// Pattern 12: Space around redirect in (<)
// Matches: word<file
// Replaces with: word < file
static SPACE_AROUND_REDIRECT_IN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\S)<(\S)")
        .expect("SPACE_AROUND_REDIRECT_IN: hardcoded regex is valid")
});

// Pattern 13: Space around background operator (&)
// Matches: word&word
// Replaces with: word & word
// Note: Applied AFTER && to avoid matching the second & in &&
static SPACE_AROUND_BACKGROUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\S)&(\S)")
        .expect("SPACE_AROUND_BACKGROUND: hardcoded regex is valid")
});

/// Repair shell syntax by adding required spaces around brackets and operators
///
/// This is a defensive fix for websites that serve minified or malformed shell code.
/// Applies only to shell-family language code blocks (bash, sh, zsh, shell).
///
/// # Idempotency
///
/// This function is idempotent - running it multiple times on the same input
/// produces the same output. Already-correct spacing is preserved.
///
/// # Arguments
///
/// * `markdown` - Markdown content containing code blocks
///
/// # Returns
///
/// * Markdown with repaired shell syntax in bash/sh/zsh code blocks
pub fn repair_shell_syntax(markdown: &str) -> String {
    let mut result = String::with_capacity(markdown.len());
    let mut in_code_block = false;
    let mut current_language = String::new();
    let mut code_buffer = Vec::new();
    
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        
        // Detect code fence opening or closing
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if in_code_block {
                // Closing fence - process accumulated code if shell
                let repaired = if is_shell_language(&current_language) {
                    repair_shell_code_block(&code_buffer.join("\n"))
                } else {
                    code_buffer.join("\n")
                };
                
                // Write repaired code
                for repaired_line in repaired.lines() {
                    result.push_str(repaired_line);
                    result.push('\n');
                }
                
                // Write closing fence
                result.push_str(line);
                result.push('\n');
                
                // Reset state
                in_code_block = false;
                current_language.clear();
                code_buffer.clear();
            } else {
                // Opening fence - extract language
                current_language = extract_language(trimmed);
                in_code_block = true;
                
                // Write opening fence
                result.push_str(line);
                result.push('\n');
            }
        } else if in_code_block {
            // Accumulate code lines for processing
            code_buffer.push(line.to_string());
        } else {
            // Non-code line - pass through
            result.push_str(line);
            result.push('\n');
        }
    }
    
    // Handle unclosed code block - flush buffered content to preserve it
    // This allows the main processor's auto-close recovery to handle it
    if in_code_block && !code_buffer.is_empty() {
        for buffered_line in &code_buffer {
            result.push_str(buffered_line);
            result.push('\n');
        }
    }

    result
}

/// Check if language tag indicates shell code
fn is_shell_language(lang: &str) -> bool {
    matches!(lang.to_lowercase().as_str(), "bash" | "sh" | "zsh" | "shell")
}

/// Extract language identifier from code fence marker
fn extract_language(fence_line: &str) -> String {
    // Remove fence markers (``` or ~~~)
    let without_fence = fence_line.trim_start_matches(['`', '~'].as_ref());
    // Extract first word (language identifier)
    without_fence.split_whitespace().next().unwrap_or("").to_string()
}

/// Apply all shell syntax repair patterns to a code block
fn repair_shell_code_block(code: &str) -> String {
    let mut repaired = code.to_string();
    
    // Apply patterns in sequence (order matters for correctness)
    // Fix double operators BEFORE single operators to avoid incorrect matches
    
    // 1. Fix keyword+bracket spacing first (if[, while[)
    repaired = SPACE_AFTER_BRACKET_KEYWORD.replace_all(&repaired, "$1 [").to_string();
    
    // 2. Fix general opening bracket spacing ([-)
    repaired = SPACE_AFTER_OPENING_BRACKET.replace_all(&repaired, "[ $1").to_string();
    
    // 2.5 Fix test flag spacing (-f"file")
    repaired = SPACE_AFTER_TEST_FLAG.replace_all(&repaired, "-$1 $2").to_string();
    
    // 2.6 Fix command-quote spacing (grep"pattern")
    repaired = SPACE_BEFORE_QUOTED_ARG.replace_all(&repaired, "$1 $2$3").to_string();
    
    // 3. Fix closing bracket spacing ("])
    repaired = SPACE_BEFORE_CLOSING_BRACKET.replace_all(&repaired, "$1 ]").to_string();
    
    // 4. Fix operator spacing (=, !=)
    repaired = SPACE_AROUND_OPERATORS.replace_all(&repaired, "$1 $2 $3").to_string();
    
    // 5. Fix test operator spacing (-eq, -ne, etc.)
    repaired = SPACE_AROUND_TEST_OPERATORS.replace_all(&repaired, "$$1 $2 $3").to_string();
    
    // 6. Fix bracket semicolon spacing (];then)
    repaired = SPACE_AFTER_BRACKET_SEMICOLON.replace_all(&repaired, "]; $1").to_string();
    
    // 7. Fix double pipe spacing BEFORE single pipe (||)
    repaired = SPACE_AROUND_DOUBLE_PIPE.replace_all(&repaired, "$1 || $2").to_string();
    
    // 8. Fix double ampersand spacing BEFORE single ampersand (&&)
    repaired = SPACE_AROUND_DOUBLE_AMP.replace_all(&repaired, "$1 && $2").to_string();
    
    // 9. Fix redirect append spacing BEFORE single redirect (>>)
    repaired = SPACE_AROUND_REDIRECT_APPEND.replace_all(&repaired, "$1 >> $2").to_string();
    
    // 10. Fix single pipe spacing (|)
    repaired = SPACE_AROUND_PIPE.replace_all(&repaired, "$1 | $2").to_string();
    
    // 11. Fix redirect out spacing (>)
    repaired = SPACE_AROUND_REDIRECT_OUT.replace_all(&repaired, "$1 > $2").to_string();
    
    // 12. Fix redirect in spacing (<)
    repaired = SPACE_AROUND_REDIRECT_IN.replace_all(&repaired, "$1 < $2").to_string();
    
    // 13. Fix background operator spacing (&)
    repaired = SPACE_AROUND_BACKGROUND.replace_all(&repaired, "$1 & $2").to_string();
    
    repaired
}
