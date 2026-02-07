# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                          # Build
cargo test                           # Run all tests (125 tests, 2 ignored)
cargo test --test git_diff_test      # Run a single test file
cargo test test_stage_single         # Run tests matching a name pattern
cargo test -- --ignored              # Run tmux-only integration tests (requires $TMUX)
cargo clippy -- -D warnings          # Lint (all clippy warnings are errors via Cargo.toml)
cargo install --path .               # Install binary
```

## Architecture

Stagent is an interactive TUI for reviewing unstaged git changes hunk-by-hunk, staging selectively, and collecting edit/comment feedback. Requires tmux.

### Data Flow

1. **Startup**: `main.rs` parses CLI args (clap), checks `$TMUX`, opens git repo via `git.rs`, gets unstaged diff
2. **Diff parsing**: `git.rs` calls `repo.diff_index_to_workdir()` → `diff.rs` uses `git2::Patch` API to build `Vec<FileDiff>` (avoids `diff.foreach()` multiple-mutable-borrow issues)
3. **TUI loop** (`app.rs::run()`): crossterm event loop renders via ratatui, dispatches to `App` methods
4. **Staging** (`staging.rs`): blob reconstruction approach (same as gitui) — read index blob → apply hunk → write new blob → update index
5. **Editor** (`editor.rs`): `tmux split-window` opens `$EDITOR`, background thread polls `tmux list-panes` to detect close
6. **Feedback** (`feedback.rs`): on quit, formats edits as unified diff and comments as `# REVIEW COMMENT:` lines

### Key Design Decisions

- **git2 only, no git CLI** — all git operations use libgit2 bindings
- **Patch API over `diff.foreach()`** — avoids closure borrow conflicts with shared mutable state
- **Hunk staging via blob reconstruction** — not `repo.apply()` (libgit2 apply has bugs). Read file from index, splice in hunk changes, write new blob, update index entry
- **Pane close detection via `tmux list-panes -a`** — not `#{pane_dead}` (unreliable on tmux 3.x when pane is destroyed immediately)
- **`flush_pending_editor_state()`** — handles race condition where user presses `q` before 500ms poll detects editor pane closed

### Module Responsibilities

- `types.rs` — all shared types: `FileDiff`, `Hunk`, `DiffLine`, `HunkStatus`, `AppMode`, `HunkFeedback`
- `app.rs` — `App` state struct + `run()` event loop. Navigation, staging dispatch, editor orchestration
- `git.rs` — `open_repo()`, `get_unstaged_diff()`
- `diff.rs` — `parse_diff()` (git2 Patch → FileDiff), `split_hunk()`
- `staging.rs` — `stage_hunk()`, `reconstruct_blob()` (pub for testing)
- `editor.rs` — tmux split lifecycle, tempfile prep, result parsing
- `highlight.rs` — syntect wrapper for syntax-highlighted diff lines
- `feedback.rs` — format `Vec<HunkFeedback>` as unified diff output
- `ui/` — ratatui widgets: `file_list`, `diff_view`, `status_bar`, `theme`

### Binary vs Library

Both `main.rs` and `lib.rs` exist. `lib.rs` re-exports all modules for integration tests. `main.rs` is the CLI entry point with its own `mod` declarations.
