# stagent

Interactive TUI for reviewing unstaged git changes hunk-by-hunk. Stage selectively and add edit/comment feedback.

## Prerequisites

- **tmux** - must run inside a tmux session
- **Rust** - for building from source

## Install

```bash
cargo install --path .
```

## Usage

```bash
stagent                        # Review unstaged changes in tmux
stagent --output review.txt    # Write feedback to file
stagent --no-stage             # Review-only mode (no staging)
stagent --files "*.rs"         # Filter by glob
stagent -C 5                   # Context lines in output
stagent --spawn                # Spawn in tmux split (for tools)
git diff | stagent -p          # Review any diff from stdin
git diff feature..main | stagent -p   # Review cross-branch diff
```

Untracked files are automatically added with intent-to-add for hunk-by-hunk review.

### Patch mode (`-p` / `--patch`)

Pipe any unified diff into stagent for review and commenting:

```bash
git diff | stagent -p
git diff HEAD~3..HEAD | stagent -p
git diff feature..main | stagent --patch
```

Staging is disabled in patch mode (no git repo context). Use `y` to accept hunks, `e` to edit, and `c` to comment.

## Keys

| Key | Action |
|-----|--------|
| `j`/`k`, `↓`/`↑` | Navigate hunks |
| `Tab` | Toggle file list / diff focus |
| `y` | Stage hunk (accept in patch mode) |
| `n` | Skip hunk |
| `s` | Split hunk |
| `e` | Edit hunk (`$EDITOR` in tmux split) |
| `c` | Comment on hunk |
| `q` | Quit |

## Output

Outputs edited hunks as unified diffs and comments as `# REVIEW COMMENT:` lines.

## Claude Code Integration

```bash
cp -r agents/skills/stagent-review ~/.claude/skills/
```

Use `/stagent-review` during a Claude Code session for human-in-the-loop code review.
