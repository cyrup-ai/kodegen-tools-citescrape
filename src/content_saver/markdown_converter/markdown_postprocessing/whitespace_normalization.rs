//! Whitespace normalization for markdown content.
//!
//! Normalizes whitespace while preserving code block formatting and respecting
//! CommonMark structural semantics.

use super::code_fence_detection::{detect_code_fence, CodeFence};

/// Classification of markdown line types for spacing logic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineType {
    /// ATX heading (# Heading)
    Heading,
    /// Code fence marker (``` or ~~~)
    CodeFence,
    /// List item (*, -, +, or numbered)
    ListItem,
    /// Blockquote line (> text)
    Blockquote,
    /// Horizontal rule (---, ***, ___)
    HorizontalRule,
    /// Table row (| ... |)
    Table,
    /// Regular paragraph text
    Paragraph,
    /// Blank line (empty or whitespace-only)
    Blank,
}

/// Classify a markdown line by its structural type
fn classify_line(line: &str) -> LineType {
    let trimmed = line.trim_start();
    
    // Check for blank lines first
    if trimmed.is_empty() {
        return LineType::Blank;
    }

    // Check for shebang BEFORE heading detection
    // Shebangs start with #! (no space) and should never be treated as headings
    // This prevents corruption like "#!/bin/bash" → "# !/bin/bash"
    if trimmed.starts_with("#!") {
        // Even though shebangs only matter inside code blocks, we need to avoid
        // misclassifying them as headings during whitespace normalization
        return LineType::Paragraph; // Treat as regular paragraph to preserve exactly
    }
    
    // Code fence (``` or ~~~)
    if detect_code_fence(trimmed).is_some() {
        return LineType::CodeFence;
    }
    
    // ATX heading (# ...)
    if trimmed.starts_with('#') {
        // Verify it's a valid heading (has space after hashes or is hashes-only)
        let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hash_count) {
            let after_hashes = &trimmed[hash_count..];
            if after_hashes.is_empty() || after_hashes.starts_with(' ') {
                return LineType::Heading;
            }
        }
    }
    
    // List item (unordered: *, -, + or ordered: 1. 2. etc)
    if trimmed.starts_with("* ") || trimmed.starts_with("- ") || trimmed.starts_with("+ ") {
        return LineType::ListItem;
    }
    
    // Ordered list (number followed by . or ))
    if let Some(first_char) = trimmed.chars().next()
        && first_char.is_ascii_digit()
        && let Some(dot_pos) = trimmed.find('.')
        && dot_pos > 0 && dot_pos < 10 // Max 9 digits
    {
        let num_part = &trimmed[..dot_pos];
        if num_part.chars().all(|c| c.is_ascii_digit()) {
            let after_dot = &trimmed[dot_pos + 1..];
            if after_dot.is_empty() || after_dot.starts_with(' ') {
                return LineType::ListItem;
            }
        }
    }
    
    // Blockquote (> ...)
    if trimmed.starts_with('>') {
        return LineType::Blockquote;
    }
    
    // Horizontal rule (---, ***, ___)
    // Must be at least 3 characters of the same type
    let is_hr = {
        let chars: Vec<char> = trimmed.chars().collect();
        if chars.len() >= 3 {
            let first = chars[0];
            (first == '-' || first == '*' || first == '_') 
                && chars.iter().all(|&c| c == first || c == ' ')
                && chars.iter().filter(|&&c| c == first).count() >= 3
        } else {
            false
        }
    };
    if is_hr {
        return LineType::HorizontalRule;
    }
    
    // Table row (| ... |)
    if trimmed.starts_with('|') || trimmed.contains('|') {
        return LineType::Table;
    }
    
    // Default: paragraph text
    LineType::Paragraph
}

/// Determine if a blank line should be added before transitioning from prev to current line type
fn should_add_blank_before(prev: LineType, current: LineType) -> bool {
    use LineType::*;
    
    match (prev, current) {
        // Always add blank before heading (unless previous was also blank)
        (Paragraph | ListItem | Blockquote | Table, Heading) => true,
        
        // Add blank before code fence when starting
        (Paragraph | ListItem | Blockquote | Table | Heading, CodeFence) => true,
        
        // Add blank before horizontal rule
        (Paragraph | ListItem | Blockquote | Table | Heading, HorizontalRule) => true,
        
        // Add blank before blockquote (unless in blockquote sequence)
        (Paragraph | ListItem | Table | Heading, Blockquote) => true,
        
        // Add blank before list when transitioning from non-list
        (Paragraph | Blockquote | Table | Heading, ListItem) => true,
        
        // Add blank before table when transitioning from non-table
        (Paragraph | Blockquote | Heading | ListItem, Table) => true,
        
        // Don't add blanks in other cases (consecutive items of same type, etc)
        _ => false,
    }
}

/// Determine if a blank line should be added after a line type
fn should_add_blank_after(line_type: LineType) -> bool {
    use LineType::*;
    
    match line_type {
        // Add blank after heading
        Heading => true,
        
        // Add blank after horizontal rule
        HorizontalRule => true,
        
        // Don't automatically add blanks after other types
        // (they'll be handled by should_add_blank_before for the next line)
        _ => false,
    }
}

/// Normalize whitespace in markdown content
///
/// This function performs comprehensive whitespace normalization while preserving
/// code block formatting and respecting CommonMark structural semantics.
///
/// # Normalization Rules
///
/// 1. **Trailing Whitespace**: Removed from all lines except inside code blocks
/// 2. **Consecutive Blank Lines**: Collapsed to maximum of 1 blank line
/// 3. **Element Spacing**: Ensures proper blank lines around structural elements:
///    - One blank line before/after headings
///    - One blank line before code fences
///    - One blank line before horizontal rules
///    - One blank line when transitioning between different block types
/// 4. **Document Edges**: Removes leading/trailing blank lines from document
/// 5. **Code Blocks**: Preserves ALL whitespace (including trailing) inside fenced code blocks
///
/// # Arguments
///
/// * `markdown` - The markdown content to normalize
///
/// # Returns
///
/// Normalized markdown string with consistent whitespace
///
/// # Examples
///
/// ```rust
/// let input = "# Heading\n\n\n\nParagraph with trailing spaces   \n\n\nAnother paragraph";
/// let output = normalize_whitespace(input);
/// // Output: "# Heading\n\nParagraph with trailing spaces\n\nAnother paragraph"
/// ```
pub fn normalize_whitespace(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut result: Vec<String> = Vec::new();
    
    // State tracking
    let mut fence_stack: Option<CodeFence> = None;
    let mut consecutive_blanks: usize = 0;
    let mut prev_line_type: Option<LineType> = None;
    
    for (i, line) in lines.iter().enumerate() {
        // Check for code fence transitions
        let trimmed = line.trim_start();
        if let Some((fence_char, fence_count)) = detect_code_fence(trimmed) {
            if let Some(ref current_fence) = fence_stack {
                // We're inside a code block - check if this closes it
                if fence_char == current_fence.char && fence_count >= current_fence.count {
                    // This closes the current fence
                    fence_stack = None;
                    
                    // Emit the closing fence (no trailing whitespace removal)
                    result.push(line.to_string());
                    prev_line_type = Some(LineType::CodeFence);
                    consecutive_blanks = 0;
                    continue;
                }
            } else {
                // This opens a new code fence
                
                // Check if we need blank line before fence
                if let Some(prev) = prev_line_type
                    && should_add_blank_before(prev, LineType::CodeFence) && consecutive_blanks == 0
                {
                    result.push(String::new());
                }
                
                fence_stack = Some(CodeFence {
                    char: fence_char,
                    count: fence_count,
                    line_number: i,
                });
                
                // Emit the opening fence (no trailing whitespace removal)
                result.push(line.to_string());
                prev_line_type = Some(LineType::CodeFence);
                consecutive_blanks = 0;
                continue;
            }
        }
        
        // If we're inside a code fence, preserve the line exactly as-is
        if fence_stack.is_some() {
            result.push(line.to_string());
            consecutive_blanks = 0;
            continue;
        }
        
        // Outside code blocks: normalize whitespace
        let trimmed_end = line.trim_end();
        
        if trimmed_end.is_empty() {
            // This is a blank line
            consecutive_blanks += 1;
            
            // Only emit if it's the first blank in a sequence
            if consecutive_blanks == 1 {
                result.push(String::new());
                prev_line_type = Some(LineType::Blank);
            }
            // Skip additional consecutive blanks
        } else {
            // This is a content line
            let current_type = classify_line(trimmed_end);
            
            // Check if we need to add a blank line before this line
            if let Some(prev) = prev_line_type
                && should_add_blank_before(prev, current_type) && consecutive_blanks == 0
            {
                result.push(String::new());
            }
            
            // Emit the content line (with trailing whitespace removed)
            result.push(trimmed_end.to_string());
            
            // Check if we need to add a blank line after this line
            if should_add_blank_after(current_type) {
                result.push(String::new());
                prev_line_type = Some(LineType::Blank);
                consecutive_blanks = 1;
            } else {
                prev_line_type = Some(current_type);
                consecutive_blanks = 0;
            }
        }
    }
    
    // Handle unclosed code fence (best-effort recovery)
    if let Some(fence) = fence_stack {
        tracing::warn!(
            "Unclosed code fence starting at line {} (char: '{}', count: {})",
            fence.line_number,
            fence.char,
            fence.count
        );
        // The fence is left unclosed in output - markdown renderers handle this gracefully
    }
    
    // Trim leading/trailing blank lines from document
    let start = result.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let end = result.iter().rposition(|l| !l.is_empty()).unwrap_or(result.len());
    
    if start <= end && end < result.len() {
        result[start..=end].join("\n")
    } else {
        String::new()
    }
}


/// Normalize spacing around inline formatting markers
///
/// Ensures proper spacing around complete `**bold**` spans to prevent
/// merging with adjacent words, WITHOUT corrupting the bold text itself.
///
/// # Rules
///
/// 1. Add space before `**content**` if preceded by alphanumeric character
/// 2. Add space after `**content**` if followed by alphanumeric character
/// 3. NEVER modify the internal structure of bold spans
/// 4. Preserve existing spacing (don't double-space)
///
/// # Examples
///
/// - `word**bold**` -> `word **bold**`
/// - `**bold**word` -> `**bold** word`
/// - `**test**` -> `**test**` (unchanged - already valid)
/// - `word **spaced** text` -> `word **spaced** text` (unchanged)
pub fn normalize_inline_formatting_spacing(markdown: &str) -> String {
    use fancy_regex::Regex;
    use std::sync::LazyLock;

    // Pattern for complete bold span content: matches any char that's not *,
    // OR a single * not followed by another * (allows bold text with single asterisks)
    // Requires at least 1 character of content between ** markers
    //
    // Examples matched by (?:[^*]|\*(?!\*))+:
    //   - "test"           (simple text)
    //   - "a * b"          (text with single asterisk)
    //   - "code-block"     (text with punctuation)
    //
    // Examples NOT matched:
    //   - ""               (empty - needs 1+ chars)
    //   - "**"             (would be closing marker)

    // Match: word character immediately before a complete **content** span
    // This catches: word**bold** -> word **bold**
    // Does NOT match inside **test** because there's no word char before the opening **
    static WORD_BEFORE_BOLD_SPAN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(\w)(\*\*(?:[^*]|\*(?!\*))+\*\*)")
            .expect("WORD_BEFORE_BOLD_SPAN regex is valid")
    });

    // Match: complete **content** span immediately before a word character
    // This catches: **bold**word -> **bold** word
    // Does NOT match **test** followed by space because space is not \w
    static BOLD_SPAN_BEFORE_WORD: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(\*\*(?:[^*]|\*(?!\*))+\*\*)(\w)")
            .expect("BOLD_SPAN_BEFORE_WORD regex is valid")
    });

    let mut result = markdown.to_string();

    // Add space before bold span if preceded by word character (no space between)
    // $1 = the word char, $2 = the complete bold span
    result = WORD_BEFORE_BOLD_SPAN.replace_all(&result, "$1 $2").to_string();

    // Add space after bold span if followed by word character (no space between)
    // $1 = the complete bold span, $2 = the word char
    result = BOLD_SPAN_BEFORE_WORD.replace_all(&result, "$1 $2").to_string();

    result
}

/// Fix internal spacing in bold markers
///
/// Removes leading/trailing spaces INSIDE `** ... **` markers while preserving
/// the content. Also removes spaces before common punctuation marks.
///
/// # Rules
///
/// 1. `** text **` → `**text**` (strip internal leading/trailing spaces)
/// 2. `**text **` → `**text**` (strip internal trailing space)
/// 3. `** text**` → `**text**` (strip internal leading space)
/// 4. `**text** :` → `**text**:` (remove space before punctuation)
///
/// # Examples
///
/// - `** Works in your terminal ** :` → `**Works in your terminal**:`
/// - `** Build features **` → `**Build features**`
/// - `** Enterprise-ready**` → `**Enterprise-ready**`
///
/// # Implementation Notes
///
/// Uses two-pass regex approach:
/// 1. First pass: Strip internal spaces from bold markers
/// 2. Second pass: Remove spaces before punctuation
///
pub fn fix_bold_internal_spacing(markdown: &str) -> String {
    use fancy_regex::Regex;
    use std::sync::LazyLock;

    // Pattern: ** followed by optional whitespace, then content (non-greedy),
    // then optional whitespace, then **
    // 
    // Breakdown:
    // - `\*\*` - literal **
    // - `\s*` - zero or more whitespace chars (leading space inside markers)
    // - `(.+?)` - one or more chars (non-greedy) - captures the content
    // - `\s*` - zero or more whitespace chars (trailing space inside markers)
    // - `\*\*` - literal **
    //
    // Replacement: `**$1**`
    // - Wraps the content ($1) in ** without any internal spaces
    static BOLD_INTERNAL_SPACING: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\*\*\s*(.+?)\s*\*\*")
            .expect("BOLD_INTERNAL_SPACING regex is valid")
    });

    // Pattern: ** ... ** followed by space and punctuation
    // 
    // This catches: `**text** :` → `**text**:`
    // Common punctuation: : , . ! ? ;
    static SPACE_BEFORE_PUNCTUATION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(\*\*[^*]+\*\*)\s+([,:;.!?])")
            .expect("SPACE_BEFORE_PUNCTUATION regex is valid")
    });

    let mut result = markdown.to_string();

    // First pass: Remove internal spaces from bold markers
    result = BOLD_INTERNAL_SPACING.replace_all(&result, |caps: &fancy_regex::Captures| {
        let content = caps.get(1).unwrap().as_str().trim();
        format!("**{}**", content)
    }).to_string();

    // Second pass: Remove spaces before punctuation after bold text
    result = SPACE_BEFORE_PUNCTUATION.replace_all(&result, "$1$2").to_string();

    result
}
