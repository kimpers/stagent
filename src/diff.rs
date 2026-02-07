use anyhow::Result;
use git2::Diff;

use crate::types::{DeltaStatus, DiffLine, FileDiff, Hunk, HunkStatus, LineKind};

/// Parse a git2 Diff into our structured FileDiff types.
///
/// Uses `diff.print()` with DiffFormat::Patch to iterate through all lines,
/// which avoids the multiple mutable borrow issues of `diff.foreach()`.
pub fn parse_diff(diff: &Diff) -> Result<Vec<FileDiff>> {
    let mut files: Vec<FileDiff> = Vec::new();

    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .unwrap_or_else(|| std::path::Path::new("<unknown>"))
            .to_path_buf();

        let status = match delta.status() {
            git2::Delta::Added => DeltaStatus::Added,
            git2::Delta::Deleted => DeltaStatus::Deleted,
            git2::Delta::Renamed => DeltaStatus::Renamed,
            git2::Delta::Untracked => DeltaStatus::Untracked,
            _ => DeltaStatus::Modified,
        };

        let is_binary = delta.flags().contains(git2::DiffFlags::BINARY);

        files.push(FileDiff {
            path,
            hunks: Vec::new(),
            status,
            is_binary,
        });
    }

    // Now parse hunks and lines using the patch API
    for (file_idx, file) in files.iter_mut().enumerate() {
        if file.is_binary {
            continue;
        }

        if let Ok(Some(patch)) = git2::Patch::from_diff(diff, file_idx) {
            let num_hunks = patch.num_hunks();

            for hunk_idx in 0..num_hunks {
                let (hunk_header, num_lines) = patch.hunk(hunk_idx).unwrap();
                let header = String::from_utf8_lossy(hunk_header.header())
                    .trim_end()
                    .to_string();

                let mut lines = Vec::new();

                for line_idx in 0..num_lines {
                    match patch.line_in_hunk(hunk_idx, line_idx) {
                        Ok(line) => {
                            let kind = match line.origin() {
                                '+' => LineKind::Added,
                                '-' => LineKind::Removed,
                                _ => LineKind::Context,
                            };

                            let content = String::from_utf8_lossy(line.content()).to_string();

                            lines.push(DiffLine {
                                kind,
                                content,
                                old_lineno: line.old_lineno(),
                                new_lineno: line.new_lineno(),
                            });
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: failed to read line {} of hunk {} in {}: {}",
                                line_idx,
                                hunk_idx,
                                file.path.display(),
                                e
                            );
                        }
                    }
                }

                file.hunks.push(Hunk {
                    header,
                    lines,
                    status: HunkStatus::Pending,
                    old_start: hunk_header.old_start(),
                    old_lines: hunk_header.old_lines(),
                    new_start: hunk_header.new_start(),
                    new_lines: hunk_header.new_lines(),
                });
            }
        }
    }

    Ok(files)
}

/// Split a hunk into smaller sub-hunks at context-only boundaries.
/// Each sub-hunk must contain at least one added or removed line.
/// If the hunk cannot be split (all changes are contiguous), returns a vec with the original hunk.
pub fn split_hunk(hunk: &Hunk) -> Vec<Hunk> {
    // Find regions of changes separated by context lines.
    // A "region" is a contiguous group of Added/Removed lines.
    let mut regions: Vec<(usize, usize)> = Vec::new(); // (start_idx, end_idx) inclusive
    let mut in_region = false;
    let mut region_start = 0;

    for (i, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            LineKind::Added | LineKind::Removed => {
                if !in_region {
                    region_start = i;
                    in_region = true;
                }
            }
            LineKind::Context => {
                if in_region {
                    regions.push((region_start, i - 1));
                    in_region = false;
                }
            }
        }
    }
    if in_region {
        regions.push((region_start, hunk.lines.len() - 1));
    }

    if regions.len() <= 1 {
        return vec![hunk.clone()];
    }

    // Split into sub-hunks. Each sub-hunk includes:
    // - Up to 3 context lines before the region
    // - The changed region
    // - Up to 3 context lines after the region
    // Context windows are clamped to avoid overlap with adjacent sub-hunks.
    let context_lines = 3usize;
    let mut sub_hunks = Vec::new();

    for (region_idx, &(start, end)) in regions.iter().enumerate() {
        let ctx_before_start = start.saturating_sub(context_lines);
        let ctx_after_end = (end + context_lines).min(hunk.lines.len() - 1);

        // Clamp to avoid overlap with the previous sub-hunk's after-context
        let clamped_before = if region_idx > 0 {
            let prev_end = regions[region_idx - 1].1;
            let prev_after = (prev_end + context_lines).min(hunk.lines.len() - 1);
            // Start after previous sub-hunk's after-context, but don't go past our region
            ctx_before_start.max(prev_after + 1).min(start)
        } else {
            ctx_before_start
        };

        // Clamp to avoid overlap with the next sub-hunk's before-context
        let clamped_after = if region_idx + 1 < regions.len() {
            let next_start = regions[region_idx + 1].0;
            let next_before = next_start.saturating_sub(context_lines);
            // End before next sub-hunk's before-context, but don't go before our region
            ctx_after_end.min(next_before.saturating_sub(1)).max(end)
        } else {
            ctx_after_end
        };

        let lines: Vec<DiffLine> = hunk.lines[clamped_before..=clamped_after].to_vec();

        // Compute line numbers for the sub-hunk header
        let mut old_count = 0u32;
        let mut new_count = 0u32;
        for line in &lines {
            match line.kind {
                LineKind::Context => {
                    old_count += 1;
                    new_count += 1;
                }
                LineKind::Removed => old_count += 1,
                LineKind::Added => new_count += 1,
            }
        }

        // Calculate old_start from the first line's old_lineno, or estimate
        let old_start = lines
            .iter()
            .find_map(|l| l.old_lineno)
            .unwrap_or(hunk.old_start);
        let new_start = lines
            .iter()
            .find_map(|l| l.new_lineno)
            .unwrap_or(hunk.new_start);

        let header = format!(
            "@@ -{},{} +{},{} @@ split {}/{}",
            old_start,
            old_count,
            new_start,
            new_count,
            region_idx + 1,
            regions.len()
        );

        sub_hunks.push(Hunk {
            header,
            lines,
            status: HunkStatus::Pending,
            old_start,
            old_lines: old_count,
            new_start,
            new_lines: new_count,
        });
    }

    sub_hunks
}
