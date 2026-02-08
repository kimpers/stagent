//! Tests for the spawn module (--spawn flag functionality).

use stagent::spawn::{SpawnOptions, build_spawn_command};
use std::path::PathBuf;

fn default_opts() -> SpawnOptions {
    SpawnOptions {
        output: None,
        files: None,
        theme: "default".to_string(),
        context_lines: stagent::feedback::DEFAULT_CONTEXT_LINES,
        no_stage: false,
    }
}

#[test]
fn test_spawn_command_format() {
    let opts = default_opts();
    let cmd = build_spawn_command(&opts);

    // Verify basic structure
    assert_eq!(cmd[0], "tmux");
    assert_eq!(cmd[1], "split-window");

    // Find the position of key flags
    let h_pos = cmd.iter().position(|s| s == "-h");
    let p_pos = cmd.iter().position(|s| s == "-p");
    let fifty_pos = cmd.iter().position(|s| s == "50");
    let big_p_pos = cmd.iter().position(|s| s == "-P");
    let f_pos = cmd.iter().position(|s| s == "-F");
    let pane_id_pos = cmd.iter().position(|s| s == "#{pane_id}");
    let separator_pos = cmd.iter().position(|s| s == "--");

    // All required flags should be present
    assert!(h_pos.is_some(), "-h flag missing");
    assert!(p_pos.is_some(), "-p flag missing");
    assert!(fifty_pos.is_some(), "50 value missing");
    assert!(big_p_pos.is_some(), "-P flag missing");
    assert!(f_pos.is_some(), "-F flag missing");
    assert!(pane_id_pos.is_some(), "#{{pane_id}} format missing");
    assert!(separator_pos.is_some(), "-- separator missing");

    // -p should be followed by 50
    assert_eq!(p_pos.unwrap() + 1, fifty_pos.unwrap());

    // -F should be followed by #{pane_id}
    assert_eq!(f_pos.unwrap() + 1, pane_id_pos.unwrap());

    // The executable should come after --
    assert!(cmd.len() > separator_pos.unwrap() + 1);
}

#[test]
fn test_spawn_command_no_spawn_flag() {
    let opts = SpawnOptions {
        output: Some(PathBuf::from("/tmp/test.diff")),
        files: Some("*.rs".to_string()),
        theme: "dark".to_string(),
        context_lines: 5,
        no_stage: true,
    };
    let cmd = build_spawn_command(&opts);

    // Should NOT contain --spawn (would cause infinite recursion)
    assert!(
        !cmd.iter().any(|s| s == "--spawn"),
        "Command should not contain --spawn flag"
    );
}

#[test]
fn test_spawn_command_forwards_output() {
    let opts = SpawnOptions {
        output: Some(PathBuf::from("/tmp/feedback.diff")),
        ..default_opts()
    };
    let cmd = build_spawn_command(&opts);

    let output_pos = cmd.iter().position(|s| s == "--output");
    assert!(output_pos.is_some(), "--output flag should be present");
    assert_eq!(
        cmd[output_pos.unwrap() + 1],
        "/tmp/feedback.diff",
        "output path should follow --output"
    );
}

#[test]
fn test_spawn_command_forwards_files_filter() {
    let opts = SpawnOptions {
        files: Some("src/**/*.rs".to_string()),
        ..default_opts()
    };
    let cmd = build_spawn_command(&opts);

    let files_pos = cmd.iter().position(|s| s == "--files");
    assert!(files_pos.is_some(), "--files flag should be present");
    assert_eq!(
        cmd[files_pos.unwrap() + 1],
        "src/**/*.rs",
        "glob pattern should follow --files"
    );
}

#[test]
fn test_spawn_command_forwards_no_stage() {
    let opts = SpawnOptions {
        no_stage: true,
        ..default_opts()
    };
    let cmd = build_spawn_command(&opts);

    assert!(
        cmd.iter().any(|s| s == "--no-stage"),
        "--no-stage flag should be present"
    );
}

// ---------------------------------------------------------------------------
// Integration tests (require tmux, marked #[ignore])
// ---------------------------------------------------------------------------

/// Test that spawning in tmux works and completes.
///
/// This test:
/// 1. Creates a temp git repo with unstaged changes
/// 2. Runs stagent --spawn with a short-lived command
/// 3. Sends 'q' to quit immediately
/// 4. Verifies the spawn completes
#[test]
#[ignore]
fn test_spawn_in_tmux() {
    use std::process::Command;

    // Skip if not in tmux
    if std::env::var("TMUX").is_err() {
        eprintln!("Skipping test: not in tmux session");
        return;
    }

    // Create a temp directory with a git repo
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_path = temp_dir.path();

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .expect("git init");

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(repo_path)
        .output()
        .expect("git config email");

    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(repo_path)
        .output()
        .expect("git config name");

    // Create and commit an initial file
    std::fs::write(repo_path.join("test.txt"), "original\n").expect("write initial file");
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(repo_path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(repo_path)
        .output()
        .expect("git commit");

    // Make an unstaged change
    std::fs::write(repo_path.join("test.txt"), "modified\n").expect("write modified file");

    // Create output file path
    let output_file = repo_path.join("feedback.diff");

    // Get the stagent binary path
    let stagent_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("stagent")))
        .unwrap_or_else(|| PathBuf::from("stagent"));

    // Spawn stagent with --spawn in the test repo
    // Use a separate thread to send 'q' after a short delay
    let output_path = output_file.clone();
    let repo_path_clone = repo_path.to_path_buf();

    std::thread::spawn(move || {
        // Wait for stagent to start
        std::thread::sleep(std::time::Duration::from_millis(1500));

        // Send 'q' to the most recently created pane
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", "{last}", "q"])
            .current_dir(&repo_path_clone)
            .output();
    });

    let result = Command::new(&stagent_exe)
        .args(["--spawn", "--output", output_path.to_str().unwrap()])
        .current_dir(repo_path)
        .output();

    match result {
        Ok(output) => {
            // The command should complete (even if with an error about no changes
            // being staged)
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("stdout: {}", stdout);
            eprintln!("stderr: {}", stderr);
        }
        Err(e) => {
            // If the binary doesn't exist, skip the test
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("Skipping test: stagent binary not found");
                return;
            }
            panic!("spawn failed: {}", e);
        }
    }
}
