//! Whitespace normalization for markdown content.
//!
//! Normalizes whitespace while preserving code block formatting and respecting
//! CommonMark structural semantics.

use super::code_fence_detection::{detect_code_fence, CodeFence};
use std::sync::LazyLock;

// Fix angle bracket spacing from htmd's unknown element handler
// Matches: < word with internal spaces >
// Examples: < nam e > → <name>, < ur l > → <url>, < comman d > → <command>
// Does NOT match shell heredoc markers: << EOF >
static ANGLE_BRACKET_SPACING: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    // Pattern explanation:
    // (?<!<)          - Negative lookbehind: NOT preceded by < (avoids heredoc: << EOF >)
    // < \s+           - Opening bracket followed by one or more whitespace
    // ([\w\s-]+?)     - Capture group: word chars, spaces, hyphens (non-greedy)
    // \s+ >           - One or more whitespace followed by closing bracket
    // Uses fancy_regex for negative lookbehind support
    fancy_regex::Regex::new(r"(?<!<)<\s+([\w\s-]+?)\s+>")
        .expect("ANGLE_BRACKET_SPACING: hardcoded regex is valid")
});

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
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::markdown_postprocessing::whitespace_normalization::normalize_whitespace;
/// let input = "# Title\n\n\n\nParagraph   \n\n";
/// let result = normalize_whitespace(input);
/// // Normalizes excessive blank lines and trailing spaces
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
        tracing::debug!(
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

/// Fix angle bracket spacing caused by htmd's unknown element handler
///
/// When HTML contains literal angle brackets like `<name>` or `<url>` for
/// placeholders, the HTML parser treats them as unknown HTML elements.
/// htmd's default element handler then inserts spaces, producing `< nam e >`.
///
/// This function detects and fixes the pattern:
/// - `< nam e >` → `<name>`
/// - `< ur l >` → `<url>`  
/// - `< comman d >` → `<command>`
///
/// # Arguments
///
/// * `markdown` - The markdown content to fix
///
/// # Returns
///
/// Markdown with corrected angle bracket spacing
///
/// # Examples
///
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::markdown_postprocessing::fix_angle_bracket_spacing;
/// let broken = "Use < nam e > and < ur l > as placeholders";
/// let fixed = fix_angle_bracket_spacing(broken);
/// assert_eq!(fixed, "Use <name> and <url> as placeholders");
/// ```
pub fn fix_angle_bracket_spacing(markdown: &str) -> String {
    ANGLE_BRACKET_SPACING.replace_all(markdown, |caps: &fancy_regex::Captures| {
        // Extract the captured content (everything between < and >)
        let content = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        
        // Remove all internal whitespace
        let cleaned = content.split_whitespace().collect::<String>();
        
        // Return with properly formatted angle brackets (no spaces)
        format!("<{}>", cleaned)
    }).to_string()
}


/// Fix spacing in HTML tag syntax from htmd's unknown element handler
///
/// When htmd encounters unrecognized HTML elements (like `<span style="...">` in code blocks),
/// its default element handler treats them as text and adds spaces around punctuation characters.
/// This creates malformed HTML syntax like:
/// - `< /span >` instead of `</span>`
/// - `style = "..."` instead of `style="..."`
/// - `style="..." >` instead of `style="...">`
///
/// This function detects and fixes these patterns using regex replacements. It is careful to
/// only match HTML attribute contexts (e.g., `attr="value" >`) and not shell redirect operators
/// (e.g., `echo 'text' >`).
///
/// # Arguments
///
/// * `markdown` - The markdown content to fix
///
/// # Returns
///
/// Markdown with corrected HTML tag spacing
///
/// # Examples
///
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::markdown_postprocessing::fix_html_tag_spacing;
/// let broken = r#"<span style = "color:red" > text < /span >"#;
/// let fixed = fix_html_tag_spacing(broken);
/// assert_eq!(fixed, r#"<span style="color:red"> text </span>"#);
/// ```
pub fn fix_html_tag_spacing(markdown: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    
    // Pattern 1: Fix spaces around = in attributes
    // Matches: attribute = "value" or attribute ="value" or attribute= "value"
    // Replaces with: attribute="value"
    static ATTR_EQUALS_SPACING: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(\w+)\s*=\s*""#)
            .expect("ATTR_EQUALS_SPACING: hardcoded regex is valid")
    });
    
    // Pattern 2: Fix spaces before closing > in HTML tags only
    // Matches: attribute="value" > or attribute='value' >
    // Does NOT match shell redirects like: echo 'text' >
    // Uses negative lookbehind to ensure quote is preceded by = (HTML attribute context)
    static CLOSE_BRACKET_SPACING: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(=["'][^"']*["'])\s+>"#)
            .expect("CLOSE_BRACKET_SPACING: hardcoded regex is valid")
    });
    
    // Pattern 5: Fix spaces after closing > in opening HTML tags
    // Matches: "> text" when the > follows a quoted attribute value
    // Replaces with: ">text" (no space)
    // Does NOT match shell redirects because quote must immediately precede >
    static OPEN_TAG_AFTER_GT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(["'])>\s+"#)
            .expect("OPEN_TAG_AFTER_GT: hardcoded regex is valid")
    });
    
    // Pattern 6: Fix spaces before opening < in closing tags
    // Matches: content followed by space(s) followed by </
    // Replaces with: content immediately followed by </
    // This handles cases like "git </span>" -> "git</span>"
    static SPACE_BEFORE_CLOSE_TAG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\s+</")
            .expect("SPACE_BEFORE_CLOSE_TAG: hardcoded regex is valid")
    });
    
    // Pattern 3: Fix spaces in closing tags after <
    // Matches: < /tag
    // Replaces with: </tag
    static CLOSE_TAG_AFTER_LT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"<\s+/")
            .expect("CLOSE_TAG_AFTER_LT: hardcoded regex is valid")
    });
    
    // Pattern 4: Fix spaces in closing tags before >
    // Matches: </tag >
    // Replaces with: </tag>
    static CLOSE_TAG_BEFORE_GT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(/\w+)\s+>")
            .expect("CLOSE_TAG_BEFORE_GT: hardcoded regex is valid")
    });
    
    let mut result = markdown.to_string();
    
    // Apply fixes in sequence
    // Order matters: fix attribute equals first, then closing bracket, then after bracket,
    // then fix < / to </, THEN remove spaces before </, then fix /tag >
    result = ATTR_EQUALS_SPACING.replace_all(&result, "$1=\"").to_string();
    result = CLOSE_BRACKET_SPACING.replace_all(&result, "$1>").to_string();
    result = OPEN_TAG_AFTER_GT.replace_all(&result, "$1>").to_string();  // Pattern 5
    result = CLOSE_TAG_AFTER_LT.replace_all(&result, "</").to_string();  // Pattern 3: < / -> </
    result = SPACE_BEFORE_CLOSE_TAG.replace_all(&result, "</").to_string();  // Pattern 6: " </" -> "</"
    result = CLOSE_TAG_BEFORE_GT.replace_all(&result, "$1>").to_string();
    
    result
}

/// Simplify redundant URL-as-link-text patterns
///
/// Converts markdown links where the link text IS the URL itself into plain URLs.
/// This handles the common case where HTML like `<a href="url">url</a>` produces
/// redundant markdown `[url](url)`.
///
/// This function is part of the markdown post-processing pipeline (Stage 3.5.5)
/// and runs AFTER relative URLs have been converted to absolute URLs in Stage 3.5.
///
/// # Rules
///
/// 1. `[https://example.com](https://example.com)` → `https://example.com`
/// 2. `[Https://example.com](https://example.com)` → `https://example.com` (case-insensitive match)
/// 3. `[http://example.com](http://example.com)` → `http://example.com`
/// 4. Preserves links where text differs from URL: `[Click here](https://example.com)` unchanged
///
/// # Performance
///
/// - Fast path: Returns immediately if markdown contains no links (no `](` pattern)
/// - Bounded quantifiers prevent catastrophic backtracking (matches LINK_RE pattern)
/// - Uses standard `regex::Regex` (no lookbehind needed, ~10x faster than fancy_regex)
///
/// # Arguments
///
/// * `markdown` - The markdown content to process
///
/// # Returns
///
/// Markdown with redundant URL links simplified to plain URLs
///
/// # Examples
///
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::markdown_postprocessing::simplify_url_as_link_text;
/// let input = "[Https://example.com/path](https://example.com/path)";
/// let result = simplify_url_as_link_text(input);
/// assert_eq!(result, "https://example.com/path");
/// ```
pub fn simplify_url_as_link_text(markdown: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    // Fast path: no markdown links present
    if !markdown.contains("](") {
        return markdown.to_string();
    }

    // Pattern: Match markdown links [text](url)
    // Group 1: link text (up to 2000 chars to match LINK_RE bounds)
    // Group 2: URL (up to 2000 chars)
    // Bounded quantifiers prevent catastrophic backtracking
    static URL_LINK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\[([^\]]{1,2000})\]\(([^)]{1,2000})\)")
            .expect("URL_LINK_PATTERN: hardcoded regex is valid")
    });

    URL_LINK_PATTERN.replace_all(markdown, |caps: &regex::Captures| {
        let link_text = &caps[1];
        let url = &caps[2];

        // Check if link text is a URL that matches the href
        // Use case-insensitive comparison to handle "Https://..." vs "https://..."
        if is_url_matching_link_text(link_text, url) {
            // Return just the URL (use the href version which has correct casing)
            url.to_string()
        } else {
            // Keep original link format - text differs from URL
            format!("[{}]({})", link_text, url)
        }
    }).to_string()
}

/// Check if link text is a URL that matches the href URL
///
/// Performs case-insensitive comparison to handle protocol case variations
/// like "Https://example.com" vs "https://example.com".
///
/// # Arguments
///
/// * `link_text` - The visible link text
/// * `url` - The href URL
///
/// # Returns
///
/// `true` if link_text is the same URL as href (allowing protocol/domain case differences)
///
/// # Implementation Notes
///
/// URLs are case-insensitive for protocol and domain, but case-sensitive for path/query.
/// However, if someone writes the same URL with different casing in the link text,
/// it's still redundant and should be simplified. This function intentionally uses
/// full lowercase comparison for simplicity and correctness.
fn is_url_matching_link_text(link_text: &str, url: &str) -> bool {
    let text_trimmed = link_text.trim();
    let url_trimmed = url.trim();

    // Must start with http:// or https:// (case-insensitive check)
    let text_lower = text_trimmed.to_lowercase();
    if !text_lower.starts_with("http://") && !text_lower.starts_with("https://") {
        return false;
    }

    // Compare URLs case-insensitively
    // This handles: [Https://Example.com](https://example.com) → matches
    // Note: This is intentionally lenient - if someone wrote the same URL
    // with different casing in text vs href, it's still redundant
    text_lower == url_trimmed.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_html_tag_spacing_complete() {
        let input = r#"<span style = "--0:#82AAFF;--1:#3B61B0" > git < /span >"#;
        let expected = r#"<span style="--0:#82AAFF;--1:#3B61B0">git</span>"#;
        let result = fix_html_tag_spacing(input);
        assert_eq!(result, expected, "All HTML tag spacing issues should be fixed");
    }

    #[test]
    fn test_fix_html_tag_spacing_space_after_gt() {
        let input = r#"<span style="color:red"> text</span>"#;
        let expected = r#"<span style="color:red">text</span>"#;
        let result = fix_html_tag_spacing(input);
        assert_eq!(result, expected, "Space after > should be removed");
    }

    #[test]
    fn test_fix_html_tag_spacing_preserves_shell_redirects() {
        let input = r#"echo "hello" > file.txt"#;
        let result = fix_html_tag_spacing(input);
        // Should NOT change shell redirects
        assert_eq!(result, input, "Shell redirects should be preserved");
    }
}
