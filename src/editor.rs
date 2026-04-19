use anyhow::{Context, Result};
use similar::TextDiff;
use std::io::Write;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::mux::{self, SplitHandle};
use crate::types::{DiffLine, FeedbackKind, Hunk, HunkFeedback, LineKind};

/// Build the editor command argv.
pub fn build_editor_command(editor: &str, file_path: &str) -> Vec<String> {
    vec![editor.to_string(), file_path.to_string()]
}

/// Build the tmux split-window command arguments for opening the editor.
pub fn build_tmux_split_command(editor: &str, file_path: &str) -> Vec<String> {
    mux::build_tmux_split_command(&build_editor_command(editor, file_path))
}

/// Build the command used to probe whether a tmux pane still exists.
pub fn build_pane_exists_check_command() -> Vec<String> {
    vec![
        "tmux".to_string(),
        "list-panes".to_string(),
        "-a".to_string(),
        "-F".to_string(),
        "#{pane_id}".to_string(),
    ]
}

/// Get the editor from environment, with fallback to vi.
pub fn get_editor() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string())
}

/// Open the editor in the preferred environment.
///
/// Prefers cmux splits, then tmux splits, and finally falls back to the current
/// terminal if no multiplexer is active.
pub fn open_editor(file_path: &str) -> Result<SplitHandle> {
    let editor = get_editor();
    let argv = build_editor_command(&editor, file_path);

    if let Some(handle) = mux::open_command_in_preferred_split(&argv)? {
        return Ok(handle);
    }

    run_editor_fullscreen(&argv)?;
    Ok(SplitHandle::Immediate)
}

/// Maximum number of poll iterations before giving up on pane close detection.
/// At 500ms per poll, this is ~5 minutes.
const MAX_PANE_POLL_ITERATIONS: u32 = 600;

/// Wait for an editor session to close by polling whether the underlying handle
/// still exists. Returns a receiver that signals when the session closes.
pub fn wait_for_editor_close(handle: SplitHandle) -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        if mux::poll_handle_close(
            &handle,
            MAX_PANE_POLL_ITERATIONS,
            Duration::from_millis(500),
        ) {
            let _ = tx.send(());
            return;
        }

        // Timeout: send signal anyway so the UI doesn't hang forever
        let _ = tx.send(());
    });

    rx
}

fn run_editor_fullscreen(argv: &[String]) -> Result<()> {
    let _suspend = CurrentTerminalEditorSession::suspend()?;
    std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .status()
        .with_context(|| format!("Failed to launch editor `{}`", argv[0]))?;
    Ok(())
}

struct CurrentTerminalEditorSession;

impl CurrentTerminalEditorSession {
    fn suspend() -> Result<Self> {
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
        )?;
        Ok(Self)
    }
}

impl Drop for CurrentTerminalEditorSession {
    fn drop(&mut self) {
        let _ = crossterm::terminal::enable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
        );
    }
}

/// Extract the "new side" content from hunk lines (context + added, skipping removed).
/// This is the content that represents the new version of the code.
pub fn extract_new_side_content(lines: &[DiffLine]) -> String {
    let mut content = String::new();
    for line in lines {
        match line.kind {
            LineKind::Context | LineKind::Added => {
                content.push_str(&line.content);
                if !line.content.ends_with('\n') {
                    content.push('\n');
                }
            }
            LineKind::Removed => {}
        }
    }
    content
}

/// Prepare a tempfile for editing a hunk.
/// Contains the new-side code (context + added lines, not removed lines).
pub fn prepare_edit_tempfile(hunk: &Hunk) -> Result<tempfile::NamedTempFile> {
    let mut tmpfile = tempfile::Builder::new()
        .prefix("stagent-edit-")
        .suffix(".tmp")
        .tempfile()
        .context("Failed to create temp file")?;

    let content = extract_new_side_content(&hunk.lines);
    write!(tmpfile, "{}", content)?;
    tmpfile.flush()?;
    Ok(tmpfile)
}

/// Prepare a tempfile for commenting on a hunk.
/// Contains the full hunk with `# COMMENT:` instruction markers.
pub fn prepare_comment_tempfile(hunk: &Hunk) -> Result<tempfile::NamedTempFile> {
    let mut tmpfile = tempfile::Builder::new()
        .prefix("stagent-comment-")
        .suffix(".tmp")
        .tempfile()
        .context("Failed to create temp file")?;

    writeln!(tmpfile, "# Add your comments anywhere in this file.")?;
    writeln!(
        tmpfile,
        "# Any new lines you add will be captured as comments."
    )?;
    writeln!(tmpfile, "# {}", hunk)?;
    writeln!(tmpfile)?;

    for line in &hunk.lines {
        write!(tmpfile, "{}{}", line.kind.prefix(), line.content)?;
        if !line.content.ends_with('\n') {
            writeln!(tmpfile)?;
        }
    }

    tmpfile.flush()?;
    Ok(tmpfile)
}

/// Parse the result of an edit operation by diffing original vs edited content.
pub fn parse_edit_result(
    original: &str,
    edited: &str,
    file_path: &str,
    hunk_header: &str,
    hunk_lines: &[crate::types::DiffLine],
) -> Option<HunkFeedback> {
    if original == edited {
        return None;
    }

    let diff = TextDiff::from_lines(original, edited);
    let mut unified = String::new();

    for hunk in diff.unified_diff().iter_hunks() {
        unified.push_str(&hunk.to_string());
    }

    if unified.is_empty() {
        return None;
    }

    Some(HunkFeedback {
        file_path: file_path.to_string(),
        hunk_header: hunk_header.to_string(),
        kind: FeedbackKind::Edit,
        content: unified,
        context_lines: hunk_lines.to_vec(),
        comment_positions: vec![],
    })
}

/// Compare an edited line against an original template line.
/// Uses trim_end() fallback to handle editors that strip trailing whitespace.
fn lines_match(edited: &str, original: &str) -> bool {
    edited == original || edited.trim_end() == original.trim_end()
}

/// Parse comment content from an edited comment tempfile.
///
/// Detects user comments by comparing the original template with the edited
/// version. Any new line that wasn't in the original template is treated as
/// a comment. Lines with `# COMMENT:` prefix have the prefix stripped for
/// backward compatibility.
pub fn parse_comment_result(
    original: &str,
    edited: &str,
    file_path: &str,
    hunk_header: &str,
    hunk_lines: &[crate::types::DiffLine],
) -> Option<HunkFeedback> {
    // Extract the "body" lines from both original and edited.
    // Body = everything after the preamble (instruction lines).
    // The preamble ends at the first empty line in the original.
    let original_lines: Vec<&str> = original.lines().collect();
    let edited_lines: Vec<&str> = edited.lines().collect();

    // Find where the body starts in the original (after the empty line separator)
    let body_start = original_lines
        .iter()
        .position(|l| l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);

    let original_body = &original_lines[body_start..];
    let edited_body: Vec<&str> = if body_start < edited_lines.len() {
        edited_lines[body_start..].to_vec()
    } else {
        edited_lines.clone()
    };

    // Walk through edited body, matching against original body lines.
    // Unmatched non-empty lines are comments. Track position as the index
    // of the last matched hunk line.
    let mut orig_idx = 0;
    let mut positioned_comments: Vec<(usize, String)> = Vec::new();
    let mut all_comment_text = Vec::new();

    for edited_line in &edited_body {
        // Try to match at the current position first
        if orig_idx < original_body.len() && lines_match(edited_line, original_body[orig_idx]) {
            orig_idx += 1;
            continue;
        }

        // Look ahead in original_body to handle deleted/replaced lines.
        // If the user replaced a template line with a comment (e.g. `cc` in
        // vim on an empty context line), the original line is gone from the
        // edited version.  Skipping past it prevents orig_idx from getting
        // stuck and treating all subsequent lines as comments.
        let mut matched_ahead = false;
        if !edited_line.trim().is_empty() {
            for (j, orig_line) in original_body.iter().enumerate().skip(orig_idx + 1) {
                if lines_match(edited_line, orig_line) {
                    orig_idx = j + 1;
                    matched_ahead = true;
                    break;
                }
            }
        }

        if !matched_ahead && !edited_line.trim().is_empty() {
            // This is a user comment at position orig_idx (after orig_idx-1)
            let text = if let Some(stripped) = edited_line.strip_prefix("# COMMENT:") {
                stripped.trim()
            } else {
                edited_line.trim()
            };
            if !text.is_empty() {
                // Map orig_idx back to hunk line index.
                // orig_idx is the count of body lines matched so far,
                // which corresponds to the hunk line index the comment follows.
                let hunk_pos = orig_idx.min(hunk_lines.len());
                positioned_comments.push((hunk_pos, text.to_string()));
                all_comment_text.push(text.to_string());
            }
        }
    }

    if positioned_comments.is_empty() {
        return None;
    }

    Some(HunkFeedback {
        file_path: file_path.to_string(),
        hunk_header: hunk_header.to_string(),
        kind: FeedbackKind::Comment,
        content: all_comment_text.join("\n"),
        context_lines: hunk_lines.to_vec(),
        comment_positions: positioned_comments,
    })
}
