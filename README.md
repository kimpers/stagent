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
stagent                        # Run inside tmux
stagent --output review.txt    # Write feedback to file
stagent --no-stage             # Review-only mode
stagent --files "*.rs"         # Filter by glob
stagent -C 5                   # Context lines in output
stagent --spawn                # Spawn in tmux split (for tools)
```

Untracked files are automatically added with intent-to-add for hunk-by-hunk review.

## Keys

| Key | Action |
|-----|--------|
| `j`/`k`, `↓`/`↑` | Navigate hunks |
| `Tab` | Toggle file list / diff focus |
| `y` | Stage hunk |
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
