//! Spawn stagent in the preferred review environment and wait for completion.
//!
//! This module provides the `--spawn` functionality that allows tools to launch
//! stagent in a cmux split when available, fall back to tmux, and finally run
//! full-screen in the current terminal if no multiplexer is active.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::mux::{self, SplitHandle};

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

/// Build the child stagent argv forwarded to the spawned review session.
pub fn build_spawn_argv(opts: &SpawnOptions) -> Vec<String> {
    let mut cmd = Vec::new();

    let stagent_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "stagent".to_string());

    cmd.push(stagent_exe);

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

/// Spawn stagent in the preferred review environment and wait for it to complete.
pub fn spawn_in_split(opts: &SpawnOptions) -> Result<()> {
    let argv = build_spawn_argv(opts);

    if let Some(handle) = mux::open_command_in_preferred_split(&argv)? {
        wait_for_handle(&handle)?;
        return Ok(());
    }

    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .status()
        .context("Failed to run stagent in the current terminal")?;

    if !status.success() {
        bail!("stagent exited with status {}", status);
    }

    Ok(())
}

/// Block until the given split-backed review session closes.
fn wait_for_handle(handle: &SplitHandle) -> Result<()> {
    if mux::poll_handle_close(
        handle,
        MAX_SPAWN_POLL_ITERATIONS,
        Duration::from_millis(500),
    ) {
        return Ok(());
    }

    bail!("Timed out waiting for spawned stagent session to close");
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
    fn test_build_spawn_argv_basic() {
        let opts = default_opts();
        let cmd = build_spawn_argv(&opts);

        assert!(!cmd.is_empty(), "argv should contain the child executable");
        assert!(
            !cmd.contains(&"--spawn".to_string()),
            "Command should not contain --spawn"
        );
    }

    #[test]
    fn test_build_spawn_argv_with_output() {
        let opts = SpawnOptions {
            output: Some(PathBuf::from("/tmp/feedback.diff")),
            ..default_opts()
        };
        let cmd = build_spawn_argv(&opts);

        assert!(cmd.contains(&"--output".to_string()));
        assert!(cmd.contains(&"/tmp/feedback.diff".to_string()));
    }

    #[test]
    fn test_build_spawn_argv_with_files() {
        let opts = SpawnOptions {
            files: Some("*.rs".to_string()),
            ..default_opts()
        };
        let cmd = build_spawn_argv(&opts);

        assert!(cmd.contains(&"--files".to_string()));
        assert!(cmd.contains(&"*.rs".to_string()));
    }

    #[test]
    fn test_build_spawn_argv_with_theme() {
        let opts = SpawnOptions {
            theme: "dark".to_string(),
            ..default_opts()
        };
        let cmd = build_spawn_argv(&opts);

        assert!(cmd.contains(&"--theme".to_string()));
        assert!(cmd.contains(&"dark".to_string()));
    }

    #[test]
    fn test_build_spawn_argv_default_theme_not_included() {
        let opts = default_opts();
        let cmd = build_spawn_argv(&opts);

        assert!(!cmd.contains(&"--theme".to_string()));
    }

    #[test]
    fn test_build_spawn_argv_with_no_stage() {
        let opts = SpawnOptions {
            no_stage: true,
            ..default_opts()
        };
        let cmd = build_spawn_argv(&opts);

        assert!(cmd.contains(&"--no-stage".to_string()));
    }

    #[test]
    fn test_build_spawn_argv_with_context_lines() {
        let opts = SpawnOptions {
            context_lines: 10,
            ..default_opts()
        };
        let cmd = build_spawn_argv(&opts);

        assert!(cmd.contains(&"--context-lines".to_string()));
        assert!(cmd.contains(&"10".to_string()));
    }

    #[test]
    fn test_build_spawn_argv_default_context_lines_not_included() {
        let opts = default_opts();
        let cmd = build_spawn_argv(&opts);

        assert!(!cmd.contains(&"--context-lines".to_string()));
    }

    #[test]
    fn test_build_spawn_argv_all_options() {
        let opts = SpawnOptions {
            output: Some(PathBuf::from("/tmp/out.diff")),
            files: Some("src/*.rs".to_string()),
            theme: "monokai".to_string(),
            context_lines: 10,
            no_stage: true,
        };
        let cmd = build_spawn_argv(&opts);

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
