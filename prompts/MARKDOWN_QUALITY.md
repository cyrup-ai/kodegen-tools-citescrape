# MARDOWN QUALITY REVIEW

## Scope

- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/ratatui.rs/ratatui.rs/**/*.md
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/code.claude.com/code.claude.com/**/*.md

*NOTE*: these directories contain wildcard .gitignore so when using `mcp__kg_kodegen__fs_search` YOU MUST use hte noignore flag to find files

## Site agnostic 

- this tool is site agnostic and can crawl millions of diverse websites
- *NEVER* highlight issues or propose solutions that are not website agnostic or general in purpose or could apply to ANY GENERAL website
- Our goal is not to hyper-optimize crawling these specific websites, these are purely example representative websites with lots of technical documentation (our specialty)

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
