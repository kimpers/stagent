use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use crate::types::{FeedbackKind, HunkFeedback};

/// Default number of context lines to show around changes in comment feedback.
pub const DEFAULT_CONTEXT_LINES: usize = 5;

/// Format all feedback as a unified diff string.
/// `context_count` controls how many surrounding lines to show around
/// changed lines in comment feedback output.
pub fn format_feedback(feedbacks: &[HunkFeedback], context_count: usize) -> String {
    if feedbacks.is_empty() {
        return String::new();
    }

    // Group feedback by file path
    let mut by_file: BTreeMap<&str, Vec<&HunkFeedback>> = BTreeMap::new();
    for fb in feedbacks {
        by_file.entry(&fb.file_path).or_default().push(fb);
    }

    let mut output = String::new();

    for (file_path, file_feedbacks) in &by_file {
        // File header
        output.push_str(&format!("--- a/{}\n", file_path));
        output.push_str(&format!("+++ b/{}\n", file_path));

        for fb in file_feedbacks {
            match fb.kind {
                FeedbackKind::Edit => {
                    output.push_str(&format!("{}\n", fb.hunk_header));
                    output.push_str(&fb.content);
                    if !fb.content.ends_with('\n') {
                        output.push('\n');
                    }
                }
                FeedbackKind::Comment => {
                    output.push_str(&format!("{}\n", fb.hunk_header));
                    // Show up to 5 context lines before and after each
                    // changed line so the comment has surrounding diff context.
                    format_comment_with_context(&mut output, fb, context_count);
                }
            }
        }
    }

    output
}

/// Format a comment with surrounding diff context from the hunk.
///
/// Each comment is placed at its original position within the hunk,
/// with `context_count` diff lines shown before and after it:
///
/// ```text
///  context_line          (up to context_count before)
/// -removed line
/// +added line
/// # REVIEW COMMENT: comment here   (at the position the user placed it)
///  context_line          (up to context_count after)
/// ```
///
/// When multiple comments are far apart, a `...` separator is shown
/// between their context windows.
fn format_comment_with_context(output: &mut String, fb: &HunkFeedback, context_count: usize) {
    if fb.comment_positions.is_empty() {
        // Fallback: no position data, just emit comments
        for line in fb.content.lines() {
            output.push_str(&format!("# REVIEW COMMENT: {}\n", line));
        }
        return;
    }

    let n = fb.context_lines.len();

    // Build a list of (emit_start, emit_end, comments_at_position) ranges.
    // Each comment at position `pos` means the comment appears after
    // context_lines[pos-1]. We show context_count lines before and after.
    struct CommentRegion {
        // Range of hunk lines to show [start, end)
        start: usize,
        end: usize,
        // (position, text) — position is where in context_lines the comment goes
        comments: Vec<(usize, String)>,
    }

    let mut regions: Vec<CommentRegion> = Vec::new();

    for (pos, text) in &fb.comment_positions {
        let ctx_start = pos.saturating_sub(context_count);
        let ctx_end = (*pos + context_count).min(n);

        // Try to merge with the last region if overlapping
        if let Some(last) = regions.last_mut()
            && ctx_start <= last.end
        {
            last.end = last.end.max(ctx_end);
            last.comments.push((*pos, text.clone()));
            continue;
        }

        regions.push(CommentRegion {
            start: ctx_start,
            end: ctx_end,
            comments: vec![(*pos, text.clone())],
        });
    }

    for (ri, region) in regions.iter().enumerate() {
        if ri > 0 {
            output.push_str("  ...\n");
        }

        // Emit hunk lines in [start, end), inserting comments at their positions
        let mut comment_idx = 0;
        for i in region.start..region.end {
            // Check if any comments go before this line (at position i)
            while comment_idx < region.comments.len() && region.comments[comment_idx].0 == i {
                output.push_str(&format!(
                    "# REVIEW COMMENT: {}\n",
                    region.comments[comment_idx].1
                ));
                comment_idx += 1;
            }
            let line = &fb.context_lines[i];
            let prefix = line.kind.prefix();
            let content = line.content.trim_end_matches('\n');
            output.push_str(&format!("{}{}\n", prefix, content));
        }

        // Emit any remaining comments that go after the last line
        while comment_idx < region.comments.len() {
            output.push_str(&format!(
                "# REVIEW COMMENT: {}\n",
                region.comments[comment_idx].1
            ));
            comment_idx += 1;
        }
    }
}

/// Write feedback to a file or stdout.
pub fn write_feedback(output: &str, file_path: Option<&Path>) -> Result<()> {
    if output.is_empty() {
        return Ok(());
    }

    match file_path {
        Some(path) => {
            let mut file = std::fs::File::create(path)
                .with_context(|| format!("Failed to create output file: {}", path.display()))?;
            file.write_all(output.as_bytes())
                .context("Failed to write feedback to file")?;
        }
        None => {
            use std::io::Write as _;
            let _ = std::io::stdout().write_all(output.as_bytes());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LineKind;

    #[test]
    fn test_empty_feedback() {
        let result = format_feedback(&[], DEFAULT_CONTEXT_LINES);
        assert_eq!(result, "");
    }

    #[test]
    fn test_single_edit_feedback() {
        let feedback = vec![HunkFeedback {
            file_path: "src/main.rs".to_string(),
            hunk_header: "@@ -1,3 +1,4 @@".to_string(),
            kind: FeedbackKind::Edit,
            context_lines: vec![],
            comment_positions: vec![],
            content: "-old line\n+new line\n".to_string(),
        }];
        let result = format_feedback(&feedback, DEFAULT_CONTEXT_LINES);
        assert!(result.contains("--- a/src/main.rs"));
        assert!(result.contains("+++ b/src/main.rs"));
        assert!(result.contains("@@ -1,3 +1,4 @@"));
        assert!(result.contains("-old line"));
        assert!(result.contains("+new line"));
    }

    #[test]
    fn test_multiple_edits_same_file() {
        let feedback = vec![
            HunkFeedback {
                file_path: "src/main.rs".to_string(),
                hunk_header: "@@ -1,3 +1,4 @@".to_string(),
                kind: FeedbackKind::Edit,
                context_lines: vec![],
                comment_positions: vec![],
                content: "-old\n+new\n".to_string(),
            },
            HunkFeedback {
                file_path: "src/main.rs".to_string(),
                hunk_header: "@@ -10,3 +11,4 @@".to_string(),
                kind: FeedbackKind::Edit,
                context_lines: vec![],
                comment_positions: vec![],
                content: "-another old\n+another new\n".to_string(),
            },
        ];
        let result = format_feedback(&feedback, DEFAULT_CONTEXT_LINES);
        // Should have only one file header pair
        assert_eq!(result.matches("--- a/src/main.rs").count(), 1);
        assert_eq!(result.matches("+++ b/src/main.rs").count(), 1);
        // But two hunk headers
        assert!(result.contains("@@ -1,3 +1,4 @@"));
        assert!(result.contains("@@ -10,3 +11,4 @@"));
    }

    #[test]
    fn test_edits_across_files() {
        let feedback = vec![
            HunkFeedback {
                file_path: "src/a.rs".to_string(),
                hunk_header: "@@ -1,3 +1,4 @@".to_string(),
                kind: FeedbackKind::Edit,
                context_lines: vec![],
                comment_positions: vec![],
                content: "-old\n+new\n".to_string(),
            },
            HunkFeedback {
                file_path: "src/b.rs".to_string(),
                hunk_header: "@@ -5,3 +5,4 @@".to_string(),
                kind: FeedbackKind::Edit,
                context_lines: vec![],
                comment_positions: vec![],
                content: "-foo\n+bar\n".to_string(),
            },
        ];
        let result = format_feedback(&feedback, DEFAULT_CONTEXT_LINES);
        assert!(result.contains("--- a/src/a.rs"));
        assert!(result.contains("--- a/src/b.rs"));
    }

    #[test]
    fn test_comment_feedback_format() {
        use crate::types::DiffLine;

        let feedback = vec![HunkFeedback {
            file_path: "src/main.rs".to_string(),
            hunk_header: "@@ -1,3 +1,3 @@".to_string(),
            kind: FeedbackKind::Comment,
            content: "This function needs better error handling".to_string(),
            context_lines: vec![
                DiffLine {
                    kind: LineKind::Context,
                    content: "fn main() {\n".into(),
                    old_lineno: Some(1),
                    new_lineno: Some(1),
                },
                DiffLine {
                    kind: LineKind::Removed,
                    content: "    old_code();\n".into(),
                    old_lineno: Some(2),
                    new_lineno: None,
                },
                DiffLine {
                    kind: LineKind::Added,
                    content: "    new_code();\n".into(),
                    old_lineno: None,
                    new_lineno: Some(2),
                },
                DiffLine {
                    kind: LineKind::Context,
                    content: "}\n".into(),
                    old_lineno: Some(3),
                    new_lineno: Some(3),
                },
            ],
            // Comment placed after the added line (index 3 = after context_lines[2])
            comment_positions: vec![(3, "This function needs better error handling".to_string())],
        }];
        let result = format_feedback(&feedback, DEFAULT_CONTEXT_LINES);
        assert!(result.contains("# REVIEW COMMENT: This function needs better error handling"));
        // Should contain context lines from the hunk
        assert!(
            result.contains(" fn main() {"),
            "should have context before: {}",
            result
        );
        assert!(
            result.contains("-    old_code();"),
            "should have removed line: {}",
            result
        );
        assert!(
            result.contains("+    new_code();"),
            "should have added line: {}",
            result
        );
        assert!(
            result.contains(" }"),
            "should have context after: {}",
            result
        );
        // Comment should be placed between changes and trailing context
        let comment_pos = result.find("# REVIEW COMMENT:").unwrap();
        let added_pos = result.find("+    new_code();").unwrap();
        let closing_brace_pos = result.rfind(" }").unwrap();
        assert!(comment_pos > added_pos, "comment should come after changes");
        assert!(
            comment_pos < closing_brace_pos,
            "comment should come before trailing context"
        );
    }

    #[test]
    fn test_mixed_edits_and_comments() {
        use crate::types::DiffLine;
        let feedback = vec![
            HunkFeedback {
                file_path: "src/main.rs".to_string(),
                hunk_header: "@@ -1,3 +1,4 @@".to_string(),
                kind: FeedbackKind::Edit,
                context_lines: vec![],
                comment_positions: vec![],
                content: "-old\n+new\n".to_string(),
            },
            HunkFeedback {
                file_path: "src/main.rs".to_string(),
                hunk_header: "@@ -10,3 +11,3 @@".to_string(),
                kind: FeedbackKind::Comment,
                context_lines: vec![
                    DiffLine {
                        kind: LineKind::Removed,
                        content: "old\n".into(),
                        old_lineno: Some(10),
                        new_lineno: None,
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        content: "new\n".into(),
                        old_lineno: None,
                        new_lineno: Some(10),
                    },
                ],
                comment_positions: vec![(2, "Consider refactoring this".to_string())],
                content: "Consider refactoring this".to_string(),
            },
        ];
        let result = format_feedback(&feedback, DEFAULT_CONTEXT_LINES);
        assert!(result.contains("-old"));
        assert!(result.contains("+new"));
        assert!(result.contains("# REVIEW COMMENT: Consider refactoring this"));
    }

    #[test]
    fn test_multiple_comments_at_different_positions() {
        use crate::types::DiffLine;

        // Hunk with two separate changes and context between them
        let feedback = vec![HunkFeedback {
            file_path: "src/main.rs".to_string(),
            hunk_header: "@@ -1,7 +1,7 @@".to_string(),
            kind: FeedbackKind::Comment,
            content: "First comment\nSecond comment".to_string(),
            context_lines: vec![
                DiffLine {
                    kind: LineKind::Context,
                    content: "line1\n".into(),
                    old_lineno: Some(1),
                    new_lineno: Some(1),
                },
                DiffLine {
                    kind: LineKind::Removed,
                    content: "old_a\n".into(),
                    old_lineno: Some(2),
                    new_lineno: None,
                },
                DiffLine {
                    kind: LineKind::Added,
                    content: "new_a\n".into(),
                    old_lineno: None,
                    new_lineno: Some(2),
                },
                DiffLine {
                    kind: LineKind::Context,
                    content: "line3\n".into(),
                    old_lineno: Some(3),
                    new_lineno: Some(3),
                },
                DiffLine {
                    kind: LineKind::Context,
                    content: "line4\n".into(),
                    old_lineno: Some(4),
                    new_lineno: Some(4),
                },
                DiffLine {
                    kind: LineKind::Context,
                    content: "line5\n".into(),
                    old_lineno: Some(5),
                    new_lineno: Some(5),
                },
                DiffLine {
                    kind: LineKind::Removed,
                    content: "old_b\n".into(),
                    old_lineno: Some(6),
                    new_lineno: None,
                },
                DiffLine {
                    kind: LineKind::Added,
                    content: "new_b\n".into(),
                    old_lineno: None,
                    new_lineno: Some(6),
                },
                DiffLine {
                    kind: LineKind::Context,
                    content: "line7\n".into(),
                    old_lineno: Some(7),
                    new_lineno: Some(7),
                },
            ],
            // Comment after first change (pos 3) and after second change (pos 8)
            comment_positions: vec![
                (3, "First comment".to_string()),
                (8, "Second comment".to_string()),
            ],
        }];

        let result = format_feedback(&feedback, 2);

        // Both comments should appear
        assert!(
            result.contains("# REVIEW COMMENT: First comment"),
            "result: {}",
            result
        );
        assert!(
            result.contains("# REVIEW COMMENT: Second comment"),
            "result: {}",
            result
        );

        // First comment should appear after new_a and before line3
        let first_comment_pos = result.find("# REVIEW COMMENT: First comment").unwrap();
        let new_a_pos = result.find("+new_a").unwrap();
        assert!(first_comment_pos > new_a_pos, "first comment after +new_a");

        // Second comment should appear after new_b
        let second_comment_pos = result.find("# REVIEW COMMENT: Second comment").unwrap();
        let new_b_pos = result.find("+new_b").unwrap();
        assert!(
            second_comment_pos > new_b_pos,
            "second comment after +new_b"
        );

        // With context_count=2, there should be a gap indicator between the
        // two comment regions since they're far apart
    }

    #[test]
    fn test_feedback_is_valid_patch() {
        let feedback = vec![HunkFeedback {
            file_path: "src/main.rs".to_string(),
            hunk_header: "@@ -1,3 +1,4 @@".to_string(),
            kind: FeedbackKind::Edit,
            context_lines: vec![],
            comment_positions: vec![],
            content: " context\n-old line\n+new line\n context2\n".to_string(),
        }];
        let result = format_feedback(&feedback, DEFAULT_CONTEXT_LINES);
        // Should start with file headers and contain valid unified diff structure
        assert!(result.starts_with("--- a/"));
        assert!(result.contains("+++ b/"));
        assert!(result.contains("@@"));
    }

    #[test]
    fn test_feedback_output_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("output.diff");
        let content = "--- a/test.rs\n+++ b/test.rs\n@@ -1 +1 @@\n-old\n+new\n";
        write_feedback(content, Some(&file_path)).unwrap();

        let written = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(written, content);
    }

    #[test]
    fn test_feedback_output_to_stdout() {
        // Just verify it doesn't panic
        write_feedback("test output", None).unwrap();
    }
}
