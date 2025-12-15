//! Expressive-code HTML preprocessing
//!
//! Handles the "expressive-code" HTML pattern where code blocks use nested div elements:
//! ```html
//! <div class="expressive-code">
//!   <pre data-language="rust">
//!     <code>
//!       <div class="ec-line"><div class="code">line 1</div></div>
//!       <div class="ec-line"><div class="code">line 2</div></div>
//!     </code>
//!   </pre>
//! </div>
//! ```
//!
//! Since `<div>` inside `<code>` is invalid HTML, parsers like html5ever collapse the
//! content into a single text node, losing newlines between lines.
//!
//! This preprocessor converts expressive-code HTML to standard HTML BEFORE parsing:
//! ```html
//! <pre data-language="rust"><code>line 1
//! line 2</code></pre>
//! ```

use anyhow::Result;
use regex::Regex;
use std::sync::LazyLock;

/// Use <br> elements as line separators - they survive HTML parser whitespace collapsing
/// and will be converted to newlines by htmd in code blocks
const NEWLINE_PLACEHOLDER: &str = "<br>";

/// Preprocess expressive-code HTML patterns
///
/// Converts `<div class="ec-line">` patterns inside code blocks to plain newline-separated text.
/// This must happen BEFORE html5ever parsing since div-inside-code is invalid HTML.
pub fn preprocess_expressive_code(html: &str) -> Result<String> {
    // Fast path: skip if no expressive-code patterns detected
    if !html.contains("ec-line") && !html.contains("expressive-code") {
        return Ok(html.to_string());
    }
    
    // Pattern to match the entire expressive-code block
    // Captures: <pre...><code>...(ec-line divs)...</code></pre>
    static EC_CODE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?s)<pre([^>]*)><code([^>]*)>(.*?)</code></pre>"#
        ).expect("EC_CODE_BLOCK regex is valid")
    });
    
    // Pattern to extract content from ec-line divs
    // Matches: <div class="ec-line">...<div class="code">CONTENT</div>...</div>
    static EC_LINE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?s)<div[^>]*class="[^"]*ec-line[^"]*"[^>]*>.*?<div[^>]*class="[^"]*code[^"]*"[^>]*>(.*?)</div>.*?</div>"#
        ).expect("EC_LINE regex is valid")
    });
    
    let result = EC_CODE_BLOCK.replace_all(html, |caps: &regex::Captures| {
        let pre_attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let code_attrs = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let inner_html = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        
        // Check if this code block contains ec-line patterns
        if !inner_html.contains("ec-line") {
            // Not expressive-code, return unchanged
            return caps[0].to_string();
        }
        
        // Extract lines from ec-line divs
        // We decode entities, then re-encode EACH LINE, then join with <br>
        // This ensures <br> stays as actual HTML element, not &lt;br&gt;
        let lines: Vec<String> = EC_LINE
            .captures_iter(inner_html)
            .map(|cap| {
                let line_content = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                // Decode HTML entities in the content
                let decoded = html_escape::decode_html_entities(line_content).to_string();
                // Re-encode special characters for valid HTML (BEFORE joining with <br>)
                decoded
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            })
            .collect();
        
        if lines.is_empty() {
            // Fallback: no ec-line matches found, return original
            return caps[0].to_string();
        }
        
        // Join with <br> AFTER encoding individual lines
        // This keeps <br> as an actual HTML element that survives parsing
        let code_content = lines.join(NEWLINE_PLACEHOLDER);
        
        format!("<pre{}><code{}>{}</code></pre>", pre_attrs, code_attrs, code_content)
    });
    
    Ok(result.to_string())
}

/// Restore newlines from placeholders in HTML
///
/// Called after HTML cleaning to restore newlines that were preserved
/// using placeholder tokens in `preprocess_expressive_code`.
/// Convert `<br>` elements to actual newline characters inside `<pre>` blocks.
///
/// This is needed because htmd doesn't convert `<br>` to newlines inside code elements.
/// The expressive-code preprocessor uses `<br>` to preserve line breaks, so we need
/// to convert them to actual `\n` characters before htmd processing.
///
/// # Arguments
/// * `html` - HTML string with `<br>` elements inside code blocks
///
/// # Returns
/// * HTML with `<br>` converted to `\n` inside `<pre>` blocks
pub fn convert_br_to_newlines_in_code(html: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    
    // Match <pre> blocks and capture their contents
    static PRE_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?si)(<pre[^>]*>)(.*?)(</pre>)")
            .expect("PRE_BLOCK_RE regex is valid")
    });
    
    // Match <br> elements (with optional closing slash and attributes)
    static BR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"<br\s*/?>")
            .expect("BR_RE regex is valid")
    });
    
    PRE_BLOCK_RE.replace_all(html, |caps: &regex::Captures| {
        let pre_open = &caps[1];
        let content = &caps[2];
        let pre_close = &caps[3];
        
        // Replace <br> with newline inside this pre block
        let content_with_newlines = BR_RE.replace_all(content, "\n");
        
        format!("{}{}{}", pre_open, content_with_newlines, pre_close)
    }).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_preprocess_expressive_code_basic() {
        let html = r#"<div class="expressive-code"><pre data-language="rust"><code><div class="ec-line"><div class="code">fn main() {</div></div>
<div class="ec-line"><div class="code">    println!("Hello");</div></div>
<div class="ec-line"><div class="code">}</div></div></code></pre></div>"#;
        
        let result = preprocess_expressive_code(html).unwrap();
        
        // Should have <br> between lines (which convert_br_to_newlines_in_code will later convert to \n)
        assert!(result.contains("fn main() {<br>"), "Should have <br> after opening brace");
        assert!(result.contains("println!"), "Should contain println");
        // Should NOT have ec-line divs anymore in the code block
        assert!(!result.contains("<div class=\"ec-line\">"), "Should not have ec-line divs in output");
    }
    
    #[test]
    fn test_preprocess_expressive_code_with_entities() {
        let html = r#"<pre><code><div class="ec-line"><div class="code">fn foo() -&gt; Result&lt;()&gt; {</div></div></code></pre>"#;
        
        let result = preprocess_expressive_code(html).unwrap();
        
        // Should decode then re-encode entities
        assert!(result.contains("&gt;"), "Should have encoded >");
        assert!(result.contains("&lt;"), "Should have encoded <");
    }
    
    #[test]
    fn test_preprocess_non_expressive_code_unchanged() {
        let html = r#"<pre><code>fn main() {
    println!("Hello");
}</code></pre>"#;
        
        let result = preprocess_expressive_code(html).unwrap();
        
        // Should be unchanged
        assert_eq!(result, html);
    }
    
    #[test]
    fn test_preprocess_fast_path() {
        let html = "<p>No code blocks here</p>";
        
        let result = preprocess_expressive_code(html).unwrap();
        
        // Should return unchanged (fast path)
        assert_eq!(result, html);
    }
}
