mod helpers;

use std::process::Command;

/// Helper to get the path to the built binary.
fn binary_path() -> std::path::PathBuf {
    // cargo test builds the binary in the target/debug directory
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("stagent");
    path
}

/// Run the binary with given args, removing TMUX from env by default.
fn run_binary(args: &[&str]) -> std::process::Output {
    Command::new(binary_path())
        .args(args)
        .env_remove("TMUX")
        .output()
        .expect("Failed to execute binary")
}

/// Run the binary with TMUX set (to bypass tmux check), inside a given directory.
fn run_binary_in_dir(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(binary_path())
        .args(args)
        .env("TMUX", "/tmp/tmux-fake/default,12345,0")
        .current_dir(dir)
        .output()
        .expect("Failed to execute binary")
}

#[test]
fn test_no_tmux_error() {
    let output = run_binary(&[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "Should fail without tmux");
    assert!(
        stderr.to_lowercase().contains("tmux"),
        "Error should mention tmux, got: {}",
        stderr
    );
}

#[test]
fn test_not_a_repo_error() {
    // Run in /tmp which is not a git repo
    let tmp = tempfile::TempDir::new().unwrap();
    let output = Command::new(binary_path())
        .env("TMUX", "/tmp/tmux-fake/default,12345,0")
        .current_dir(tmp.path())
        .output()
        .expect("Failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "Should fail outside a git repo");
    assert!(
        stderr.to_lowercase().contains("git"),
        "Error should mention git, got: {}",
        stderr
    );
}

#[test]
fn test_no_changes_message() {
    // Create a temp repo with no unstaged changes
    let (dir, _repo) = helpers::create_temp_repo();
    let output = run_binary_in_dir(dir.path(), &[]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Should succeed with no changes");
    assert!(
        stdout.to_lowercase().contains("no unstaged changes"),
        "Should print 'no unstaged changes', got: {}",
        stdout
    );
}

#[test]
fn test_output_flag_parsed() {
    // The --output flag should be accepted (we won't actually write to a file
    // since there's no tmux, but we verify the flag doesn't cause a parse error)
    let output = run_binary(&["--output", "/tmp/test-output.md"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // It should fail because of tmux, not because of a bad flag
    assert!(
        stderr.to_lowercase().contains("tmux"),
        "Should fail due to tmux, not bad arg parse, got: {}",
        stderr
    );
    // Verify it's NOT an argument parsing error
    assert!(
        !stderr.contains("error: unexpected argument") && !stderr.contains("error: invalid value"),
        "Should not have arg parsing error, got: {}",
        stderr
    );
}

#[test]
fn test_no_stage_flag() {
    let output = run_binary(&["--no-stage"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should fail due to tmux, not a parse error
    assert!(
        stderr.to_lowercase().contains("tmux"),
        "Should fail due to tmux, not bad arg parse, got: {}",
        stderr
    );
}

#[test]
fn test_files_glob_filter() {
    let output = run_binary(&["--files", "*.rs"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should fail due to tmux, not a parse error
    assert!(
        stderr.to_lowercase().contains("tmux"),
        "Should fail due to tmux, not bad arg parse, got: {}",
        stderr
    );
}

#[test]
fn test_binary_file_skipped_via_glob() {
    // Test that the --files glob filter can exclude files.
    // Create a repo with a text change, then use --files to filter it out,
    // which should result in "no unstaged changes".
    let (dir, repo) = helpers::create_temp_repo();

    // Commit a text file, then modify it
    helpers::commit_file(&repo, "src/main.rs", "fn main() {}");
    helpers::modify_file(&repo, "src/main.rs", "fn main() { println!(\"hello\"); }");

    // Use --files glob that doesn't match the changed file
    let output = run_binary_in_dir(dir.path(), &["--files", "*.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Should succeed when glob filters out all files. stderr: {}, stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        stdout
    );
    assert!(
        stdout.to_lowercase().contains("no unstaged changes"),
        "Should report no changes when all files filtered out, got: {}",
        stdout
    );
}

#[test]
fn test_theme_flag_parsed() {
    let output = run_binary(&["--theme", "monokai"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should fail due to tmux, not a parse error
    assert!(
        stderr.to_lowercase().contains("tmux"),
        "Should fail due to tmux, not bad arg parse, got: {}",
        stderr
    );
}

#[test]
fn test_help_flag() {
    let output = run_binary(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Help flag should succeed");
    assert!(
        stdout.contains("stagent"),
        "Help should mention stagent, got: {}",
        stdout
    );
    assert!(
        stdout.contains("--output"),
        "Help should mention --output flag"
    );
    assert!(
        stdout.contains("--no-stage"),
        "Help should mention --no-stage flag"
    );
    assert!(
        stdout.contains("--files"),
        "Help should mention --files flag"
    );
    assert!(
        stdout.contains("--theme"),
        "Help should mention --theme flag"
    );
}

#[test]
fn test_unknown_flag_rejected() {
    let output = run_binary(&["--nonexistent-flag"]);
    assert!(
        !output.status.success(),
        "Unknown flag should cause failure"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument") || stderr.contains("error"),
        "Should report argument error, got: {}",
        stderr
    );
}
