#![allow(dead_code)]

use git2::{Repository, Signature};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Create a temporary git repository with an initial commit.
pub fn create_temp_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let repo = Repository::init(dir.path()).expect("Failed to init repo");

    // Create initial commit so HEAD exists
    {
        let mut index = repo.index().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();
    }

    (dir, repo)
}

/// Add and commit a file to the repository.
pub fn commit_file(repo: &Repository, path: &str, content: &str) {
    let workdir = repo.workdir().expect("Not a bare repo");
    let full_path = workdir.join(path);

    // Create parent directories if needed
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    fs::write(&full_path, content).unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();

    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let sig = Signature::now("Test", "test@test.com").unwrap();

    let head = repo.head().unwrap();
    let parent_commit = head.peel_to_commit().unwrap();

    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("Add {}", path),
        &tree,
        &[&parent_commit],
    )
    .unwrap();
}

/// Modify a file in the working directory (without staging).
pub fn modify_file(repo: &Repository, path: &str, content: &str) {
    let workdir = repo.workdir().expect("Not a bare repo");
    let full_path = workdir.join(path);

    // Create parent directories if needed
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    fs::write(&full_path, content).unwrap();
}

/// Create a new untracked file in the working directory.
pub fn create_untracked_file(repo: &Repository, path: &str, content: &str) {
    let workdir = repo.workdir().expect("Not a bare repo");
    let full_path = workdir.join(path);

    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    fs::write(&full_path, content).unwrap();
}

/// Delete a tracked file from the working directory.
pub fn delete_file(repo: &Repository, path: &str) {
    let workdir = repo.workdir().expect("Not a bare repo");
    let full_path = workdir.join(path);
    fs::remove_file(&full_path).unwrap();
}

/// Create a binary file in the working directory.
pub fn create_binary_file(repo: &Repository, path: &str) {
    let workdir = repo.workdir().expect("Not a bare repo");
    let full_path = workdir.join(path);

    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    // Write actual binary content (non-UTF8 bytes)
    let binary_content: Vec<u8> = (0..256).map(|i| i as u8).collect();
    fs::write(&full_path, &binary_content).unwrap();
}
