Now i've recrawled both websites that were audited for markdown quality:

- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/code.claude.com/code.claude.com/docs/en
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/ratatui.rs/ratatui.rs

For each subtask in /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/task/*.md related to markdown rendering quaity issues:

I want you to spawn 1 agent *per taskfile* with `run_in_background: false` in parallel. That means if there are 8 taskfiles, 8 agents in parallel, one per file in /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/task/*.md 

I want you to prompt them each with the following prompt:

```markdown
You are to perform a functional validation that the issues highlighted in {{taskfile}}
are fully and completely resolves based on the specific files in:

- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/code.claude.com/code.claude.com/docs/en
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/ratatui.rs/ratatui.rs

Both websites have been fully recrawled WITH THE FIXES the developer claimed were fully and truly resolved but now we have the actual evidence and this is where the rubber meets the road. Are we able to truly and fully see with evidence in the specific markdown files flagged as problematic that the issues have been fully and completely resolved.

If yes, I want you to delete the taskfile at {{taskfile}}.

If not, I want you to clearly UPDATE THE TASKFILE with STATUS: QA FAILED

and link to the specific evidence in:

- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/code.claude.com/code.claude.com/docs/en
- /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/docs/ratatui.rs/ratatui.rs

Add metadata with a timestamp and number of attempts 1 (if not present).
If number of attempts is already present, increment it by 1.

Do not try to solve the task. Simply update the taskfile based on the clear outcome evident in the non-controversial markdown rendered to disk.
```

remember, set `run_in_background: false` so you await the results of all QA. Once all reviews are completed, summarize the result for me.
