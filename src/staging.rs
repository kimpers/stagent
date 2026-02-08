use anyhow::{Context, Result, bail};
use git2::Repository;
use std::path::Path;

use crate::types::{FileDiff, Hunk, LineKind};

/// Stage a single hunk by reconstructing the blob content in the index.
///
/// `line_offset` accounts for line count changes introduced by previously
/// staged hunks in the same file. When staging hunks sequentially, earlier
/// hunks may add or remove lines, shifting the positions of later hunks.
/// The caller must compute this as the sum of `(new_lines - old_lines)` for
/// all previously staged hunks that appear before this one in the file.
///
/// Algorithm (same approach as gitui):
/// 1. Read the file's current content from the index (or empty for new/untracked files)
/// 2. Apply the hunk's changes to produce a new version of the file
/// 3. Write the new content as a blob
/// 4. Update the index entry with the new blob OID
/// 5. Write the index to disk
pub fn stage_hunk(
    repo: &Repository,
    file_diff: &FileDiff,
    hunk: &Hunk,
    line_offset: i32,
) -> Result<()> {
    let file_path = &file_diff.path;
    let mut index = repo.index().context("Failed to get repository index")?;

    // Read current index content (what's already staged or HEAD content)
    let old_content = get_index_content(repo, file_path)?;

    // Reconstruct content with this hunk applied (adjusting for offset)
    let new_content = reconstruct_blob(&old_content, hunk, line_offset)?;

    // Write the new blob
    let blob_oid = repo
        .blob(new_content.as_bytes())
        .context("Failed to write blob")?;

    // Create/update the index entry
    let file_path_str = file_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("File path is not valid UTF-8: {:?}", file_path))?;

    // Get existing entry or create new one
    let mut entry = if let Some(existing) = index.get_path(Path::new(file_path_str), 0) {
        existing
    } else {
        // New file - create a fresh index entry
        let workdir = repo.workdir().context("Bare repository not supported")?;
        let full_path = workdir.join(file_path);
        let _metadata = std::fs::metadata(&full_path)
            .with_context(|| format!("Failed to read metadata for {}", full_path.display()))?;

        git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o100644,
            uid: 0,
            gid: 0,
            file_size: new_content.len() as u32,
            id: blob_oid,
            flags: 0,
            flags_extended: 0,
            path: file_path_str.as_bytes().to_vec(),
        }
    };

    entry.id = blob_oid;
    entry.file_size = new_content.len() as u32;

    // Clear the intent-to-add flag if present. Without this, files added
    // via `git add -N` (intent-to-add) would retain the flag after staging,
    // causing git to treat them as not actually staged.
    const GIT_IDXENTRY_INTENT_TO_ADD: u16 = 1 << 13;
    entry.flags_extended &= !GIT_IDXENTRY_INTENT_TO_ADD;

    index.add(&entry).context("Failed to update index entry")?;
    index.write().context("Failed to write index")?;

    Ok(())
}

/// Read the current content of a file from the index/HEAD.
/// Returns empty string for untracked/new files.
fn get_index_content(repo: &Repository, path: &Path) -> Result<String> {
    let index = repo.index().context("Failed to get index")?;
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("File path is not valid UTF-8: {:?}", path))?;

    if let Some(entry) = index.get_path(Path::new(path_str), 0) {
        let blob = repo
            .find_blob(entry.id)
            .context("Failed to find blob for index entry")?;
        if blob.content().contains(&0) {
            bail!(
                "File appears to be binary (contains null bytes): {:?}",
                path
            );
        }
        let content = String::from_utf8(blob.content().to_vec())
            .with_context(|| format!("File is not valid UTF-8: {:?}", path))?;
        Ok(content)
    } else {
        // Try HEAD tree
        if let Ok(head) = repo.head()
            && let Ok(tree) = head.peel_to_tree()
            && let Ok(entry) = tree.get_path(Path::new(path_str))
        {
            let obj = entry
                .to_object(repo)
                .context("Failed to resolve tree entry")?;
            if let Some(blob) = obj.as_blob() {
                if blob.content().contains(&0) {
                    bail!(
                        "File appears to be binary (contains null bytes): {:?}",
                        path
                    );
                }
                return String::from_utf8(blob.content().to_vec())
                    .with_context(|| format!("File is not valid UTF-8: {:?}", path));
            }
        }
        // New file - return empty
        Ok(String::new())
    }
}

/// Reconstruct file content with a single hunk applied.
///
/// `line_offset` adjusts `old_start` to account for line count changes
/// from previously staged hunks in the same file.
///
/// This walks the original file line-by-line. When we reach the hunk's
/// target range, we apply the changes (keep context, add '+' lines, skip '-' lines).
/// Outside the hunk range, we keep original content unchanged.
pub fn reconstruct_blob(original: &str, hunk: &Hunk, line_offset: i32) -> Result<String> {
    let orig_lines: Vec<&str> = if original.is_empty() {
        Vec::new()
    } else {
        original.lines().collect()
    };

    let mut result = Vec::new();
    let adjusted_start = (hunk.old_start as i32 + line_offset).max(0) as usize;
    // old_start is 1-based, convert to 0-based index
    let hunk_start_idx = if adjusted_start == 0 {
        0
    } else {
        adjusted_start - 1
    };

    // Count original lines consumed by this hunk (context + removed)
    let hunk_old_line_count = hunk.old_lines as usize;

    // Copy lines before the hunk
    for line in orig_lines.iter().take(hunk_start_idx) {
        result.push(line.to_string());
    }

    // Apply hunk lines
    for diff_line in &hunk.lines {
        match diff_line.kind {
            LineKind::Context | LineKind::Added => {
                // Trim trailing newline if present (we re-add with join)
                let content = diff_line.content.trim_end_matches('\n');
                result.push(content.to_string());
            }
            LineKind::Removed => {
                // Skip removed lines - they are consumed from original
            }
        }
    }

    // Copy lines after the hunk
    let after_hunk_idx = hunk_start_idx + hunk_old_line_count;
    for line in orig_lines.iter().skip(after_hunk_idx) {
        result.push(line.to_string());
    }

    // Preserve trailing newline if original had one
    let mut output = result.join("\n");
    if original.ends_with('\n') || original.is_empty() {
        output.push('\n');
    }

    Ok(output)
}
