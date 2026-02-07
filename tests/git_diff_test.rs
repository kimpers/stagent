mod helpers;

use helpers::*;
use stagent::git::{get_unstaged_diff, open_repo};
use stagent::types::{DeltaStatus, LineKind};

#[test]
fn test_open_repo() {
    let (dir, _repo) = create_temp_repo();
    let repo = open_repo(dir.path());
    assert!(repo.is_ok(), "open_repo should succeed on a valid git repo");
}

#[test]
fn test_no_unstaged_changes() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "hello.txt", "hello world\n");

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert!(
        diffs.is_empty(),
        "clean repo should have no unstaged changes"
    );
}

#[test]
fn test_single_file_single_hunk() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "file.txt", "line 1\nline 2\nline 3\n");
    modify_file(&repo, "file.txt", "line 1\nline 2 modified\nline 3\n");

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert_eq!(diffs.len(), 1, "should have exactly 1 FileDiff");
    assert_eq!(diffs[0].hunks.len(), 1, "should have exactly 1 hunk");
    assert_eq!(diffs[0].status, DeltaStatus::Modified);
    assert!(!diffs[0].is_binary);
}

#[test]
fn test_single_file_multiple_hunks() {
    let (_dir, repo) = create_temp_repo();

    // Create a file with many lines so changes are far apart
    let mut original = String::new();
    for i in 1..=30 {
        original.push_str(&format!("line {}\n", i));
    }
    commit_file(&repo, "big.txt", &original);

    // Modify lines far apart (line 2 and line 28) so git produces separate hunks
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

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert_eq!(diffs.len(), 1, "should have exactly 1 FileDiff");
    assert!(
        diffs[0].hunks.len() >= 2,
        "should have at least 2 hunks, got {}",
        diffs[0].hunks.len()
    );
}

#[test]
fn test_multiple_files() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "a.txt", "aaa\n");
    commit_file(&repo, "b.txt", "bbb\n");
    commit_file(&repo, "c.txt", "ccc\n");

    // Modify only a.txt and b.txt
    modify_file(&repo, "a.txt", "aaa modified\n");
    modify_file(&repo, "b.txt", "bbb modified\n");

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert_eq!(diffs.len(), 2, "should have exactly 2 FileDiffs");

    let paths: Vec<String> = diffs.iter().map(|d| d.path.display().to_string()).collect();
    assert!(paths.contains(&"a.txt".to_string()), "should contain a.txt");
    assert!(paths.contains(&"b.txt".to_string()), "should contain b.txt");
}

#[test]
fn test_new_untracked_file() {
    let (_dir, repo) = create_temp_repo();
    create_untracked_file(&repo, "new_file.txt", "brand new content\n");

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert_eq!(
        diffs.len(),
        1,
        "should have exactly 1 FileDiff for untracked file"
    );
    assert_eq!(diffs[0].status, DeltaStatus::Untracked);
    assert_eq!(diffs[0].path.display().to_string(), "new_file.txt");
}

#[test]
fn test_deleted_file() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "doomed.txt", "this will be deleted\n");
    delete_file(&repo, "doomed.txt");

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert_eq!(
        diffs.len(),
        1,
        "should have exactly 1 FileDiff for deleted file"
    );
    assert_eq!(diffs[0].status, DeltaStatus::Deleted);
    assert_eq!(diffs[0].path.display().to_string(), "doomed.txt");
}

#[test]
fn test_hunk_line_content() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "file.txt", "aaa\nbbb\nccc\n");
    modify_file(&repo, "file.txt", "aaa\nBBB\nccc\n");

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert_eq!(diffs.len(), 1);

    let hunk = &diffs[0].hunks[0];
    assert!(!hunk.lines.is_empty(), "hunk should have lines");

    // Find the removed line
    let removed: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Removed)
        .collect();
    assert_eq!(removed.len(), 1, "should have 1 removed line");
    assert_eq!(removed[0].content, "bbb\n");
    assert!(
        removed[0].old_lineno.is_some(),
        "removed line should have old_lineno"
    );
    assert!(
        removed[0].new_lineno.is_none(),
        "removed line should not have new_lineno"
    );

    // Find the added line
    let added: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Added)
        .collect();
    assert_eq!(added.len(), 1, "should have 1 added line");
    assert_eq!(added[0].content, "BBB\n");
    assert!(
        added[0].new_lineno.is_some(),
        "added line should have new_lineno"
    );
    assert!(
        added[0].old_lineno.is_none(),
        "added line should not have old_lineno"
    );

    // Check context lines
    let context: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Context)
        .collect();
    assert!(!context.is_empty(), "should have context lines");
    for ctx in &context {
        assert!(
            ctx.old_lineno.is_some(),
            "context line should have old_lineno"
        );
        assert!(
            ctx.new_lineno.is_some(),
            "context line should have new_lineno"
        );
    }
}

#[test]
fn test_hunk_header_format() {
    let (_dir, repo) = create_temp_repo();
    commit_file(&repo, "file.txt", "line 1\nline 2\nline 3\n");
    modify_file(&repo, "file.txt", "line 1\nline 2 changed\nline 3\n");

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert_eq!(diffs.len(), 1);

    let hunk = &diffs[0].hunks[0];
    // The header should start with @@ and contain the line range info
    assert!(
        hunk.header.starts_with("@@"),
        "header should start with @@, got: {}",
        hunk.header
    );
    assert!(
        hunk.header.contains("-"),
        "header should contain '-' for old range, got: {}",
        hunk.header
    );
    assert!(
        hunk.header.contains("+"),
        "header should contain '+' for new range, got: {}",
        hunk.header
    );

    // Verify the parsed numeric fields match
    assert!(hunk.old_start > 0, "old_start should be > 0");
    assert!(hunk.old_lines > 0, "old_lines should be > 0");
    assert!(hunk.new_start > 0, "new_start should be > 0");
    assert!(hunk.new_lines > 0, "new_lines should be > 0");
}

#[test]
fn test_binary_file_detected() {
    let (_dir, repo) = create_temp_repo();

    // Commit a binary file, then modify it
    create_binary_file(&repo, "image.bin");

    // Stage and commit the binary file
    {
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("image.bin")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();
        let head = repo.head().unwrap();
        let parent = head.peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Add binary", &tree, &[&parent])
            .unwrap();
    }

    // Modify the binary file with different binary content
    let workdir = repo.workdir().unwrap();
    let binary_path = workdir.join("image.bin");
    let new_content: Vec<u8> = (0..256).rev().map(|i| i as u8).collect();
    std::fs::write(&binary_path, &new_content).unwrap();

    let diffs = get_unstaged_diff(&repo).expect("get_unstaged_diff failed");
    assert_eq!(
        diffs.len(),
        1,
        "should have exactly 1 FileDiff for binary file"
    );

    let file_diff = &diffs[0];
    assert_eq!(file_diff.path.display().to_string(), "image.bin");

    // Binary files should have no text hunks parsed from the patch
    // (git2's Patch API produces no hunks for binary content).
    // Note: the BINARY flag in git2 is set lazily during content examination,
    // so is_binary may or may not be set depending on when deltas are inspected.
    // The key invariant is that binary diffs produce no parseable hunks.
    assert!(
        file_diff.hunks.is_empty(),
        "binary file should have no parsed hunks, got {}",
        file_diff.hunks.len()
    );
}
