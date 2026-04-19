//! Tests for the spawn module (--spawn flag functionality).

use stagent::spawn::{SpawnOptions, build_spawn_argv};
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
    let cmd = build_spawn_argv(&opts);

    assert!(!cmd.is_empty(), "child argv should include the executable");
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
    let cmd = build_spawn_argv(&opts);

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
    let cmd = build_spawn_argv(&opts);

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
    let cmd = build_spawn_argv(&opts);

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
    let cmd = build_spawn_argv(&opts);

    assert!(
        cmd.iter().any(|s| s == "--no-stage"),
        "--no-stage flag should be present"
    );
}

// ---------------------------------------------------------------------------
// Integration tests (require tmux or cmux, marked #[ignore])
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_spawn_in_mux() {
    use std::process::Command;

    let in_tmux = std::env::var("TMUX").is_ok();
    let in_cmux = Command::new("cmux")
        .arg("identify")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !in_tmux && !in_cmux {
        eprintln!("Skipping test: not in tmux or cmux");
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
        std::thread::sleep(std::time::Duration::from_millis(1500));

        if std::env::var("TMUX").is_ok() {
            let _ = Command::new("tmux")
                .args(["send-keys", "-t", "{last}", "q"])
                .current_dir(&repo_path_clone)
                .output();
        } else {
            let _ = Command::new("cmux")
                .args(["send", "q"])
                .current_dir(&repo_path_clone)
                .output();
            let _ = Command::new("cmux")
                .args(["send-key", "Enter"])
                .current_dir(&repo_path_clone)
                .output();
        }
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
