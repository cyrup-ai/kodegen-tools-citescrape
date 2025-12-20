# MARDOWN QUALITY REVIEW

## Scope

- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/ratatui.rs/ratatui.rs/**/*.md
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/code.claude.com/code.claude.com/**/*.md

*NOTE*: these directories contain wildcard .gitignore so when using `mcp__kg_kodegen__fs_search` YOU MUST use hte noignore flag to find files

## Site agnostic 

- this tool is site agnostic and can crawl millions of diverse websites
- *NEVER* highlight issues or propose solutions that are not website agnostic or general in purpose or could apply to ANY GENERAL website
- Our goal is not to hyper-optimize crawling these specific websites, these are purely example representative websites with lots of technical documentation (our specialty)

### CRITICAL: Semantic HTML Requirement

**Issues MUST be based on semantic HTML signals, NOT pattern-matching guesses about author intent.**

Before creating a task file, ask yourself:

1. **Does the HTML provide semantic markup for this feature?**
   - Lists: `<ol>`, `<ul>`, `<li>` elements
   - Headings: `<h1>`-`<h6>` elements
   - Code: `<pre>`, `<code>` elements
   - Emphasis: `<em>`, `<strong>` elements
   - Media: `<video>`, `<audio>`, `<img>` elements
   - Admonitions: class attributes like `admonition`, `callout`, `alert`

2. **Can the issue be solved WITHOUT inferring author intent from visual patterns?**
   - ✅ VALID: "Video elements contain fallback text in markdown" (semantic: `<video>` tag exists)
   - ❌ INVALID: "Standalone numbers should become numbered lists" (guessing: no `<ol>` tag)

3. **Will the solution work across millions of diverse websites?**
   - ✅ VALID: Detecting UI chrome via ARIA `role="button"` (standard attribute)
   - ❌ INVALID: Detecting lists by looking for "1\n\nText\n\n2\n\nText" patterns (brittle)

### Examples of INVALID Issues (DO NOT CREATE)

#### ❌ INVALID EXAMPLE: "Standalone numbers should become numbered lists"

**Why invalid:**
- HTML has no `<ol>` or `<ul>` elements - no semantic signal
- Trying to infer "this looks like a list" from standalone numbers is guessing
- Would require brittle pattern matching (number + blank line + text)
- Breaks on countless edge cases across millions of websites
- **If the website author didn't use list markup, they didn't want a list**

**Correct approach:**
- If HTML lacks semantic markup, the markdown should reflect that
- Don't try to "fix" bad HTML by guessing intent
- Only convert elements that have clear semantic meaning

#### ❌ INVALID EXAMPLE: "Text in quotes should become blockquotes"

**Why invalid:**
- Quotation marks are visual styling, not semantic markup
- Blockquotes require `<blockquote>` element in HTML
- Can't distinguish actual quotes from scare quotes, dialogue, etc.
- Would break on millions of websites using quotes differently

#### ❌ INVALID EXAMPLE: "Bold text followed by colon should become heading"

**Why invalid:**
- `<strong>` is not `<h1>-<h6>` - different semantic meaning
- Pattern matching "bold + colon" is guessing structure from styling
- Authors use bold for emphasis, not to indicate headings
- Would create false headings across millions of websites

### Examples of VALID Issues (These are Good)

#### ✅ VALID: "Zero-width space in heading anchor links"

**Why valid:**
- HTML has `<h2><a href="#id">​</a>Text</h2>` - semantic heading element exists
- Anchor has invisible Unicode (U+200B) - detectable character property
- Solution: Filter anchors containing only invisible Unicode
- Works universally - invisible characters are invisible on all websites

#### ✅ VALID: "UI button text appears before code blocks"

**Why valid:**
- HTML has `<span class="copy-button">Copy</span>` or `<a role="button">Copy</a>`
- Semantic signals: ARIA `role="button"`, class names with "button", "copy"
- Solution: Extend existing `is_widget_element()` to check ARIA roles
- Works across frameworks - ARIA roles are standardized

#### ✅ VALID: "Video fallback text appears in markdown"

**Why valid:**
- HTML has `<video><source src="...">Fallback text</video>`
- Semantic element: `<video>` tag clearly identifies media
- Solution: Process only `<source>` elements, ignore text nodes
- Works universally - HTML5 media spec is standard

### The Litmus Test

**Before creating a task, complete this sentence:**

"This issue can be solved by detecting the `<______>` HTML element / `______` attribute / `______` character property."

If you can't fill in the blanks with **semantic HTML signals**, the issue is invalid.

## Review markdown rendering

Start by reviewing markdown rendering one page at a a time. SCRUTINIZE the markdown quality looking for:

- lint invalid markdown generation
- missing elements 
- garbled content
- incorrect markdown in any form

## Create task files immediately

- when you find an issue, create a taskfile for it in `./packages/kodegen-tools-citescrape/task/*.md`
- don't wait and hold issues and then create a bunch of task files at the end
- DO REVIEW the other taskfiles and make sure your issue is not a duplicate of an existing task file
- IF DUPLICATE, add your finding as an ADDITIONAL HELPFUL EXAMPLE in the taskfile already generated.
- ALL TASKFILES must bear the ## Site Agnostic information so the coder understands we're not looking for solutions that don't scale to millions of websites 

## Continue reviewing in depth

Keep reviewing over many multiple sessions. Don't just find a handful of issues and call it a day, we're looking for detailed review.

## Once you've found most issues

Eventually you'll reach a point of diminishing returns where new issues aren't being frequently found. Once you do, go back and look at the HTML files that were used to generate the markdown that's problematic.

Then switch to a mode of HTML vs Markdown analyis. The .html files are always sister files to the markdown so they are easy to locate

CLEARLY IDENTIFY:

- is this issue caused by defective HTML->Markdown conversion?
- OR is the issue actually in the HTML itself and the markdown is just bearing the signature of the bad HTML?
- update an annotate each task with this detail

## DO NOT 

- guess or speculate about the likely cause of the issues 
- write down anything but provable, factual evidence of the manifestation of the issue
- write prescriptive solutions for the problem
- speculate about where to look in the codebase to solve the issue
