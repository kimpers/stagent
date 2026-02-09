mod helpers;

use helpers::*;
use stagent::patch::parse_unified_diff;
use stagent::types::{DeltaStatus, LineKind};
use std::process::Command;

/// Run `git diff` in the given repo and return the output as a string.
fn git_diff_output(repo: &git2::Repository) -> String {
    let workdir = repo.workdir().expect("not a bare repo");
    let output = Command::new("git")
        .args(["diff"])
        .current_dir(workdir)
        .output()
        .expect("failed to run git diff");
    String::from_utf8(output.stdout).expect("git diff produced invalid UTF-8")
}

#[test]
fn test_patch_parse_single_file_modification() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "file.txt", "line 1\nline 2\nline 3\n");
    modify_file(&repo, "file.txt", "line 1\nline 2 modified\nline 3\n");

    let diff_text = git_diff_output(&repo);
    let files = parse_unified_diff(&diff_text).expect("parse failed");

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path.to_string_lossy(), "file.txt");
    assert_eq!(files[0].status, DeltaStatus::Modified);
    assert_eq!(files[0].hunks.len(), 1);

    let hunk = &files[0].hunks[0];
    let removed: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Removed)
        .collect();
    let added: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Added)
        .collect();
    assert_eq!(removed.len(), 1);
    assert_eq!(added.len(), 1);
    assert_eq!(removed[0].content, "line 2\n");
    assert_eq!(added[0].content, "line 2 modified\n");
}

#[test]
fn test_patch_parse_multiple_files() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "a.txt", "aaa\n");
    commit_file(&repo, "b.txt", "bbb\n");
    modify_file(&repo, "a.txt", "aaa modified\n");
    modify_file(&repo, "b.txt", "bbb modified\n");

    let diff_text = git_diff_output(&repo);
    let files = parse_unified_diff(&diff_text).expect("parse failed");

    assert_eq!(files.len(), 2);
    let paths: Vec<String> = files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    assert!(paths.contains(&"a.txt".to_string()));
    assert!(paths.contains(&"b.txt".to_string()));
}

#[test]
fn test_patch_parse_multiple_hunks() {
    let (_dir, repo) = create_temp_repo();

    let mut original = String::new();
    for i in 1..=30 {
        original.push_str(&format!("line {}\n", i));
    }
    commit_file(&repo, "big.txt", &original);

    let mut modified = String::new();
    for i in 1..=30 {
        if i == 2 {
            modified.push_str("line 2 CHANGED\n");
        } else if i == 28 {
            modified.push_str("line 28 CHANGED\n");
        } else {
            modified.push_str(&format!("line {}\n", i));
        }
    }
    modify_file(&repo, "big.txt", &modified);

    let diff_text = git_diff_output(&repo);
    let files = parse_unified_diff(&diff_text).expect("parse failed");

    assert_eq!(files.len(), 1);
    assert!(
        files[0].hunks.len() >= 2,
        "should have at least 2 hunks, got {}",
        files[0].hunks.len()
    );
}

#[test]
fn test_patch_parse_line_numbers_match_git2() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "file.txt", "aaa\nbbb\nccc\n");
    modify_file(&repo, "file.txt", "aaa\nBBB\nccc\n");

    // Parse with our unified diff parser
    let diff_text = git_diff_output(&repo);
    let patch_files = parse_unified_diff(&diff_text).expect("parse failed");

    // Parse with git2
    let git2_files = stagent::git::get_unstaged_diff(&repo).expect("git2 diff failed");

    assert_eq!(patch_files.len(), git2_files.len());
    assert_eq!(patch_files[0].hunks.len(), git2_files[0].hunks.len());

    let patch_hunk = &patch_files[0].hunks[0];
    let git2_hunk = &git2_files[0].hunks[0];

    // Hunk ranges should match
    assert_eq!(patch_hunk.old_start, git2_hunk.old_start);
    assert_eq!(patch_hunk.old_lines, git2_hunk.old_lines);
    assert_eq!(patch_hunk.new_start, git2_hunk.new_start);
    assert_eq!(patch_hunk.new_lines, git2_hunk.new_lines);

    // Line counts and kinds should match
    assert_eq!(patch_hunk.lines.len(), git2_hunk.lines.len());
    for (pl, gl) in patch_hunk.lines.iter().zip(git2_hunk.lines.iter()) {
        assert_eq!(pl.kind, gl.kind, "line kind mismatch");
        assert_eq!(pl.old_lineno, gl.old_lineno, "old_lineno mismatch");
        assert_eq!(pl.new_lineno, gl.new_lineno, "new_lineno mismatch");
        assert_eq!(pl.content, gl.content, "content mismatch");
    }
}

#[test]
fn test_patch_parse_deleted_file() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "doomed.txt", "this will be deleted\n");
    delete_file(&repo, "doomed.txt");

    let diff_text = git_diff_output(&repo);
    let files = parse_unified_diff(&diff_text).expect("parse failed");

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path.to_string_lossy(), "doomed.txt");
    assert_eq!(files[0].status, DeltaStatus::Deleted);
    assert_eq!(files[0].hunks.len(), 1);

    // All lines should be removed
    for line in &files[0].hunks[0].lines {
        assert_eq!(line.kind, LineKind::Removed);
    }
}

#[test]
fn test_patch_parse_added_lines_only() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "file.txt", "line 1\nline 2\n");
    modify_file(&repo, "file.txt", "line 1\nline 2\nline 3\nline 4\n");

    let diff_text = git_diff_output(&repo);
    let files = parse_unified_diff(&diff_text).expect("parse failed");

    assert_eq!(files.len(), 1);
    let added: Vec<_> = files[0].hunks[0]
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Added)
        .collect();
    assert_eq!(added.len(), 2);
    assert_eq!(added[0].content, "line 3\n");
    assert_eq!(added[1].content, "line 4\n");
}
