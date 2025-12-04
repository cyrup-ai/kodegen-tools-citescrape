/// Test with actual HTML from the scraped ratatui.rs documentation
/// This HTML shows the real-world Expressive Code structure that was causing issues
use kodegen_tools_citescrape::content_saver::markdown_converter::{convert_html_to_markdown_sync, ConversionOptions};

#[test]
fn test_real_scraped_html_with_expressive_code() {
    // This is the actual HTML structure from ratatui.rs docs
    // It has the Expressive Code structure with a button containing data-code attribute
    let html = r#"<div class="expressive-code"><link rel="stylesheet" href="/_astro/ec.0bye2.css"><script type="module" src="/_astro/ec.p1z7b.js"></script><figure class="frame has-title not-content"><figcaption class="header"><span class="title">main.rs</span></figcaption><pre data-language="rust"><code><div class="ec-line"><div class="code"><span style="--0:#C792EA;--1:#8844AE">pub</span><span style="--0:#D6DEEB;--1:#403F53"> </span><span style="--0:#C792EA;--1:#8844AE">fn</span><span style="--0:#D6DEEB;--1:#403F53"> </span><span style="--0:#82AAFF;--1:#3B61B0">main</span><span style="--0:#D6DEEB;--1:#403F53">() </span><span style="--0:#7FDBCA;--1:#096E72">-&gt;</span><span style="--0:#D6DEEB;--1:#403F53"> io</span><span style="--0:#7FDBCA;--1:#096E72">::</span><span style="--0:#D6DEEB;--1:#403F53">Result&lt;()&gt; {</span></div></div><div class="ec-line"><div class="code"><span class="indent">    </span><span style="--0:#82AAFF;--1:#3B61B0">init_panic_hook</span><span style="--0:#D6DEEB;--1:#403F53">();</span></div></div><div class="ec-line"><div class="code"><span class="indent">    </span><span style="--0:#C792EA;--1:#8844AE">let</span><span style="--0:#D6DEEB;--1:#403F53"> </span><span style="--0:#C792EA;--1:#8844AE">mut</span><span style="--0:#D6DEEB;--1:#403F53"> </span><span style="--0:#C5E478;--1:#3B61B0">tui</span><span style="--0:#D6DEEB;--1:#403F53"> </span><span style="--0:#C792EA;--1:#8844AE">=</span><span style="--0:#D6DEEB;--1:#403F53"> </span><span style="--0:#82AAFF;--1:#3B61B0">init_tui</span><span style="--0:#D6DEEB;--1:#403F53">()</span><span style="--0:#7FDBCA;--1:#096E72">?</span><span style="--0:#D6DEEB;--1:#403F53">;</span></div></div><div class="ec-line"><div class="code"><span style="--0:#D6DEEB;--1:#403F53">}</span></div></div></code></pre><div class="copy"><button title="Copy to clipboard" data-copied="Copied!" data-code="pub fn main() -&gt; io::Result&lt;()&gt; {    init_panic_hook();    let mut tui = init_tui()?;}" data-citescrape-interactive="interactive-220"><div></div></button></div></figure></div>"#;

    let options = ConversionOptions::default();
    let markdown = convert_html_to_markdown_sync(html, &options).unwrap();

    println!("=== REAL SCRAPED HTML OUTPUT ===");
    println!("{}", markdown);
    println!("=== END OUTPUT ===\n");

    // The button has data-code attribute with the CLEAN code (this should be extracted)
    // Expected: properly formatted code with newlines
    // If the preprocess_expressive_code is working correctly, it should use the data-code attribute

    // Check that we got a code block
    assert!(markdown.contains("```"), "Should have code fence");

    // The data-code attribute has spaces instead of actual newlines (that's the source format)
    // So if the code is extracted from data-code, it will have the compact format
    // BUT the ec-line extraction should give us proper newlines

    // Let's check what we actually got:
    if markdown.contains("{\n    init_panic_hook();") || markdown.contains("{\r\n    init_panic_hook();") {
        println!("✅ Code has proper newlines (extracted from ec-line elements)");
    } else if markdown.contains("{    init_panic_hook();    let mut tui") {
        panic!("❌ BUG: Code is on one line (extracted from data-code button attribute which has spaces not newlines)");
    } else {
        println!("⚠️  Unexpected format: {:?}", markdown);
    }
}
