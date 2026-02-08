//! Spawn stagent in a tmux split pane and wait for completion.
//!
//! This module provides the `--spawn` functionality that allows Claude (or other
//! tools) to launch stagent in a new tmux split, wait for the user to complete
//! their review, and then read the feedback output.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::editor::pane_exists;

/// Options for spawning stagent in a split pane.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Output file for feedback (--output)
    pub output: Option<PathBuf>,
    /// Glob pattern for filtering files (--files)
    pub files: Option<String>,
    /// Theme name (--theme)
    pub theme: String,
    /// Context lines for feedback (--context-lines)
    pub context_lines: usize,
    /// No-stage mode (--no-stage)
    pub no_stage: bool,
}

/// Build the tmux split-window command for spawning stagent.
///
/// Constructs a command that:
/// - Opens a horizontal split at 50% width
/// - Returns the pane ID via -P -F '#{pane_id}'
/// - Runs stagent with forwarded CLI args (but NOT --spawn)
pub fn build_spawn_command(opts: &SpawnOptions) -> Vec<String> {
    let mut cmd = vec![
        "tmux".to_string(),
        "split-window".to_string(),
        "-h".to_string(),
        "-p".to_string(),
        "50".to_string(),
        "-P".to_string(),
        "-F".to_string(),
        "#{pane_id}".to_string(),
        "--".to_string(),
    ];

    // Get the current executable path
    let stagent_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "stagent".to_string());

    cmd.push(stagent_exe);

    // Forward CLI args (but NOT --spawn to avoid infinite recursion)
    if let Some(ref output) = opts.output {
        cmd.push("--output".to_string());
        cmd.push(output.to_string_lossy().to_string());
    }

    if let Some(ref files) = opts.files {
        cmd.push("--files".to_string());
        cmd.push(files.clone());
    }

    if opts.theme != "default" {
        cmd.push("--theme".to_string());
        cmd.push(opts.theme.clone());
    }

    if opts.context_lines != crate::feedback::DEFAULT_CONTEXT_LINES {
        cmd.push("--context-lines".to_string());
        cmd.push(opts.context_lines.to_string());
    }

    if opts.no_stage {
        cmd.push("--no-stage".to_string());
    }

    cmd
}

/// Maximum number of poll iterations before giving up.
/// At 500ms per poll, this is ~30 minutes.
const MAX_SPAWN_POLL_ITERATIONS: u32 = 3600;

/// Spawn stagent in a tmux split pane and wait for it to complete.
///
/// Returns Ok(()) when the spawned stagent completes, or an error if
/// the spawn fails.
pub fn spawn_in_split(opts: &SpawnOptions) -> Result<()> {
    let cmd = build_spawn_command(opts);

    let output = Command::new(&cmd[0])
        .args(&cmd[1..])
        .output()
        .context("Failed to run tmux split-window")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("tmux split-window failed: {}", stderr);
    }

    let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if pane_id.is_empty() {
        bail!("tmux split-window did not return a pane ID");
    }

    // Poll until the pane closes
    wait_for_pane(&pane_id)?;

    Ok(())
}

/// Block until the given tmux pane closes.
fn wait_for_pane(pane_id: &str) -> Result<()> {
    for _ in 0..MAX_SPAWN_POLL_ITERATIONS {
        if !pane_exists(pane_id) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }

    bail!("Timed out waiting for stagent pane to close");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_opts() -> SpawnOptions {
        SpawnOptions {
            output: None,
            files: None,
            theme: "default".to_string(),
            context_lines: crate::feedback::DEFAULT_CONTEXT_LINES,
            no_stage: false,
        }
    }

    #[test]
    fn test_build_spawn_command_basic() {
        let opts = default_opts();
        let cmd = build_spawn_command(&opts);

        assert_eq!(cmd[0], "tmux");
        assert_eq!(cmd[1], "split-window");
        assert!(cmd.contains(&"-h".to_string()));
        assert!(cmd.contains(&"-p".to_string()));
        assert!(cmd.contains(&"50".to_string()));
        assert!(cmd.contains(&"-P".to_string()));
        assert!(cmd.contains(&"#{pane_id}".to_string()));
        assert!(cmd.contains(&"--".to_string()));

        // Should NOT contain --spawn
        assert!(
            !cmd.contains(&"--spawn".to_string()),
            "Command should not contain --spawn"
        );
    }

    #[test]
    fn test_build_spawn_command_with_output() {
        let opts = SpawnOptions {
            output: Some(PathBuf::from("/tmp/feedback.diff")),
            ..default_opts()
        };
        let cmd = build_spawn_command(&opts);

        assert!(cmd.contains(&"--output".to_string()));
        assert!(cmd.contains(&"/tmp/feedback.diff".to_string()));
    }

    #[test]
    fn test_build_spawn_command_with_files() {
        let opts = SpawnOptions {
            files: Some("*.rs".to_string()),
            ..default_opts()
        };
        let cmd = build_spawn_command(&opts);

        assert!(cmd.contains(&"--files".to_string()));
        assert!(cmd.contains(&"*.rs".to_string()));
    }

    #[test]
    fn test_build_spawn_command_with_theme() {
        let opts = SpawnOptions {
            theme: "dark".to_string(),
            ..default_opts()
        };
        let cmd = build_spawn_command(&opts);

        assert!(cmd.contains(&"--theme".to_string()));
        assert!(cmd.contains(&"dark".to_string()));
    }

    #[test]
    fn test_build_spawn_command_default_theme_not_included() {
        let opts = default_opts();
        let cmd = build_spawn_command(&opts);

        // Default theme should not be explicitly passed
        assert!(!cmd.contains(&"--theme".to_string()));
    }

    #[test]
    fn test_build_spawn_command_with_no_stage() {
        let opts = SpawnOptions {
            no_stage: true,
            ..default_opts()
        };
        let cmd = build_spawn_command(&opts);

        assert!(cmd.contains(&"--no-stage".to_string()));
    }

    #[test]
    fn test_build_spawn_command_with_context_lines() {
        let opts = SpawnOptions {
            context_lines: 10, // Use non-default value
            ..default_opts()
        };
        let cmd = build_spawn_command(&opts);

        assert!(cmd.contains(&"--context-lines".to_string()));
        assert!(cmd.contains(&"10".to_string()));
    }

    #[test]
    fn test_build_spawn_command_default_context_lines_not_included() {
        let opts = default_opts();
        let cmd = build_spawn_command(&opts);

        // Default context lines should not be explicitly passed
        assert!(!cmd.contains(&"--context-lines".to_string()));
    }

    #[test]
    fn test_build_spawn_command_all_options() {
        let opts = SpawnOptions {
            output: Some(PathBuf::from("/tmp/out.diff")),
            files: Some("src/*.rs".to_string()),
            theme: "monokai".to_string(),
            context_lines: 10,
            no_stage: true,
        };
        let cmd = build_spawn_command(&opts);

        assert!(cmd.contains(&"--output".to_string()));
        assert!(cmd.contains(&"/tmp/out.diff".to_string()));
        assert!(cmd.contains(&"--files".to_string()));
        assert!(cmd.contains(&"src/*.rs".to_string()));
        assert!(cmd.contains(&"--theme".to_string()));
        assert!(cmd.contains(&"monokai".to_string()));
        assert!(cmd.contains(&"--context-lines".to_string()));
        assert!(cmd.contains(&"10".to_string()));
        assert!(cmd.contains(&"--no-stage".to_string()));
        assert!(!cmd.contains(&"--spawn".to_string()));
    }
}
