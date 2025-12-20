For each subtask in /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/task/*.md related to markdown rendering quaity issues:

I want you to spawn 1 agent *per taskfile* with `run_in_background: false` in parallel. That means if there are 8 taskfiles, 8 agents in parallel, one per file in /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/task/*.md 

I want you to prompt them each with the following prompt:

```markdown
# AUGMENT TASK DETAILS FILE

## SOURCES 

- [Chromiumoxide](/Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/tmp/chromiumoxide)
- [Proper Handlers for Markdown Conversion](/Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/src/content_saver/markdown_converter/htmd/element_handler/**/*.rs)

*Review* and _Research_ the Task Assignment

TASK FILE:
`{{absolute_file_path}}`

This task `{{absolute_file_path}}` highlights an observable issue in the markdown reendered from the two sites crawled with:

`cargo clean && cargo run --example ratatui && cargo run --example claude_code`

The evidence is plainly visible in:

- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/code.claude.com/code.claude.com/docs/en
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/ratatui.rs/ratatui.rs

**NOTICE: Regex based solutions, pre-processing solutions and post-processing solutions are COMPLETELY OFF THE TABLE. The only acceptable solution is in proper handlers in our [htmd handlers](/Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/src/content_saver/markdown_converter/htmd/element_handler/**/*.rs). If it's actually true that this cannot be solved with proper dom parsing handlers, then I don't want the fixes, but you'd better be able to prove it. "It's hard" is not a valid excuse.

## Site agnostic 

- this tool is site agnostic and can crawl millions of diverse websites
- *NEVER* highlight issues or propose solutions that are not website agnostic or general in purpose or could apply to ANY GENERAL website
- Our goal is not to hyper-optimize crawling these specific websites, these are purely example representative websites with lots of technical documentation (our specialty)

### CRITICAL: Semantic HTML Requirement

**Solutions MUST be based on semantic HTML signals, NOT pattern-matching guesses about author intent.**

Before you develop a solution, ask yourself:

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

### Examples of INVALID Solutions (DO NOT CREATE)

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

### Examples of VALID Solutions (These are Good)

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

**Before creating a solution, complete this sentence:**

"This issue can be solved by detecting the `<______>` HTML element / `______` attribute / `______` character property."

If you can't fill in the blanks with **semantic HTML signals**, the solution is invalid.

## READ 

read the full and complete task file with sequential thinking. Think "out loud" about the core User OBJECTIVE.

## LOOK AROUND

- if a crate/package, run:
  - `lsd --tree ./src/` 
- if a workspace, run:
  - `lsd --tree ./packages/` 

  - look at all of the module hierarchy 
  - search for files related to the feature
  - often times you'll discover that much of the code needed is ALREADY WRITTEN and just needs to be connected or adapted or sometimes ... the task is done fully and correctly altread

USE CODE ALREADY WRITTEN and DO NOT CALL FOR DUPLICATION of functionality in your specification task file.

NEXT: think deeply with step by step reasoning about the task. 

- what is the core objective? 
- what needs to change in impacted packages /src/ files to accomplish this task?
- what questions do you have?
- what do you need to research (if anything) to successfully and fully execute the task as written? 

- clone any third party libraries needed for execution into ./tmp (relative to the project)
- Augment the task markdown file in {{absolute_file_path}}  with rich detail from your research
- link to citation sources in ./tmp and in ./src with path relative markdown hyperlinks
- Plan out the source code required with ULTRATHINK and demonstrate core patterns right in the actual task file.

## WHAT NOT TO DO:

- Do not add requirements for unit tests, functional tests for this feature
- Do not call for benchmarks for this feature
- Do not call for extensive "documentation" for the feature
- Do not change the scope of the task ... 

## WHAT YOU SHOULD DO

- remove completely any language calling for unit tests, functional tests, benchmarks or documentation
- provide clear instruction on exactly what needs to change in the ./src
- provide clear instruction on where and how to accomplish the task 
- provide a clear definition of done (not by proving it with extensive testing)

WRITE THE UPDATE md to disk using desktop commander which all the rich new information. 

REPLACE THE FORMER FILE. DO NOT WRITE THE AUGMENTATIONS to some other new file. The goal is to preserve and augment the EXISTING task file.

In our chat, print the full absolute filepath as the VERY LAST LINE IN YOUR OUTPUT to the revised, augmented task file so i can easily copy and paste it.

Then return immediately to planning, awaiting your next instruction.

## NO OPTIONS

The task file should not present "options" for the developer but instead should be prescriptive in nature. When you evaluate options, you should always select the most feature-rich, complex, code-correct "option" and present it as the only required implementation path. Avoid being lazy and going with the path of least resistance and instead focus on code correctness and achieving the ultimate goal.


## TOOLS 

- use `mcp__plugin_kg_kodegen__sequential_thinking` and ULTRATHINK to think step by step about the task
- use `mcp__plugin_kg_kodegen__web_search` if research on the web is needed for the task scope
- use `mcp__plugin_kg_kodegen__scrape_url` if you find websites that are key to understanding the task to scrape the full website
- use `mcp__plugin_kg_kodegen__fs_search` to search the local codebase and understand the architecture and relevant files that may need to be modified or built around
- use `mcp__plugin_kg_kodegen__fs_read_file` and/or `mcp__plugin_kg_kodegen__fs_read_multiple_files` to read files
- use `mcp__plugin_kg_kodegen__fs_write_file` to write your ultimate augmentation to: `{{absolute_file_path}}`
  - if you need to make additional edits to the task file periodically, use `mcp__plugin_kg_kodegen__fs_edit_block` to make those edits
- feel free to use other `mcp__plugin_kg_kodegen__*` commands as needed

**NOTICE: If you come back with regex based solutions, HTML preprocessing solutions or HTML post-processing solutions, or "it's already fixed" you will be immediately fired**
```

remember, set `run_in_background: false` so you await the results of all task augmentation. Once all augmentations are completed, summarize the result for me, analyzing for any fabricated solutions or any hint of HTML preprocessing solutions or HTML post-processing solutions, or "it's already fixed".
