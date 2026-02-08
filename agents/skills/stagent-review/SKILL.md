---
name: stagent-review
description: Launch stagent for interactive code review and staging
user-invocable: true
---

# Stagent Review

Launch stagent in a tmux split for the user to interactively review unstaged git changes.

## User Actions

The user can:
- **Stage approved hunks** (`y`) - add the hunk to the git index
- **Skip hunks** (`n`) - leave the hunk unstaged
- **Edit hunks** (`e`) - modify the code before staging
- **Add comments** (`c`) - leave feedback for Claude to address
- **Split hunks** (`s`) - break large hunks into smaller pieces
- **Quit** (`q`) - finish the review session

## Steps

1. Run stagent in spawn mode with feedback output:
   ```bash
   stagent --spawn --output /tmp/stagent-feedback.diff
   ```

2. Tell the user that stagent is now open in a split pane and explain the key bindings.

3. Wait for the command to complete (the user will stage hunks and quit when done).

4. Read `/tmp/stagent-feedback.diff` for any edit suggestions or comments the user left.

5. If feedback exists:
   - **Edits**: Apply the suggested changes to the staged code
   - **Comments**: Address the review comments on the staged code

6. Report what was staged and what feedback was processed.

## Example Output Format

The feedback file contains unified diff format for edits and positioned comments:

```diff
# FILE: src/main.rs
# HUNK: @@ -10,5 +10,6 @@ fn main()
# COMMENT at line 3: Consider adding error handling here
# COMMENT at line 5: This function name could be more descriptive
```

## Notes

- Requires tmux (stagent is a TUI that runs inside tmux)
- Only works with unstaged changes (use `git add -N` for new files)
- The `--spawn` flag handles opening the split and waiting for completion
