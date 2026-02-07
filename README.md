# stagent

Interactive TUI for reviewing unstaged git changes hunk-by-hunk. Stage selectively and add edit/comment feedback. Requires tmux.

## Install

```bash
cargo install --path .
```

## Usage

```bash
# Run inside tmux
stagent

# Options
stagent --output review.txt    # Write feedback to file
stagent --no-stage             # Review-only mode (don't stage)
stagent --files "*.rs"         # Filter files by glob
stagent -N                     # Include untracked files (intent-to-add)
stagent -C 5                   # Context lines in feedback output
```

## Keybindings

| Key | Action |
|-----|--------|
| `j`/`k` or `↓`/`↑` | Navigate hunks |
| `Tab` | Toggle focus between file list and diff |
| `y` | Stage current hunk |
| `n` | Skip current hunk |
| `s` | Split current hunk |
| `e` | Edit hunk (opens `$EDITOR` in tmux split) |
| `c` | Add comment to hunk |
| `q` | Quit and output feedback |

## Output

On quit, stagent outputs:
- Edited hunks as unified diffs
- Comments prefixed with `# REVIEW COMMENT:`
