use anyhow::{Result, bail};

use crate::types::{DeltaStatus, DiffLine, FileDiff, Hunk, HunkStatus, LineKind};

/// Parse a unified diff (as produced by `git diff`) into our structured `FileDiff` types.
///
/// This is the stdin-based counterpart to `diff::parse_diff()` which uses git2.
pub fn parse_unified_diff(input: &str) -> Result<Vec<FileDiff>> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut files: Vec<FileDiff> = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Look for "diff --git a/... b/..."
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let (file_diff, next_i) = parse_file_diff(rest, &lines, i)?;
            files.push(file_diff);
            i = next_i;
        } else {
            i += 1;
        }
    }

    Ok(files)
}

/// Parse a single file's diff starting from the "diff --git" line.
/// Returns the FileDiff and the index of the next line to process.
fn parse_file_diff(
    git_header_rest: &str,
    lines: &[&str],
    start: usize,
) -> Result<(FileDiff, usize)> {
    // Extract path from "a/path b/path"
    let path = parse_git_header_path(git_header_rest);

    let mut i = start + 1; // skip "diff --git" line
    let mut status = DeltaStatus::Modified;
    let mut is_binary = false;
    let mut actual_path = path.clone();

    // Parse extended headers
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("diff --git ") || line.starts_with("@@ ") {
            break;
        }

        if line.starts_with("new file mode") {
            status = DeltaStatus::Added;
        } else if line.starts_with("deleted file mode") {
            status = DeltaStatus::Deleted;
        } else if let Some(rest) = line.strip_prefix("rename to ") {
            status = DeltaStatus::Renamed;
            actual_path = rest.to_string();
        } else if line.starts_with("Binary files ") && line.ends_with(" differ") {
            is_binary = true;
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            // "+++ b/path" — use this as the definitive path
            let p = strip_ab_prefix(rest);
            if p != "/dev/null" {
                actual_path = p.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("--- ") {
            // For deleted files, "--- a/path" is the only real path
            let p = strip_ab_prefix(rest);
            if rest != "/dev/null" && status == DeltaStatus::Deleted {
                actual_path = p.to_string();
            }
        }

        i += 1;
    }

    let mut hunks = Vec::new();

    // Parse hunks
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("diff --git ") {
            break;
        }

        if line.starts_with("@@ ") {
            let (hunk, next_i) = parse_hunk(lines, i)?;
            hunks.push(hunk);
            i = next_i;
        } else {
            i += 1;
        }
    }

    Ok((
        FileDiff {
            path: actual_path.into(),
            hunks,
            status,
            is_binary,
        },
        i,
    ))
}

/// Parse the path from the git diff header "a/path b/path".
/// Handles paths with spaces by splitting on " b/".
fn parse_git_header_path(header: &str) -> String {
    // Format: "a/path b/path" — find the " b/" separator
    if let Some(pos) = header.find(" b/") {
        header[pos + 3..].to_string()
    } else {
        // Fallback: try splitting on space
        header
            .split_whitespace()
            .last()
            .map(|s| strip_ab_prefix(s).to_string())
            .unwrap_or_else(|| header.to_string())
    }
}

/// Strip "a/" or "b/" prefix from a path.
fn strip_ab_prefix(path: &str) -> &str {
    if let Some(rest) = path.strip_prefix("a/") {
        rest
    } else if let Some(rest) = path.strip_prefix("b/") {
        rest
    } else {
        path
    }
}

/// Parse a single hunk starting from the "@@ ... @@" line.
/// Returns the Hunk and the index of the next line to process.
fn parse_hunk(lines: &[&str], start: usize) -> Result<(Hunk, usize)> {
    let header_line = lines[start];
    let (old_start, old_lines, new_start, new_lines, header) = parse_hunk_header(header_line)?;

    let mut diff_lines = Vec::new();
    let mut old_lineno = old_start;
    let mut new_lineno = new_start;
    let mut i = start + 1;

    while i < lines.len() {
        let line = lines[i];

        // Stop at next hunk header, next file, or end
        if line.starts_with("@@ ") || line.starts_with("diff --git ") {
            break;
        }

        // "\ No newline at end of file" — skip
        if line.starts_with("\\ ") {
            i += 1;
            continue;
        }

        if let Some(content) = line.strip_prefix('+') {
            diff_lines.push(DiffLine {
                kind: LineKind::Added,
                content: format!("{content}\n"),
                old_lineno: None,
                new_lineno: Some(new_lineno),
            });
            new_lineno += 1;
        } else if let Some(content) = line.strip_prefix('-') {
            diff_lines.push(DiffLine {
                kind: LineKind::Removed,
                content: format!("{content}\n"),
                old_lineno: Some(old_lineno),
                new_lineno: None,
            });
            old_lineno += 1;
        } else if let Some(content) = line.strip_prefix(' ') {
            diff_lines.push(DiffLine {
                kind: LineKind::Context,
                content: format!("{content}\n"),
                old_lineno: Some(old_lineno),
                new_lineno: Some(new_lineno),
            });
            old_lineno += 1;
            new_lineno += 1;
        } else if line.is_empty() {
            // Empty context line (some diffs omit the leading space for blank lines)
            diff_lines.push(DiffLine {
                kind: LineKind::Context,
                content: "\n".to_string(),
                old_lineno: Some(old_lineno),
                new_lineno: Some(new_lineno),
            });
            old_lineno += 1;
            new_lineno += 1;
        } else {
            // Unknown line — stop parsing this hunk.
            // This shouldn't happen with well-formed git diff output but can
            // occur with manually edited or truncated diffs.
            eprintln!(
                "Warning: unexpected line in hunk at line {}: {:?}",
                i + 1,
                line.chars().take(60).collect::<String>()
            );
            break;
        }

        i += 1;
    }

    // Validate parsed line counts against header.
    let actual_old = diff_lines
        .iter()
        .filter(|l| matches!(l.kind, LineKind::Removed | LineKind::Context))
        .count() as u32;
    let actual_new = diff_lines
        .iter()
        .filter(|l| matches!(l.kind, LineKind::Added | LineKind::Context))
        .count() as u32;
    if actual_old != old_lines || actual_new != new_lines {
        eprintln!(
            "Warning: hunk line count mismatch in {}: header says -{},{} +{},{} but parsed -{},{}",
            header, old_start, old_lines, new_start, new_lines, actual_old, actual_new
        );
    }

    Ok((
        Hunk {
            header,
            lines: diff_lines,
            status: HunkStatus::Pending,
            old_start,
            old_lines,
            new_start,
            new_lines,
        },
        i,
    ))
}

/// Parse a hunk header like "@@ -10,5 +10,7 @@ fn foo()".
/// Returns (old_start, old_lines, new_start, new_lines, full_header_string).
fn parse_hunk_header(line: &str) -> Result<(u32, u32, u32, u32, String)> {
    let header = line.trim_end().to_string();

    // Extract the range part between @@ markers
    let after_at = line
        .strip_prefix("@@ ")
        .ok_or_else(|| anyhow::anyhow!("Invalid hunk header: {}", line))?;

    let end_at = after_at
        .find(" @@")
        .ok_or_else(|| anyhow::anyhow!("Invalid hunk header: {}", line))?;

    let range_part = &after_at[..end_at];

    // Split into old and new ranges: "-10,5 +10,7"
    let parts: Vec<&str> = range_part.split_whitespace().collect();
    if parts.len() != 2 {
        bail!("Invalid hunk header range: {}", range_part);
    }

    let (old_start, old_lines) = parse_range(parts[0].strip_prefix('-').unwrap_or(parts[0]))?;
    let (new_start, new_lines) = parse_range(parts[1].strip_prefix('+').unwrap_or(parts[1]))?;

    Ok((old_start, old_lines, new_start, new_lines, header))
}

/// Parse a range like "10,5" or "10" (omitted count = 1).
fn parse_range(range: &str) -> Result<(u32, u32)> {
    if let Some((start_s, count_s)) = range.split_once(',') {
        let start: u32 = start_s.parse()?;
        let count: u32 = count_s.parse()?;
        Ok((start, count))
    } else {
        let start: u32 = range.parse()?;
        Ok((start, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let result = parse_unified_diff("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_whitespace_only_input() {
        let result = parse_unified_diff("   \n  \n").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_file_single_hunk() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc1234..def5678 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"hello\");
+    println!(\"hello world\");
+    println!(\"goodbye\");
 }
";
        let files = parse_unified_diff(diff).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.to_string_lossy(), "src/main.rs");
        assert_eq!(files[0].status, DeltaStatus::Modified);
        assert!(!files[0].is_binary);
        assert_eq!(files[0].hunks.len(), 1);

        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_lines, 3);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_lines, 4);
        assert_eq!(hunk.status, HunkStatus::Pending);

        // 4 lines: context, removed, added, added, context
        assert_eq!(hunk.lines.len(), 5);
        assert_eq!(hunk.lines[0].kind, LineKind::Context);
        assert_eq!(hunk.lines[1].kind, LineKind::Removed);
        assert_eq!(hunk.lines[2].kind, LineKind::Added);
        assert_eq!(hunk.lines[3].kind, LineKind::Added);
        assert_eq!(hunk.lines[4].kind, LineKind::Context);
    }

    #[test]
    fn test_line_numbers_tracked() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -10,4 +10,5 @@
 line10
-line11old
+line11new
+line11b
 line12
";
        let files = parse_unified_diff(diff).unwrap();
        let hunk = &files[0].hunks[0];

        // Context line 10
        assert_eq!(hunk.lines[0].old_lineno, Some(10));
        assert_eq!(hunk.lines[0].new_lineno, Some(10));
        // Removed line 11
        assert_eq!(hunk.lines[1].old_lineno, Some(11));
        assert_eq!(hunk.lines[1].new_lineno, None);
        // Added line 11
        assert_eq!(hunk.lines[2].old_lineno, None);
        assert_eq!(hunk.lines[2].new_lineno, Some(11));
        // Added line 11b
        assert_eq!(hunk.lines[3].old_lineno, None);
        assert_eq!(hunk.lines[3].new_lineno, Some(12));
        // Context line 12
        assert_eq!(hunk.lines[4].old_lineno, Some(12));
        assert_eq!(hunk.lines[4].new_lineno, Some(13));
    }

    #[test]
    fn test_multi_file() {
        let diff = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,2 +1,2 @@
-old_a
+new_a
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
@@ -1,2 +1,2 @@
-old_b
+new_b
";
        let files = parse_unified_diff(diff).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path.to_string_lossy(), "a.rs");
        assert_eq!(files[1].path.to_string_lossy(), "b.rs");
    }

    #[test]
    fn test_multi_hunk() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
-old1
+new1
 ctx
@@ -20,3 +20,3 @@
-old2
+new2
 ctx
";
        let files = parse_unified_diff(diff).unwrap();
        assert_eq!(files[0].hunks.len(), 2);
        assert_eq!(files[0].hunks[0].old_start, 1);
        assert_eq!(files[0].hunks[1].old_start, 20);
    }

    #[test]
    fn test_new_file() {
        let diff = "\
diff --git a/new.rs b/new.rs
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,3 @@
+fn new() {
+    // new file
+}
";
        let files = parse_unified_diff(diff).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.to_string_lossy(), "new.rs");
        assert_eq!(files[0].status, DeltaStatus::Added);
        assert_eq!(files[0].hunks[0].lines.len(), 3);
    }

    #[test]
    fn test_deleted_file() {
        let diff = "\
diff --git a/old.rs b/old.rs
deleted file mode 100644
index abc1234..0000000
--- a/old.rs
+++ /dev/null
@@ -1,3 +0,0 @@
-fn old() {
-    // deleted
-}
";
        let files = parse_unified_diff(diff).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.to_string_lossy(), "old.rs");
        assert_eq!(files[0].status, DeltaStatus::Deleted);
    }

    #[test]
    fn test_renamed_file() {
        let diff = "\
diff --git a/old_name.rs b/new_name.rs
similarity index 95%
rename from old_name.rs
rename to new_name.rs
--- a/old_name.rs
+++ b/new_name.rs
@@ -1,3 +1,3 @@
-old
+new
";
        let files = parse_unified_diff(diff).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.to_string_lossy(), "new_name.rs");
        assert_eq!(files[0].status, DeltaStatus::Renamed);
    }

    #[test]
    fn test_binary_file() {
        let diff = "\
diff --git a/image.png b/image.png
Binary files a/image.png and b/image.png differ
";
        let files = parse_unified_diff(diff).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].is_binary);
        assert!(files[0].hunks.is_empty());
    }

    #[test]
    fn test_no_newline_at_eof() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1,2 +1,2 @@
-old
+new
\\ No newline at end of file
";
        let files = parse_unified_diff(diff).unwrap();
        assert_eq!(files[0].hunks[0].lines.len(), 2);
    }

    #[test]
    fn test_hunk_header_with_function_context() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -10,3 +10,4 @@ fn some_function()
 context
+added
 context
";
        let files = parse_unified_diff(diff).unwrap();
        let header = &files[0].hunks[0].header;
        assert!(header.contains("fn some_function()"));
    }

    #[test]
    fn test_omitted_hunk_count() {
        // When count is 1, it can be omitted: @@ -1 +1 @@
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1 +1 @@
-old
+new
";
        let files = parse_unified_diff(diff).unwrap();
        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_lines, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_lines, 1);
    }

    #[test]
    fn test_parse_range() {
        assert_eq!(parse_range("10,5").unwrap(), (10, 5));
        assert_eq!(parse_range("1").unwrap(), (1, 1));
        assert_eq!(parse_range("0,0").unwrap(), (0, 0));
    }

    #[test]
    fn test_parse_range_invalid() {
        assert!(parse_range("abc").is_err());
        assert!(parse_range("1,abc").is_err());
        assert!(parse_range("abc,1").is_err());
        assert!(parse_range("").is_err());
        assert!(parse_range(",").is_err());
    }

    #[test]
    fn test_malformed_hunk_header() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -BAD +STUFF @@
 context
";
        let result = parse_unified_diff(diff);
        assert!(
            result.is_err(),
            "Malformed hunk header should produce an error"
        );
    }

    #[test]
    fn test_malformed_hunk_header_missing_closing_at() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1,2 +1,2
 context
";
        let result = parse_unified_diff(diff);
        assert!(
            result.is_err(),
            "Missing closing @@ should produce an error"
        );
    }

    #[test]
    fn test_strip_ab_prefix() {
        assert_eq!(strip_ab_prefix("a/foo.rs"), "foo.rs");
        assert_eq!(strip_ab_prefix("b/foo.rs"), "foo.rs");
        assert_eq!(strip_ab_prefix("/dev/null"), "/dev/null");
        assert_eq!(strip_ab_prefix("plain"), "plain");
    }

    #[test]
    fn test_content_has_newlines() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1,2 +1,2 @@
 context
-old
+new
";
        let files = parse_unified_diff(diff).unwrap();
        let hunk = &files[0].hunks[0];
        // All content lines should end with \n
        for line in &hunk.lines {
            assert!(
                line.content.ends_with('\n'),
                "Line content should end with newline: {:?}",
                line.content
            );
        }
    }
}
