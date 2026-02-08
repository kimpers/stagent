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
stagent --spawn                # Spawn in tmux split and wait (for Claude/tools)
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

## Claude Code Integration

Stagent includes a skill for [Claude Code](https://claude.ai/code) that enables interactive code review during AI-assisted development.

### Setup

Copy the skill to your Claude Code skills directory:

```bash
cp -r agents/skills/stagent-review ~/.claude/skills/
```

### Usage

During a Claude Code session, use `/stagent-review` to launch an interactive review. Claude will:

1. Open stagent in a tmux split pane
2. Wait while you review and stage hunks
3. Read any edits or comments you leave
4. Apply your feedback to the code

This creates a human-in-the-loop workflow where you can guide Claude's code changes through direct review.
