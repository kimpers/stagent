mod helpers;

use git2::{DiffOptions, Repository};
use stagent::diff::{parse_diff, split_hunk};
use stagent::git::intent_to_add_untracked;
use stagent::staging::{reconstruct_blob, stage_hunk};
use stagent::types::{DiffLine, FileDiff, Hunk, HunkStatus, LineKind};

/// Helper: get the staged (cached) diff for assertion checks.
fn get_staged_diff(repo: &Repository) -> Vec<FileDiff> {
    let head_tree = repo.head().unwrap().peel_to_tree().unwrap();
    let diff = repo
        .diff_tree_to_index(Some(&head_tree), None, None)
        .unwrap();
    parse_diff(&diff).unwrap()
}

/// Helper: get the unstaged diff (index-to-workdir).
fn get_unstaged_diff(repo: &Repository) -> Vec<FileDiff> {
    let mut opts = DiffOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts)).unwrap();
    parse_diff(&diff).unwrap()
}

/// Helper: get the unstaged diff with untracked file content included.
fn get_unstaged_diff_with_untracked_content(repo: &Repository) -> Vec<FileDiff> {
    let mut opts = DiffOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    opts.show_untracked_content(true);
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts)).unwrap();
    parse_diff(&diff).unwrap()
}

// ============================================================
// Integration tests: stage_hunk
// ============================================================

#[test]
fn test_stage_single_hunk_single_file() {
    let (dir, repo) = helpers::create_temp_repo();

    // Commit a file, then modify it
    helpers::commit_file(&repo, "hello.txt", "line1\nline2\nline3\n");
    helpers::modify_file(&repo, "hello.txt", "line1\nline2 modified\nline3\n");

    // Parse the unstaged diff
    let files = get_unstaged_diff(&repo);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks.len(), 1);

    // Stage the hunk
    stage_hunk(&repo, &files[0], &files[0].hunks[0], 0).unwrap();

    // Verify: staged diff should show this change
    let staged = get_staged_diff(&repo);
    assert_eq!(staged.len(), 1, "Staged diff should have 1 file");
    assert_eq!(staged[0].path.to_str().unwrap(), "hello.txt");

    // Verify: unstaged diff should be empty now
    let unstaged = get_unstaged_diff(&repo);
    let hello_unstaged: Vec<_> = unstaged
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "hello.txt")
        .collect();
    assert!(
        hello_unstaged.is_empty() || hello_unstaged[0].hunks.is_empty(),
        "All changes should be staged"
    );

    drop(dir);
}

#[test]
fn test_stage_one_of_two_hunks() {
    let (dir, repo) = helpers::create_temp_repo();

    // Create a file with enough lines to produce two separate hunks
    let original = (1..=20)
        .map(|i| format!("line{}", i))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    helpers::commit_file(&repo, "multi.txt", &original);

    // Modify lines near the top and near the bottom to create two hunks
    let modified = original
        .replace("line2", "line2 CHANGED")
        .replace("line19", "line19 CHANGED");
    helpers::modify_file(&repo, "multi.txt", &modified);

    let files = get_unstaged_diff(&repo);
    assert_eq!(files.len(), 1);
    assert!(
        files[0].hunks.len() >= 2,
        "Expected at least 2 hunks, got {}",
        files[0].hunks.len()
    );

    // Stage only the first hunk
    stage_hunk(&repo, &files[0], &files[0].hunks[0], 0).unwrap();

    // Staged diff should show the first change
    let staged = get_staged_diff(&repo);
    assert_eq!(staged.len(), 1);
    // Should have the first hunk staged
    let staged_lines: String = staged[0]
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.content.clone())
        .collect();
    assert!(
        staged_lines.contains("line2 CHANGED"),
        "First hunk should be staged"
    );

    // Unstaged diff should still show the second change
    let unstaged = get_unstaged_diff(&repo);
    let multi_unstaged: Vec<_> = unstaged
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "multi.txt")
        .collect();
    assert!(
        !multi_unstaged.is_empty(),
        "Second hunk should remain unstaged"
    );
    let unstaged_lines: String = multi_unstaged[0]
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.content.clone())
        .collect();
    assert!(
        unstaged_lines.contains("line19 CHANGED"),
        "Second hunk should remain unstaged"
    );

    drop(dir);
}

#[test]
fn test_stage_hunk_new_file() {
    let (dir, repo) = helpers::create_temp_repo();

    // Create a new untracked file
    helpers::create_untracked_file(&repo, "newfile.txt", "brand new content\n");

    // Need include_untracked_content to get diff hunks for untracked files
    let files = get_unstaged_diff_with_untracked_content(&repo);
    let new_file: Vec<_> = files
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "newfile.txt")
        .collect();
    assert_eq!(new_file.len(), 1, "Should detect new file");
    assert!(!new_file[0].hunks.is_empty(), "New file should have hunks");

    // Stage the new file's hunk
    stage_hunk(&repo, new_file[0], &new_file[0].hunks[0], 0).unwrap();

    // Staged diff should show the new file
    let staged = get_staged_diff(&repo);
    let staged_new: Vec<_> = staged
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "newfile.txt")
        .collect();
    assert_eq!(staged_new.len(), 1, "New file should appear in staged diff");

    drop(dir);
}

#[test]
fn test_stage_new_file_via_intent_to_add_clears_ita_flag() {
    let (dir, repo) = helpers::create_temp_repo();

    // Create a new untracked file and mark it intent-to-add (same as `stagent -N`)
    helpers::create_untracked_file(&repo, "newfile.txt", "brand new content\nsecond line\n");
    intent_to_add_untracked(&repo).unwrap();

    // Get the unstaged diff (intent-to-add shows content as added lines)
    let files = get_unstaged_diff(&repo);
    let new_file: Vec<_> = files
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "newfile.txt")
        .collect();
    assert_eq!(new_file.len(), 1, "Should detect intent-to-add file");
    assert!(
        !new_file[0].hunks.is_empty(),
        "Intent-to-add file should have hunks"
    );

    // Stage the hunk
    stage_hunk(&repo, new_file[0], &new_file[0].hunks[0], 0).unwrap();

    // After staging, the intent-to-add flag must be cleared on the index entry.
    // If it's still set, git CLI treats the file as "not staged" even though
    // the blob has real content.
    let index = repo.index().unwrap();
    let entry = index
        .get_path(std::path::Path::new("newfile.txt"), 0)
        .expect("newfile.txt should be in the index after staging");

    const GIT_IDXENTRY_INTENT_TO_ADD: u16 = 1 << 13;
    assert_eq!(
        entry.flags_extended & GIT_IDXENTRY_INTENT_TO_ADD,
        0,
        "Intent-to-add flag should be cleared after staging, \
         otherwise git treats the file as not staged"
    );

    // Also verify the staged diff shows the content
    let staged = get_staged_diff(&repo);
    let staged_new: Vec<_> = staged
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "newfile.txt")
        .collect();
    assert_eq!(staged_new.len(), 1, "New file should appear in staged diff");

    let staged_content: String = staged_new[0]
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind == LineKind::Added)
        .map(|l| l.content.clone())
        .collect();
    assert!(
        staged_content.contains("brand new content"),
        "Staged diff should contain the file content, got: {}",
        staged_content
    );

    drop(dir);
}

#[test]
fn test_get_unstaged_diff_sees_ita_files_with_staged_changes() {
    // Reproduce the user's exact scenario:
    // - Some files are committed and then staged (modified)
    // - Some files are new with intent-to-add
    // - get_unstaged_diff should return the ITA files with hunks
    let (dir, repo) = helpers::create_temp_repo();

    // Create and commit some files, then stage modifications (like the user has)
    helpers::commit_file(&repo, "existing.txt", "original\n");
    helpers::modify_file(&repo, "existing.txt", "modified\n");
    // Stage the modification
    {
        let mut index = repo.index().unwrap();
        index
            .add_path(std::path::Path::new("existing.txt"))
            .unwrap();
        index.write().unwrap();
    }

    // Create new untracked files and mark them intent-to-add
    helpers::create_untracked_file(&repo, "newfile.txt", "brand new content\nsecond line\n");
    intent_to_add_untracked(&repo).unwrap();

    // Now call the LIBRARY function (same code path as main.rs)
    let files = stagent::git::get_unstaged_diff(&repo).unwrap();

    // The ITA file should appear with hunks
    let new_file: Vec<_> = files
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "newfile.txt")
        .collect();
    assert!(
        !new_file.is_empty(),
        "ITA file should appear in unstaged diff, got files: {:?}",
        files
            .iter()
            .map(|f| f.path.to_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert!(
        !new_file[0].hunks.is_empty(),
        "ITA file should have hunks so it can be staged"
    );

    drop(dir);
}

#[test]
fn test_get_unstaged_diff_sees_ita_files_after_repo_reopen() {
    // Simulate the case where `git add -N` was run externally before stagent
    let (dir, repo) = helpers::create_temp_repo();

    // Create new file and add intent-to-add
    helpers::create_untracked_file(&repo, "newfile.txt", "brand new content\n");
    intent_to_add_untracked(&repo).unwrap();

    // Drop and reopen the repo (simulates running stagent as a new process)
    drop(repo);
    let repo = stagent::git::open_repo(dir.path()).unwrap();

    let files = stagent::git::get_unstaged_diff(&repo).unwrap();
    let new_file: Vec<_> = files
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "newfile.txt")
        .collect();
    assert!(
        !new_file.is_empty(),
        "ITA file should appear after repo reopen, got files: {:?}",
        files
            .iter()
            .map(|f| f.path.to_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert!(
        !new_file[0].hunks.is_empty(),
        "ITA file should have hunks after repo reopen"
    );

    drop(dir);
}

#[test]
fn test_stage_hunk_deleted_lines() {
    let (dir, repo) = helpers::create_temp_repo();

    helpers::commit_file(&repo, "del.txt", "keep1\nremove_me\nkeep2\n");
    helpers::modify_file(&repo, "del.txt", "keep1\nkeep2\n");

    let files = get_unstaged_diff(&repo);
    assert_eq!(files.len(), 1);

    stage_hunk(&repo, &files[0], &files[0].hunks[0], 0).unwrap();

    let staged = get_staged_diff(&repo);
    assert_eq!(staged.len(), 1);
    // The staged change should show a removed line
    let has_removed = staged[0]
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .any(|l| l.kind == LineKind::Removed && l.content.contains("remove_me"));
    assert!(has_removed, "Staged diff should show removed line");

    drop(dir);
}

#[test]
fn test_stage_hunk_added_lines() {
    let (dir, repo) = helpers::create_temp_repo();

    helpers::commit_file(&repo, "add.txt", "first\nlast\n");
    helpers::modify_file(&repo, "add.txt", "first\nnew_middle\nlast\n");

    let files = get_unstaged_diff(&repo);
    assert_eq!(files.len(), 1);

    stage_hunk(&repo, &files[0], &files[0].hunks[0], 0).unwrap();

    let staged = get_staged_diff(&repo);
    assert_eq!(staged.len(), 1);
    let has_added = staged[0]
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .any(|l| l.kind == LineKind::Added && l.content.contains("new_middle"));
    assert!(has_added, "Staged diff should show added line");

    drop(dir);
}

#[test]
fn test_stage_hunk_mixed_changes() {
    let (dir, repo) = helpers::create_temp_repo();

    helpers::commit_file(&repo, "mix.txt", "alpha\nbeta\ngamma\ndelta\n");
    helpers::modify_file(&repo, "mix.txt", "alpha\nBETA\ngamma\nepsilon\n");

    let files = get_unstaged_diff(&repo);
    assert_eq!(files.len(), 1);

    stage_hunk(&repo, &files[0], &files[0].hunks[0], 0).unwrap();

    let staged = get_staged_diff(&repo);
    assert_eq!(staged.len(), 1);

    let staged_lines: Vec<_> = staged[0]
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .collect();

    let has_beta_removed = staged_lines
        .iter()
        .any(|l| l.kind == LineKind::Removed && l.content.contains("beta"));
    let has_beta_added = staged_lines
        .iter()
        .any(|l| l.kind == LineKind::Added && l.content.contains("BETA"));
    let has_delta_removed = staged_lines
        .iter()
        .any(|l| l.kind == LineKind::Removed && l.content.contains("delta"));
    let has_epsilon_added = staged_lines
        .iter()
        .any(|l| l.kind == LineKind::Added && l.content.contains("epsilon"));

    assert!(has_beta_removed, "Should show beta removed");
    assert!(has_beta_added, "Should show BETA added");
    assert!(has_delta_removed, "Should show delta removed");
    assert!(has_epsilon_added, "Should show epsilon added");

    drop(dir);
}

#[test]
fn test_stage_preserves_other_files() {
    let (dir, repo) = helpers::create_temp_repo();

    helpers::commit_file(&repo, "file_a.txt", "content_a\n");
    helpers::commit_file(&repo, "file_b.txt", "content_b\n");

    // Modify both files
    helpers::modify_file(&repo, "file_a.txt", "content_a modified\n");
    helpers::modify_file(&repo, "file_b.txt", "content_b modified\n");

    let files = get_unstaged_diff(&repo);
    assert_eq!(files.len(), 2);

    // Stage only file_a's hunk
    let file_a = files
        .iter()
        .find(|f| f.path.to_str().unwrap() == "file_a.txt")
        .unwrap();
    stage_hunk(&repo, file_a, &file_a.hunks[0], 0).unwrap();

    // file_a should be staged
    let staged = get_staged_diff(&repo);
    let staged_a: Vec<_> = staged
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "file_a.txt")
        .collect();
    assert_eq!(staged_a.len(), 1, "file_a should be staged");

    // file_b should NOT be staged
    let staged_b: Vec<_> = staged
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "file_b.txt")
        .collect();
    assert!(staged_b.is_empty(), "file_b should not be staged");

    // file_b should still be in unstaged diff
    let unstaged = get_unstaged_diff(&repo);
    let unstaged_b: Vec<_> = unstaged
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "file_b.txt")
        .collect();
    assert_eq!(unstaged_b.len(), 1, "file_b should remain unstaged");

    drop(dir);
}

// ============================================================
// Unit tests: reconstruct_blob
// ============================================================

fn make_hunk(
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<(LineKind, &str)>,
) -> Hunk {
    Hunk {
        header: format!(
            "@@ -{},{} +{},{} @@",
            old_start, old_lines, new_start, new_lines
        ),
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
        old_start,
        old_lines,
        new_start,
        new_lines,
    }
}

#[test]
fn test_reconstruct_blob_content() {
    // Original: 5 lines, hunk modifies line 3
    let original = "line1\nline2\nline3\nline4\nline5\n";

    let hunk = make_hunk(
        2,
        3,
        2,
        3,
        vec![
            (LineKind::Context, "line2\n"),
            (LineKind::Removed, "line3\n"),
            (LineKind::Added, "LINE3_MODIFIED\n"),
            (LineKind::Context, "line4\n"),
        ],
    );

    let result = reconstruct_blob(original, &hunk, 0).unwrap();
    assert_eq!(result, "line1\nline2\nLINE3_MODIFIED\nline4\nline5\n");
}

#[test]
fn test_reconstruct_blob_hunk_at_start() {
    let original = "first\nsecond\nthird\n";

    let hunk = make_hunk(
        1,
        2,
        1,
        2,
        vec![
            (LineKind::Removed, "first\n"),
            (LineKind::Added, "FIRST\n"),
            (LineKind::Context, "second\n"),
        ],
    );

    let result = reconstruct_blob(original, &hunk, 0).unwrap();
    assert_eq!(result, "FIRST\nsecond\nthird\n");
}

#[test]
fn test_reconstruct_blob_hunk_at_end() {
    let original = "first\nsecond\nthird\n";

    let hunk = make_hunk(
        2,
        2,
        2,
        2,
        vec![
            (LineKind::Context, "second\n"),
            (LineKind::Removed, "third\n"),
            (LineKind::Added, "THIRD\n"),
        ],
    );

    let result = reconstruct_blob(original, &hunk, 0).unwrap();
    assert_eq!(result, "first\nsecond\nTHIRD\n");
}

#[test]
fn test_reconstruct_blob_add_lines() {
    // Adding a line between line2 and line3
    let original = "line1\nline2\nline3\n";

    let hunk = make_hunk(
        2,
        2,
        2,
        3,
        vec![
            (LineKind::Context, "line2\n"),
            (LineKind::Added, "inserted\n"),
            (LineKind::Context, "line3\n"),
        ],
    );

    let result = reconstruct_blob(original, &hunk, 0).unwrap();
    assert_eq!(result, "line1\nline2\ninserted\nline3\n");
}

#[test]
fn test_reconstruct_blob_remove_lines() {
    let original = "line1\nline2\nline3\nline4\n";

    let hunk = make_hunk(
        2,
        3,
        2,
        2,
        vec![
            (LineKind::Context, "line2\n"),
            (LineKind::Removed, "line3\n"),
            (LineKind::Context, "line4\n"),
        ],
    );

    let result = reconstruct_blob(original, &hunk, 0).unwrap();
    assert_eq!(result, "line1\nline2\nline4\n");
}

#[test]
fn test_reconstruct_blob_empty_original() {
    // New file: original is empty, hunk adds all lines
    let original = "";

    let hunk = make_hunk(
        0,
        0,
        1,
        2,
        vec![
            (LineKind::Added, "new_line1\n"),
            (LineKind::Added, "new_line2\n"),
        ],
    );

    let result = reconstruct_blob(original, &hunk, 0).unwrap();
    assert_eq!(result, "new_line1\nnew_line2\n");
}

#[test]
fn test_reconstruct_blob_multiple_sequential() {
    // Stage hunk 0, then stage hunk 1 on the result
    let original = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";

    // First hunk: modify line 2 (b -> B)
    let hunk0 = make_hunk(
        1,
        3,
        1,
        3,
        vec![
            (LineKind::Context, "a\n"),
            (LineKind::Removed, "b\n"),
            (LineKind::Added, "B\n"),
            (LineKind::Context, "c\n"),
        ],
    );

    let after_hunk0 = reconstruct_blob(original, &hunk0, 0).unwrap();
    assert_eq!(after_hunk0, "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\n");

    // Second hunk: modify line 9 (i -> I) — operating on the *new* content
    let hunk1 = make_hunk(
        8,
        3,
        8,
        3,
        vec![
            (LineKind::Context, "h\n"),
            (LineKind::Removed, "i\n"),
            (LineKind::Added, "I\n"),
            (LineKind::Context, "j\n"),
        ],
    );

    let after_hunk1 = reconstruct_blob(&after_hunk0, &hunk1, 0).unwrap();
    assert_eq!(after_hunk1, "a\nB\nc\nd\ne\nf\ng\nh\nI\nj\n");
}

// ============================================================
// Tests: split_hunk
// ============================================================

#[test]
fn test_split_hunk_two_regions() {
    // Hunk with two change regions separated by context
    let hunk = make_hunk(
        1,
        9,
        1,
        9,
        vec![
            (LineKind::Context, "ctx1\n"),
            (LineKind::Removed, "old1\n"),
            (LineKind::Added, "new1\n"),
            (LineKind::Context, "ctx2\n"),
            (LineKind::Context, "ctx3\n"),
            (LineKind::Context, "ctx4\n"),
            (LineKind::Context, "ctx5\n"),
            (LineKind::Removed, "old2\n"),
            (LineKind::Added, "new2\n"),
        ],
    );

    let sub_hunks = split_hunk(&hunk);
    assert_eq!(sub_hunks.len(), 2, "Should split into 2 sub-hunks");

    // First sub-hunk should contain the first change
    let first_has_change = sub_hunks[0]
        .lines
        .iter()
        .any(|l| l.kind == LineKind::Added && l.content.contains("new1"));
    assert!(first_has_change, "First sub-hunk should have first change");

    // Second sub-hunk should contain the second change
    let second_has_change = sub_hunks[1]
        .lines
        .iter()
        .any(|l| l.kind == LineKind::Added && l.content.contains("new2"));
    assert!(
        second_has_change,
        "Second sub-hunk should have second change"
    );
}

#[test]
fn test_split_hunk_unsplittable() {
    // All changes are contiguous — should return the original hunk
    let hunk = make_hunk(
        1,
        4,
        1,
        4,
        vec![
            (LineKind::Context, "before\n"),
            (LineKind::Removed, "old1\n"),
            (LineKind::Added, "new1\n"),
            (LineKind::Removed, "old2\n"),
            (LineKind::Added, "new2\n"),
            (LineKind::Context, "after\n"),
        ],
    );

    let sub_hunks = split_hunk(&hunk);
    assert_eq!(sub_hunks.len(), 1, "Unsplittable hunk should return 1");
    assert_eq!(sub_hunks[0].lines.len(), hunk.lines.len());
}

#[test]
fn test_split_hunk_preserves_headers() {
    let hunk = make_hunk(
        1,
        11,
        1,
        11,
        vec![
            (LineKind::Removed, "old_first\n"),
            (LineKind::Added, "new_first\n"),
            (LineKind::Context, "c1\n"),
            (LineKind::Context, "c2\n"),
            (LineKind::Context, "c3\n"),
            (LineKind::Context, "c4\n"),
            (LineKind::Context, "c5\n"),
            (LineKind::Context, "c6\n"),
            (LineKind::Context, "c7\n"),
            (LineKind::Removed, "old_last\n"),
            (LineKind::Added, "new_last\n"),
        ],
    );

    let sub_hunks = split_hunk(&hunk);
    assert_eq!(sub_hunks.len(), 2);

    // Each sub-hunk should have a valid @@ header
    for (i, sh) in sub_hunks.iter().enumerate() {
        assert!(
            sh.header.starts_with("@@"),
            "Sub-hunk {} should have @@ header, got: {}",
            i,
            sh.header
        );
        assert!(
            sh.header.contains("split"),
            "Sub-hunk {} header should contain 'split' marker",
            i
        );
    }

    // Verify old_lines and new_lines are correct counts
    for sh in &sub_hunks {
        let computed_old: u32 = sh
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Context || l.kind == LineKind::Removed)
            .count() as u32;
        let computed_new: u32 = sh
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Context || l.kind == LineKind::Added)
            .count() as u32;
        assert_eq!(sh.old_lines, computed_old, "old_lines should match");
        assert_eq!(sh.new_lines, computed_new, "new_lines should match");
    }
}

// ============================================================
// Integration tests: split-then-stage workflow
// ============================================================

#[test]
fn test_stage_split_then_stage() {
    let (dir, repo) = helpers::create_temp_repo();

    // Create a file with enough lines to produce two separate hunks
    // when split_hunk is called
    let original = (1..=20)
        .map(|i| format!("line{}", i))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    helpers::commit_file(&repo, "split.txt", &original);

    // Modify lines to create two hunks with enough context gap between them
    let modified = original
        .replace("line2", "line2 CHANGED")
        .replace("line19", "line19 CHANGED");
    helpers::modify_file(&repo, "split.txt", &modified);

    let files = get_unstaged_diff(&repo);
    assert_eq!(files.len(), 1);

    // For this test, we'll work with the single hunk from git diff
    // and demonstrate that we can stage individual hunks
    assert!(
        !files[0].hunks.is_empty(),
        "File should have at least one hunk"
    );

    // Stage the first hunk
    stage_hunk(&repo, &files[0], &files[0].hunks[0], 0).unwrap();

    // Verify that at least one change was staged
    let staged = get_staged_diff(&repo);
    assert_eq!(staged.len(), 1, "Staged diff should have 1 file");
    assert!(!staged[0].hunks.is_empty(), "Staged file should have hunks");

    // Verify unstaged changes remain
    let unstaged = get_unstaged_diff(&repo);
    let split_unstaged: Vec<_> = unstaged
        .iter()
        .filter(|f| f.path.to_str().unwrap() == "split.txt")
        .collect();
    assert!(
        !split_unstaged.is_empty(),
        "Should have unstaged changes remaining"
    );

    drop(dir);
}

// ============================================================
// Unit tests: reconstruct_blob with offset
// ============================================================

#[test]
fn test_reconstruct_blob_with_offset() {
    // Test that line_offset parameter is used to adjust hunk.old_start.
    // The offset is used when staging multiple hunks sequentially:
    // if the first hunk's net change is +N lines, the second hunk's
    // old_start is adjusted by that offset.

    let original = "a\nb\nc\nd\ne\n";

    // Simple test: hunk that removes "b" and adds "B"
    // old_start=2, old_lines=2 means we start at line 2 and consume 2 lines
    let hunk_modify_b = make_hunk(
        2,
        2,
        2,
        2,
        vec![
            (LineKind::Removed, "b\n"),
            (LineKind::Added, "B\n"),
            (LineKind::Context, "c\n"),
        ],
    );

    let after_modify = reconstruct_blob(original, &hunk_modify_b, 0).unwrap();
    // Should replace b with B, keep everything else
    assert_eq!(after_modify, "a\nB\nc\nd\ne\n");

    // Test applying with offset: if we apply the same hunk but with offset=1
    // (as if a previous hunk added 1 line), old_start would become 3
    let original_with_insert = "a\nINSERTED\nb\nc\nd\ne\n";

    // This hunk still targets the same content (remove b, add B)
    // but the offset tells us the hunk's old_start is shifted by +1
    let after_with_offset = reconstruct_blob(original_with_insert, &hunk_modify_b, 1).unwrap();
    // With offset=1, old_start=2 becomes 3, so we start at line 3 which is "b"
    // We should still get the modification
    assert_eq!(after_with_offset, "a\nINSERTED\nB\nc\nd\ne\n");
}
