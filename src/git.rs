use anyhow::{Context, Result};
use git2::{DiffOptions, Repository};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::diff;
use crate::types::FileDiff;

/// Open a git repository at the given path.
pub fn open_repo(path: impl AsRef<Path>) -> Result<Repository> {
    Repository::discover(path.as_ref())
        .context("Failed to open git repository. Are you in a git repo?")
}

/// Add all untracked files to the index with intent-to-add (`git add -N`).
/// This creates an empty blob entry for each untracked file so its full
/// content appears as unstaged changes in the diff.
pub fn intent_to_add_untracked(repo: &Repository) -> Result<()> {
    let statuses = repo.statuses(None).context("Failed to get repo status")?;

    let untracked: Vec<String> = statuses
        .iter()
        .filter(|e| e.status().contains(git2::Status::WT_NEW))
        .filter_map(|e| e.path().map(String::from))
        .collect();

    if untracked.is_empty() {
        return Ok(());
    }

    let mut index = repo.index().context("Failed to open index")?;
    let empty_oid = repo.blob(&[]).context("Failed to create empty blob")?;

    for path in &untracked {
        let file_path = repo.workdir().context("Bare repo")?.join(path);
        let metadata =
            std::fs::metadata(&file_path).with_context(|| format!("Failed to stat {}", path))?;

        let mut entry = git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: if metadata.permissions().mode() & 0o111 != 0 {
                0o100755
            } else {
                0o100644
            },
            uid: 0,
            gid: 0,
            file_size: 0,
            id: empty_oid,
            flags: 0,
            flags_extended: 0,
            path: path.as_bytes().to_vec(),
        };
        const GIT_IDXENTRY_INTENT_TO_ADD: u16 = 1 << 13;
        entry.flags_extended |= GIT_IDXENTRY_INTENT_TO_ADD;

        index
            .add(&entry)
            .with_context(|| format!("Failed to add intent-to-add for {}", path))?;
    }

    index.write().context("Failed to write index")?;

    // Force the repo to reload the index from disk so subsequent
    // diff_index_to_workdir calls see the newly added ITA entries.
    repo.set_index(&mut repo.index().context("Failed to reload index")?)
        .context("Failed to refresh repo index")?;

    Ok(())
}

/// Get all unstaged changes as a list of FileDiff.
pub fn get_unstaged_diff(repo: &Repository) -> Result<Vec<FileDiff>> {
    let index = repo.index().context("Failed to open index")?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    opts.show_untracked_content(true);

    let diff = repo
        .diff_index_to_workdir(Some(&index), Some(&mut opts))
        .context("Failed to compute diff")?;

    diff::parse_diff(&diff)
}
