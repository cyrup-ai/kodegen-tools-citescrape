For each subtask in /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/task/*.md related to markdown rendering quaity issues:

I want you to spawn 1 agent *per taskfile* with `run_in_background: false` in parallel. That means if there are 8 taskfiles, 8 agents in parallel, one per file in /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/task/*.md 

I want you to prompt them each with the following prompt:

```markdown
# AUGMENT TASK DETAILS FILE

## SOURCES 

- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/tmp/chromiumoxide
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/tmp/htmd-rust
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/tmp/fancy-regex

*Review* and _Research_ the Task Assignment

TASK FILE:
`{{absolute_file_path}}`

This task `{{absolute_file_path}}` failed qa after cargo was cleaned and recrawled with:

`cargo clean && cargo run --example ratatui && cargo run --example claude_code`

The evidence of the failure is plainly visible in:

- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/code.claude.com/code.claude.com/docs/en
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/ratatui.rs/ratatui.rs

Both websites have been fully recrawled WITH THE FIXES the developer claimed were fully and truly resolved but now we have the actual evidence and this is where the rubber meets the road. It didn't work.

**NOTICE: If you come back with "it's already fixed" you will be immediately fired**

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

**NOTICE: If you come back with "it's already fixed" you will be immediately fired**
```

remember, set `run_in_background: false` so you await the results of all task augmentation. Once all augmentations are completed, summarize the result for me, analyzing for any fabricated solutions or any hint of "it already works".
