use std::io::Read;

use stagent::editor::{
    build_pane_exists_check_command, build_tmux_split_command, parse_comment_result,
    parse_edit_result, prepare_comment_tempfile, prepare_edit_tempfile,
};
use stagent::types::{DiffLine, FeedbackKind, Hunk, HunkStatus, LineKind};

/// Helper: build a Hunk with the given lines for testing.
fn make_hunk(header: &str, lines: Vec<(LineKind, &str)>) -> Hunk {
    Hunk {
        header: header.to_string(),
        lines: lines
            .into_iter()
            .map(|(kind, content)| DiffLine {
                kind,
                content: content.to_string(),
                old_lineno: None,
                new_lineno: None,
            })
            .collect(),
        status: HunkStatus::Pending,
        old_start: 1,
        old_lines: 3,
        new_start: 1,
        new_lines: 4,
    }
}

// ---------------------------------------------------------------------------
// build_tmux_split_command
// ---------------------------------------------------------------------------

#[test]
fn test_build_tmux_split_command() {
    let cmd = build_tmux_split_command("vim", "/tmp/test.rs");
    assert_eq!(cmd[0], "tmux");
    assert_eq!(cmd[1], "split-window");
    assert!(cmd.contains(&"-h".to_string()));
    assert!(cmd.contains(&"-P".to_string()));
    assert!(cmd.contains(&"#{pane_id}".to_string()));
    // The last argument should be the editor + filepath
    let last = cmd.last().unwrap();
    assert_eq!(last, "vim /tmp/test.rs");
}

#[test]
fn test_build_tmux_split_respects_editor_env() {
    let cmd = build_tmux_split_command("nano", "/tmp/file.txt");
    let last = cmd.last().unwrap();
    assert!(
        last.starts_with("nano"),
        "expected command to use nano, got: {}",
        last
    );
    assert_eq!(last, "nano /tmp/file.txt");
}

#[test]
fn test_build_tmux_split_falls_back_to_vi() {
    // build_tmux_split_command itself doesn't resolve the editor,
    // but we verify the typical fallback integration:
    // get_editor() returns "vi" when neither VISUAL nor EDITOR is set.
    let cmd = build_tmux_split_command("vi", "/tmp/file.txt");
    let last = cmd.last().unwrap();
    assert_eq!(last, "vi /tmp/file.txt");
}

#[test]
fn test_editor_env_precedence() {
    // get_editor() checks VISUAL first, then EDITOR, then falls back to vi.
    // We test indirectly by calling get_editor() with env vars set.
    use stagent::editor::get_editor;

    // Save originals
    let orig_visual = std::env::var("VISUAL").ok();
    let orig_editor = std::env::var("EDITOR").ok();

    // Set both — VISUAL should win
    std::env::set_var("VISUAL", "code");
    std::env::set_var("EDITOR", "nano");
    assert_eq!(get_editor(), "code");

    // Remove VISUAL — EDITOR should win
    std::env::remove_var("VISUAL");
    assert_eq!(get_editor(), "nano");

    // Remove both — should fall back to vi
    std::env::remove_var("EDITOR");
    assert_eq!(get_editor(), "vi");

    // Restore originals
    match orig_visual {
        Some(v) => std::env::set_var("VISUAL", v),
        None => std::env::remove_var("VISUAL"),
    }
    match orig_editor {
        Some(v) => std::env::set_var("EDITOR", v),
        None => std::env::remove_var("EDITOR"),
    }
}

// ---------------------------------------------------------------------------
// build_pane_exists_check_command
// ---------------------------------------------------------------------------

#[test]
fn test_pane_exists_check_command() {
    let cmd = build_pane_exists_check_command();
    assert_eq!(cmd[0], "tmux");
    assert_eq!(cmd[1], "list-panes");
    assert!(cmd.contains(&"-a".to_string()));
    assert!(cmd.contains(&"#{pane_id}".to_string()));
}

// ---------------------------------------------------------------------------
// prepare_edit_tempfile
// ---------------------------------------------------------------------------

#[test]
fn test_prepare_edit_tempfile() {
    let hunk = make_hunk(
        "@@ -1,3 +1,4 @@ fn main()",
        vec![
            (LineKind::Context, "fn main() {\n"),
            (LineKind::Removed, "    old_code();\n"),
            (LineKind::Added, "    new_code();\n"),
            (LineKind::Added, "    extra_code();\n"),
            (LineKind::Context, "}\n"),
        ],
    );

    let tmpfile = prepare_edit_tempfile(&hunk).expect("should create tempfile");
    let mut content = String::new();
    std::fs::File::open(tmpfile.path())
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    // Should contain context and added lines, NOT removed lines
    assert!(content.contains("fn main() {"), "missing context line");
    assert!(content.contains("new_code()"), "missing added line");
    assert!(
        content.contains("extra_code()"),
        "missing second added line"
    );
    assert!(
        !content.contains("old_code()"),
        "removed line should not appear"
    );
    assert!(content.contains("}"), "missing closing brace context");
}

#[test]
fn test_prepare_edit_tempfile_trailing_newlines() {
    // Lines without trailing newlines should still get one
    let hunk = make_hunk(
        "@@ -1,1 +1,1 @@",
        vec![(LineKind::Added, "no_newline_here")],
    );

    let tmpfile = prepare_edit_tempfile(&hunk).expect("should create tempfile");
    let mut content = String::new();
    std::fs::File::open(tmpfile.path())
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(
        content.ends_with('\n'),
        "content should have trailing newline"
    );
}

// ---------------------------------------------------------------------------
// prepare_comment_tempfile
// ---------------------------------------------------------------------------

#[test]
fn test_prepare_comment_tempfile() {
    let hunk = make_hunk(
        "@@ -10,3 +10,4 @@ fn review()",
        vec![
            (LineKind::Context, "fn review() {\n"),
            (LineKind::Removed, "    bad_code();\n"),
            (LineKind::Added, "    good_code();\n"),
            (LineKind::Context, "}\n"),
        ],
    );

    let tmpfile = prepare_comment_tempfile(&hunk).expect("should create tempfile");
    let mut content = String::new();
    std::fs::File::open(tmpfile.path())
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    // Should contain instruction markers
    assert!(
        content.contains("# Add your comments"),
        "missing comment instruction"
    );
    // Should contain the hunk header
    assert!(
        content.contains("@@ -10,3 +10,4 @@ fn review()"),
        "missing hunk header"
    );
    // Should contain all lines with proper prefixes
    assert!(
        content.contains(" fn review() {"),
        "missing context line with space prefix"
    );
    assert!(
        content.contains("-    bad_code();"),
        "missing removed line with - prefix"
    );
    assert!(
        content.contains("+    good_code();"),
        "missing added line with + prefix"
    );
    assert!(content.contains(" }"), "missing closing brace context");
}

// ---------------------------------------------------------------------------
// parse_edit_result
// ---------------------------------------------------------------------------

#[test]
fn test_parse_edited_result() {
    let original = "fn main() {\n    old_code();\n}\n";
    let edited = "fn main() {\n    new_code();\n}\n";

    let result = parse_edit_result(original, edited, "src/main.rs", "@@ -1,3 +1,3 @@", &[]);

    assert!(result.is_some(), "should produce feedback for a diff");
    let feedback = result.unwrap();
    assert_eq!(feedback.file_path, "src/main.rs");
    assert_eq!(feedback.hunk_header, "@@ -1,3 +1,3 @@");
    assert_eq!(feedback.kind, FeedbackKind::Edit);
    // The content should be a unified diff
    assert!(
        feedback.content.contains("-    old_code();"),
        "diff should show removed line"
    );
    assert!(
        feedback.content.contains("+    new_code();"),
        "diff should show added line"
    );
}

#[test]
fn test_parse_no_changes() {
    let content = "fn main() {\n    code();\n}\n";

    let result = parse_edit_result(content, content, "src/main.rs", "@@ -1,3 +1,3 @@", &[]);

    assert!(
        result.is_none(),
        "should return None when content is unchanged"
    );
}

// ---------------------------------------------------------------------------
// parse_comment_result
// ---------------------------------------------------------------------------

#[test]
fn test_parse_comments() {
    let original = "\
# Add your comments anywhere in this file.
# Any new lines you add will be captured as comments.
# @@ -10,3 +10,4 @@ fn review()

 fn review() {
-    bad_code();
+    good_code();
 }
";

    let edited = "\
# Add your comments anywhere in this file.
# Any new lines you add will be captured as comments.
# @@ -10,3 +10,4 @@ fn review()

 fn review() {
# COMMENT: This function needs better error handling
-    bad_code();
# COMMENT: Good replacement, but consider using a Result
+    good_code();
 }
";

    let result = parse_comment_result(original, edited, "src/review.rs", "@@ -10,3 +10,4 @@", &[]);

    assert!(result.is_some(), "should extract comments");
    let feedback = result.unwrap();
    assert_eq!(feedback.file_path, "src/review.rs");
    assert_eq!(feedback.hunk_header, "@@ -10,3 +10,4 @@");
    assert_eq!(feedback.kind, FeedbackKind::Comment);
    assert!(
        feedback
            .content
            .contains("This function needs better error handling"),
        "should contain first comment"
    );
    assert!(
        feedback
            .content
            .contains("Good replacement, but consider using a Result"),
        "should contain second comment"
    );
}

#[test]
fn test_parse_comments_no_comments() {
    let content = "\
# Add your comments anywhere in this file.
# Any new lines you add will be captured as comments.
# @@ -1,3 +1,3 @@

 fn main() {
     code();
 }
";

    // Same content as original — no changes means no comments
    let result = parse_comment_result(content, content, "src/main.rs", "@@ -1,3 +1,3 @@", &[]);

    assert!(
        result.is_none(),
        "should return None when no changes are made"
    );
}

#[test]
fn test_parse_comments_whitespace_handling() {
    let original = "some original content\n";
    let edited =
        "some original content\n# COMMENT:   spaces around   \n# COMMENT:no leading space\n";

    let result = parse_comment_result(original, edited, "test.rs", "@@", &[]);

    assert!(result.is_some());
    let feedback = result.unwrap();
    // trim() is applied to each comment line
    assert!(feedback.content.contains("spaces around"));
    assert!(feedback.content.contains("no leading space"));
}

// ---------------------------------------------------------------------------
// Integration tests (require tmux, marked #[ignore])
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_tmux_split_opens_and_closes() {
    use stagent::editor::{open_editor, wait_for_pane_close};

    // Use a temp file and an editor command that exits immediately
    let tmpfile = tempfile::NamedTempFile::new().expect("create tmp");
    let path = tmpfile.path().to_str().unwrap().to_string();

    // Override editor to something that exits immediately
    std::env::set_var("VISUAL", "true"); // `true` exits 0 immediately

    let pane_id = open_editor(&path).expect("should open tmux split");
    assert!(
        pane_id.starts_with('%'),
        "pane_id should start with %%, got: {}",
        pane_id
    );

    let rx = wait_for_pane_close(pane_id);
    // Should receive signal within a reasonable time
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .expect("pane should close within 10s");

    std::env::remove_var("VISUAL");
}

#[test]
#[ignore]
fn test_tmux_pane_id_captured() {
    use stagent::editor::open_editor;

    let tmpfile = tempfile::NamedTempFile::new().expect("create tmp");
    let path = tmpfile.path().to_str().unwrap().to_string();

    std::env::set_var("VISUAL", "true");

    let pane_id = open_editor(&path).expect("should open tmux split");
    assert!(!pane_id.is_empty(), "pane_id should not be empty");
    // tmux pane IDs look like %0, %1, %42, etc.
    assert!(
        pane_id.starts_with('%'),
        "pane_id should be a tmux pane id (%%N), got: {}",
        pane_id
    );

    std::env::remove_var("VISUAL");
}
