/// Regression test: comment feedback must be captured even when user presses
/// `q` immediately after the editor closes.
///
/// Bug: The editor close detection ran on a 500ms poll in a background thread.
/// If the user pressed `q` within that window, the event loop hit the
/// `WaitingForEditor` early-exit path (`break Ok(app.feedback)`) before the
/// editor result was processed, so `app.feedback` was empty.
///
/// The fix: when quitting from `WaitingForEditor`, the pending editor result
/// must be flushed (read the tempfile and process it) before returning feedback.
use stagent::editor;
use stagent::types::*;

/// Test that `parse_comment_result` correctly captures user comments from
/// a comment tempfile after the user has edited it.
/// This is the unit-level verification that the pipeline works.
#[test]
fn test_comment_content_round_trip() {
    let hunk = Hunk {
        header: "@@ -1,3 +1,4 @@".to_string(),
        lines: vec![
            DiffLine {
                kind: LineKind::Context,
                content: "line1\n".to_string(),
                old_lineno: Some(1),
                new_lineno: Some(1),
            },
            DiffLine {
                kind: LineKind::Removed,
                content: "old\n".to_string(),
                old_lineno: Some(2),
                new_lineno: None,
            },
            DiffLine {
                kind: LineKind::Added,
                content: "new\n".to_string(),
                old_lineno: None,
                new_lineno: Some(2),
            },
        ],
        status: HunkStatus::Pending,
        old_start: 1,
        old_lines: 3,
        new_start: 1,
        new_lines: 4,
    };

    // Prepare the comment tempfile (what the TUI creates before opening vim)
    let tmpfile = editor::prepare_comment_tempfile(&hunk).unwrap();

    // Simulate what the user would do: read the file, add a comment, write it back
    let original = std::fs::read_to_string(tmpfile.path()).unwrap();
    let mut edited = original.clone();
    edited.push_str("# COMMENT: This change looks good but needs a test\n");
    std::fs::write(tmpfile.path(), &edited).unwrap();

    // Simulate what the app does after the editor closes: read and parse
    let content = std::fs::read_to_string(tmpfile.path()).unwrap();
    let feedback =
        editor::parse_comment_result(&original, &content, "src/main.rs", "@@ -1,3 +1,4 @@", &[]);

    assert!(
        feedback.is_some(),
        "Feedback should be captured from comment file"
    );
    let fb = feedback.unwrap();
    assert_eq!(fb.kind, FeedbackKind::Comment);
    assert!(
        fb.content
            .contains("This change looks good but needs a test"),
        "Comment text should be in feedback, got: {}",
        fb.content
    );
}

/// Test the scenario that caused the bug: the app has a pending editor state
/// (tmpfile has been written by the user) but the pane close hasn't been
/// detected yet. When we flush the pending editor state, the feedback should
/// still be captured.
#[test]
fn test_flush_pending_comment_captures_feedback() {
    use stagent::app::App;

    let files = vec![FileDiff {
        path: "src/main.rs".into(),
        hunks: vec![Hunk {
            header: "@@ -1,3 +1,4 @@".to_string(),
            lines: vec![
                DiffLine {
                    kind: LineKind::Context,
                    content: "ctx\n".to_string(),
                    old_lineno: Some(1),
                    new_lineno: Some(1),
                },
                DiffLine {
                    kind: LineKind::Removed,
                    content: "old\n".to_string(),
                    old_lineno: Some(2),
                    new_lineno: None,
                },
                DiffLine {
                    kind: LineKind::Added,
                    content: "new\n".to_string(),
                    old_lineno: None,
                    new_lineno: Some(2),
                },
            ],
            status: HunkStatus::Pending,
            old_start: 1,
            old_lines: 3,
            new_start: 1,
            new_lines: 4,
        }],
        status: DeltaStatus::Modified,
        is_binary: false,
    }];

    let mut app = App::new(files, true);
    app.mode = AppMode::WaitingForEditor;

    // Create a tempfile simulating what prepare_comment_tempfile + user editing produces
    let tmpfile = editor::prepare_comment_tempfile(app.current_hunk().unwrap()).unwrap();
    let original = std::fs::read_to_string(tmpfile.path()).unwrap();
    let mut edited = original.clone();
    edited.push_str("# COMMENT: needs error handling\n");
    std::fs::write(tmpfile.path(), &edited).unwrap();

    // Call flush_pending_editor_state — this is the function that should exist
    // to handle the race condition where q is pressed before pane close is detected
    app.flush_pending_editor_state(tmpfile.path(), true, &original);

    assert!(
        !app.feedback.is_empty(),
        "Feedback should have been captured by flush_pending_editor_state"
    );
    assert_eq!(app.feedback[0].kind, FeedbackKind::Comment);
    assert!(
        app.feedback[0].content.contains("needs error handling"),
        "Comment text should be preserved, got: {}",
        app.feedback[0].content
    );
}

/// BUG REPRO: User writes a plain text comment (no `# COMMENT:` prefix)
/// in the comment tempfile. `parse_comment_result` returns None because
/// it only looks for `# COMMENT:` prefixed lines. The TUI says "Comment
/// captured" (unconditionally) but nothing is in the feedback, so nothing
/// is printed on quit.
#[test]
fn test_plain_text_comment_is_captured() {
    let hunk = Hunk {
        header: "@@ -1,3 +1,4 @@".to_string(),
        lines: vec![
            DiffLine {
                kind: LineKind::Context,
                content: "line1\n".to_string(),
                old_lineno: Some(1),
                new_lineno: Some(1),
            },
            DiffLine {
                kind: LineKind::Removed,
                content: "old\n".to_string(),
                old_lineno: Some(2),
                new_lineno: None,
            },
            DiffLine {
                kind: LineKind::Added,
                content: "new\n".to_string(),
                old_lineno: None,
                new_lineno: Some(2),
            },
        ],
        status: HunkStatus::Pending,
        old_start: 1,
        old_lines: 3,
        new_start: 1,
        new_lines: 4,
    };

    let tmpfile = editor::prepare_comment_tempfile(&hunk).unwrap();
    let original = std::fs::read_to_string(tmpfile.path()).unwrap();

    // User just types plain text — no # COMMENT: prefix
    let mut edited = original.clone();
    edited.push_str("This looks good but needs a test\n");
    std::fs::write(tmpfile.path(), &edited).unwrap();

    let content = std::fs::read_to_string(tmpfile.path()).unwrap();
    let feedback =
        editor::parse_comment_result(&original, &content, "src/main.rs", "@@ -1,3 +1,4 @@", &[]);

    assert!(feedback.is_some(), "Plain text comment should be captured");
    let fb = feedback.unwrap();
    assert_eq!(fb.kind, FeedbackKind::Comment);
    assert!(
        fb.content.contains("This looks good but needs a test"),
        "Plain text comment should be in feedback, got: {}",
        fb.content
    );
}

/// Verify that comments with the old `# COMMENT:` prefix still work
/// (backward compat) and the prefix is stripped from output.
#[test]
fn test_prefixed_comment_still_works() {
    let hunk = Hunk {
        header: "@@ -1,3 +1,4 @@".to_string(),
        lines: vec![DiffLine {
            kind: LineKind::Context,
            content: "line1\n".to_string(),
            old_lineno: Some(1),
            new_lineno: Some(1),
        }],
        status: HunkStatus::Pending,
        old_start: 1,
        old_lines: 3,
        new_start: 1,
        new_lines: 4,
    };

    let tmpfile = editor::prepare_comment_tempfile(&hunk).unwrap();
    let original = std::fs::read_to_string(tmpfile.path()).unwrap();

    let mut edited = original.clone();
    edited.push_str("# COMMENT: This is a prefixed comment\n");
    std::fs::write(tmpfile.path(), &edited).unwrap();

    let content = std::fs::read_to_string(tmpfile.path()).unwrap();
    let feedback =
        editor::parse_comment_result(&original, &content, "src/main.rs", "@@ -1,3 +1,4 @@", &[]);

    assert!(feedback.is_some(), "Prefixed comment should be captured");
    let fb = feedback.unwrap();
    assert!(
        fb.content.contains("This is a prefixed comment"),
        "Prefix should be stripped, got: {}",
        fb.content
    );
    // The `# COMMENT:` prefix should be stripped
    assert!(
        !fb.content.contains("# COMMENT:"),
        "Prefix should not be in output, got: {}",
        fb.content
    );
}

/// Verify no feedback is produced when the user makes no changes.
#[test]
fn test_no_changes_produces_no_feedback() {
    let hunk = Hunk {
        header: "@@ -1,3 +1,4 @@".to_string(),
        lines: vec![DiffLine {
            kind: LineKind::Context,
            content: "line1\n".to_string(),
            old_lineno: Some(1),
            new_lineno: Some(1),
        }],
        status: HunkStatus::Pending,
        old_start: 1,
        old_lines: 3,
        new_start: 1,
        new_lines: 4,
    };

    let tmpfile = editor::prepare_comment_tempfile(&hunk).unwrap();
    let original = std::fs::read_to_string(tmpfile.path()).unwrap();

    // User makes no changes — just saves and quits
    let feedback =
        editor::parse_comment_result(&original, &original, "src/main.rs", "@@ -1,3 +1,4 @@", &[]);
    assert!(feedback.is_none(), "No changes should produce no feedback");
}

/// Verify that comments inserted at different positions within the hunk
/// are tracked with correct positions for inline rendering.
#[test]
fn test_positioned_comments_in_hunk() {
    let hunk = Hunk {
        header: "@@ -1,5 +1,5 @@".to_string(),
        lines: vec![
            DiffLine {
                kind: LineKind::Context,
                content: "line1\n".to_string(),
                old_lineno: Some(1),
                new_lineno: Some(1),
            },
            DiffLine {
                kind: LineKind::Removed,
                content: "old_a\n".to_string(),
                old_lineno: Some(2),
                new_lineno: None,
            },
            DiffLine {
                kind: LineKind::Added,
                content: "new_a\n".to_string(),
                old_lineno: None,
                new_lineno: Some(2),
            },
            DiffLine {
                kind: LineKind::Context,
                content: "line3\n".to_string(),
                old_lineno: Some(3),
                new_lineno: Some(3),
            },
            DiffLine {
                kind: LineKind::Context,
                content: "line4\n".to_string(),
                old_lineno: Some(4),
                new_lineno: Some(4),
            },
        ],
        status: HunkStatus::Pending,
        old_start: 1,
        old_lines: 5,
        new_start: 1,
        new_lines: 5,
    };

    let tmpfile = editor::prepare_comment_tempfile(&hunk).unwrap();
    let original = std::fs::read_to_string(tmpfile.path()).unwrap();

    // User adds a comment after the change and another after line3
    let original_lines: Vec<&str> = original.lines().collect();
    let mut edited_lines: Vec<String> = original_lines.iter().map(|l| l.to_string()).collect();

    // Find the +new_a line and insert comment after it
    let new_a_idx = edited_lines
        .iter()
        .position(|l| l.starts_with("+new_a"))
        .unwrap();
    edited_lines.insert(new_a_idx + 1, "First change looks good".to_string());

    // Find the line3 line (after insertion, index shifted by 1)
    let line3_idx = edited_lines
        .iter()
        .position(|l| l.starts_with(" line3"))
        .unwrap();
    edited_lines.insert(line3_idx + 1, "But this context needs review".to_string());

    let edited = edited_lines.join("\n") + "\n";
    std::fs::write(tmpfile.path(), &edited).unwrap();

    let content = std::fs::read_to_string(tmpfile.path()).unwrap();
    let feedback = editor::parse_comment_result(
        &original,
        &content,
        "src/main.rs",
        "@@ -1,5 +1,5 @@",
        &hunk.lines,
    );

    assert!(feedback.is_some(), "Should capture positioned comments");
    let fb = feedback.unwrap();

    assert_eq!(
        fb.comment_positions.len(),
        2,
        "Should have 2 positioned comments"
    );

    // First comment after the +new_a line (hunk line index 3)
    assert_eq!(fb.comment_positions[0].0, 3, "First comment at pos 3");
    assert!(
        fb.comment_positions[0]
            .1
            .contains("First change looks good")
    );

    // Second comment after line3 (hunk line index 4)
    assert_eq!(fb.comment_positions[1].0, 4, "Second comment at pos 4");
    assert!(
        fb.comment_positions[1]
            .1
            .contains("But this context needs review")
    );

    // Verify format output has inline comments
    let output = stagent::feedback::format_feedback(&[fb], 2);
    assert!(
        output.contains("# REVIEW COMMENT: First change looks good"),
        "output: {}",
        output
    );
    assert!(
        output.contains("# REVIEW COMMENT: But this context needs review"),
        "output: {}",
        output
    );
}

/// Same test but for edit mode.
#[test]
fn test_flush_pending_edit_captures_feedback() {
    use stagent::app::App;

    let files = vec![FileDiff {
        path: "src/main.rs".into(),
        hunks: vec![Hunk {
            header: "@@ -1,3 +1,4 @@".to_string(),
            lines: vec![
                DiffLine {
                    kind: LineKind::Context,
                    content: "ctx\n".to_string(),
                    old_lineno: Some(1),
                    new_lineno: Some(1),
                },
                DiffLine {
                    kind: LineKind::Removed,
                    content: "old\n".to_string(),
                    old_lineno: Some(2),
                    new_lineno: None,
                },
                DiffLine {
                    kind: LineKind::Added,
                    content: "new\n".to_string(),
                    old_lineno: None,
                    new_lineno: Some(2),
                },
            ],
            status: HunkStatus::Pending,
            old_start: 1,
            old_lines: 3,
            new_start: 1,
            new_lines: 4,
        }],
        status: DeltaStatus::Modified,
        is_binary: false,
    }];

    let mut app = App::new(files, true);
    app.mode = AppMode::WaitingForEditor;

    // Create a tempfile simulating what prepare_edit_tempfile + user editing produces
    let tmpfile = editor::prepare_edit_tempfile(app.current_hunk().unwrap()).unwrap();
    let original_content = std::fs::read_to_string(tmpfile.path()).unwrap();
    // Edit: change "new" to "better"
    let edited = "ctx\nbetter\n";
    std::fs::write(tmpfile.path(), edited).unwrap();

    // Flush pending editor state (edit mode, not comment)
    app.flush_pending_editor_state(tmpfile.path(), false, &original_content);

    assert!(
        !app.feedback.is_empty(),
        "Edit feedback should have been captured"
    );
    assert_eq!(app.feedback[0].kind, FeedbackKind::Edit);
    assert!(
        app.feedback[0].content.contains("better"),
        "Edited content should be in feedback diff"
    );
}
