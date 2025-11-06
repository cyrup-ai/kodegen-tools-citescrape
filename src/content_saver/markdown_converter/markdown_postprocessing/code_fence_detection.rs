//! Code fence detection and validation utilities.

/// Code fence state to track fence type and character count
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeFence {
    pub char: char,         // '`' or '~'
    pub count: usize,       // Number of characters in the fence
    pub line_number: usize, // Line number where the fence opened
}

/// Detect code fence marker at the start of a line
/// Returns Some((char, count)) if the line starts with 3+ backticks or tildes
pub fn detect_code_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();

    // Check for backticks
    if trimmed.starts_with('`') {
        let count = trimmed.chars().take_while(|&c| c == '`').count();
        if count >= 3 {
            return Some(('`', count));
        }
    }

    // Check for tildes
    if trimmed.starts_with('~') {
        let count = trimmed.chars().take_while(|&c| c == '~').count();
        if count >= 3 {
            return Some(('~', count));
        }
    }

    None
}

/// Detect if a line looks like code based on common code patterns
/// Used for heuristic recovery of unclosed code fences
pub fn looks_like_code(line: &str) -> bool {
    let trimmed = line.trim();

    // Existing checks
    if trimmed.ends_with(';')
        || trimmed.ends_with('{')
        || trimmed.ends_with('}')
        || trimmed.contains("return ")
        || trimmed.contains("function ")
        || trimmed.contains("def ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("from ")
    {
        return true;
    }

    // NEW: Detect function/method calls with parentheses
    if trimmed.contains('(') && trimmed.contains(')') {
        return true;
    }

    // NEW: Detect variable assignments and operators
    if trimmed.contains(" = ")
        || trimmed.contains("const ")
        || trimmed.contains("let ")
        || trimmed.contains("var ")
    {
        return true;
    }

    // NEW: Detect array/object access
    if trimmed.contains('[') || trimmed.contains(']') {
        return true;
    }

    // Detect C-style comments only (avoid confusion with markdown headings)
    if trimmed.starts_with("//") {
        return true;
    }

    // NEW: Indented lines are likely code continuation
    if line.starts_with("    ") || line.starts_with('\t') {
        return true;
    }

    false
}
