//! Code block protection during HTML cleaning
//!
//! Protects `<pre>` blocks from HTML cleaning by extracting raw HTML text
//! and converting directly to markdown, bypassing htmd to preserve ALL whitespace.
//!
//! This solves the critical bug where html5ever's HTML parser collapses whitespace
//! in text nodes, causing shell commands to lose spaces (e.g., "export VAR" → "exportVAR").

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use base64::{Engine as _, engine::general_purpose};

/// Regex to match <pre> blocks (including nested content)
static PRE_CODE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<pre[^>]*>.*?</pre>")
        .expect("PRE_CODE_BLOCK regex is valid")
});

/// Protected code blocks storage
pub struct CodeBlockProtector {
    /// Maps placeholder to original HTML
    blocks: HashMap<String, String>,
    /// Counter for generating unique placeholders
    counter: usize,
}

impl CodeBlockProtector {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            counter: 0,
        }
    }
    
    /// Protect all <pre> blocks by replacing with base64-encoded HTML comment placeholders
    ///
    /// This ensures code blocks bypass ALL DOM parsing operations, preserving whitespace.
    ///
    /// Strategy:
    /// 1. Find all <pre>...</pre> blocks (including nested <code>)
    /// 2. Base64-encode the entire block (including tags)
    /// 3. Replace with HTML comment: <!--CITESCRAPE-PRE-{id}:{base64}-->
    /// 4. HTML comments are guaranteed to pass through DOM parsers unchanged
    ///
    /// Example transformation:
    /// ```
    /// <pre><code>npm install -g @anthropic-ai/claude-code</code></pre>
    /// ↓
    /// <!--CITESCRAPE-PRE-0:PHByZT48Y29kZT5ucG0gaW5zdGFsbCAtZyBAYW50aHJvcGljLWFpL2NsYXVkZS1jb2RlPC9jb2RlPjwvcHJlPg==-->
    /// ```
    ///
    /// Returns: HTML with all <pre> blocks replaced by comment placeholders
    pub fn protect(&mut self, html: &str) -> String {
        PRE_CODE_BLOCK.replace_all(html, |caps: &regex::Captures| {
            let full_match = &caps[0];
            
            // Base64-encode the ENTIRE <pre> block (including tags)
            // This preserves all whitespace, attributes, and nested structure
            let encoded = general_purpose::STANDARD.encode(full_match.as_bytes());
            
            // Generate unique ID for this block
            let id = self.counter;
            self.counter += 1;
            
            // Store original HTML in HashMap for potential fallback
            let placeholder_key = format!("PRE_BLOCK_{}", id);
            self.blocks.insert(placeholder_key, full_match.to_string());
            
            // Return HTML comment placeholder
            // Format: <!--CITESCRAPE-PRE-{id}:{base64}-->
            format!("<!--CITESCRAPE-PRE-{}:{}-->", id, encoded)
        }).to_string()
    }
    
    /// Restore original code blocks from HTML comment placeholders
    ///
    /// This runs AFTER all DOM parsing is complete.
    ///
    /// Strategy:
    /// 1. Find all <!--CITESCRAPE-PRE-{id}:{base64}--> placeholders
    /// 2. Base64-decode to get original HTML
    /// 3. Replace placeholder with original <pre> block
    /// 4. Whitespace is preserved because it never went through DOM parsing
    ///
    /// Returns: HTML with placeholders replaced by original <pre> blocks
    pub fn restore(&self, html: &str) -> String {
        // Regex to match: <!--CITESCRAPE-PRE-{id}:{base64}-->
        // Captures: (1) id as \d+, (2) base64 data as [A-Za-z0-9+/=]+
        let comment_re = regex::Regex::new(
            r"<!--CITESCRAPE-PRE-(\d+):([A-Za-z0-9+/=]+)-->"
        ).expect("Comment regex is valid");
        
        comment_re.replace_all(html, |caps: &regex::Captures| {
            let _id = &caps[1];  // Not currently used, but available for debugging
            let encoded = &caps[2];
            
            // Decode base64 back to original HTML bytes
            match general_purpose::STANDARD.decode(encoded) {
                Ok(decoded_bytes) => {
                    // Convert bytes to UTF-8 string
                    match String::from_utf8(decoded_bytes) {
                        Ok(original_html) => {
                            // Successfully restored original <pre> block with all whitespace intact
                            original_html
                        },
                        Err(e) => {
                            // Invalid UTF-8 - return placeholder as-is (should never happen)
                            tracing::warn!("Failed to decode UTF-8 from base64 placeholder: {}", e);
                            caps[0].to_string()
                        }
                    }
                },
                Err(e) => {
                    // Invalid base64 - return placeholder as-is (should never happen)
                    tracing::warn!("Failed to decode base64 placeholder: {}", e);
                    caps[0].to_string()
                }
            }
        }).to_string()
    }

}

impl Default for CodeBlockProtector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_protect_preserves_spaces() {
        let html = r#"<pre><code>export CLAUDE_CODE_USE_BEDROCK=1</code></pre>"#;
        
        let mut protector = CodeBlockProtector::new();
        let protected = protector.protect(html);
        
        // Should be HTML comment placeholder
        assert!(protected.starts_with("<!--CITESCRAPE-PRE-0:"));
        assert!(protected.ends_with("-->"));
        
        // Should NOT contain the original HTML (it's base64 encoded)
        assert!(!protected.contains("<pre><code>"));
    }
    
    #[test]
    fn test_restore_decodes_spaces() {
        let html = r#"<pre><code>export AWS_REGION=us-east-1</code></pre>"#;
        
        let mut protector = CodeBlockProtector::new();
        
        // First protect to generate a valid placeholder
        let protected = protector.protect(html);
        
        // Then restore it
        let restored = protector.restore(&protected);
        
        // Should decode back to original HTML
        assert!(restored.contains("export AWS_REGION=us-east-1"));
        assert!(restored.contains("<pre><code>"));
    }
    
    #[test]
    fn test_end_to_end_whitespace_preservation() {
        let html = r#"<pre><code>npm install -g @anthropic-ai/claude-code</code></pre>"#;
        
        let mut protector = CodeBlockProtector::new();
        
        // Step 1: Protect (before DOM parsing)
        let protected = protector.protect(html);
        assert!(protected.starts_with("<!--CITESCRAPE-PRE-0:"));
        assert!(protected.ends_with("-->"));
        
        // Step 2: Simulate DOM parsing (this would normally collapse spaces)
        // For this test, we'll just pass through since actual DOM parsing
        // requires kuchiki which is in another module
        let after_dom = protected;
        
        // Step 3: Restore (after DOM parsing)
        let restored = protector.restore(&after_dom);
        assert!(restored.contains("npm install -g @anthropic-ai/claude-code"));
        assert!(restored.contains("<pre><code>"));
    }
    
    #[test]
    fn test_preserve_spaces_in_shell_commands() {
        // This is the critical bug fix: preserve spaces in shell commands
        let html = r#"<pre><code class="language-bash">export CLAUDE_CODE_USE_BEDROCK=1
export AWS_REGION=us-east-1  # or your preferred region</code></pre>"#;
        
        let mut protector = CodeBlockProtector::new();
        let protected = protector.protect(html);
        let restored = protector.restore(&protected);
        
        // CRITICAL: Spaces must be preserved exactly
        assert!(restored.contains("export CLAUDE_CODE_USE_BEDROCK=1"), "Space after 'export' must be preserved");
        assert!(restored.contains("export AWS_REGION=us-east-1"), "Space after 'export' must be preserved");
        assert!(restored.contains("us-east-1  #"), "Spaces before comment must be preserved");
    }
    
    #[test]
    fn test_preserve_newlines() {
        let html = r#"<pre><code>line1
line2
line3</code></pre>"#;
        
        let mut protector = CodeBlockProtector::new();
        let protected = protector.protect(html);
        let restored = protector.restore(&protected);
        
        // Newlines should be preserved exactly
        assert!(restored.contains("line1\nline2\nline3"));
    }
    
    #[test]
    fn test_multiple_code_blocks() {
        let html = r#"<pre><code>block1 content</code></pre><p>text</p><pre><code>block2 content</code></pre>"#;
        
        let mut protector = CodeBlockProtector::new();
        let protected = protector.protect(html);
        
        // Should have two HTML comment placeholders
        assert!(protected.contains("<!--CITESCRAPE-PRE-0:"));
        assert!(protected.contains("<!--CITESCRAPE-PRE-1:"));
        
        // Should NOT contain original blocks (they're base64 encoded)
        assert!(!protected.contains("block1 content"));
        assert!(!protected.contains("block2 content"));
        
        let restored = protector.restore(&protected);
        assert!(restored.contains("block1 content"));
        assert!(restored.contains("block2 content"));
    }
}
