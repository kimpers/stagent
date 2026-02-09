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
fn test_patch_flag_parsed() {
    let output = run_binary(&["-p"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should fail due to tmux, not a parse error
    assert!(
        stderr.to_lowercase().contains("tmux"),
        "Should fail due to tmux, not bad arg parse, got: {}",
        stderr
    );
}

#[test]
fn test_patch_and_spawn_rejected() {
    // Must set TMUX so we get past the tmux check and actually hit the
    // --patch + --spawn conflict validation at main.rs:49-51.
    let output = Command::new(binary_path())
        .args(["-p", "--spawn"])
        .env("TMUX", "/tmp/tmux-fake/default,12345,0")
        .output()
        .expect("Failed to execute binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "Should fail with --patch + --spawn, got: {}",
        stderr
    );
    assert!(
        stderr.contains("--patch and --spawn cannot be used together"),
        "Should report the flag conflict, got: {}",
        stderr
    );
}

#[test]
fn test_patch_empty_diff_from_stdin() {
    // Pipe an empty string to stagent -p. Should exit cleanly with "No changes to review."
    use std::process::Stdio;
    let mut child = Command::new(binary_path())
        .args(["-p"])
        .env("TMUX", "/tmp/tmux-fake/default,12345,0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn binary");

    // Write empty diff to stdin and close it
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("Failed to wait for binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Should succeed with empty piped input. stderr: {}, stdout: {}",
        stderr,
        stdout
    );
    assert!(
        stdout.contains("No changes to review"),
        "Should report no changes for empty diff, got stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

#[test]
fn test_patch_reads_piped_diff() {
    // Pipe a real unified diff to stagent -p. Since the test subprocess has no
    // controlling terminal (/dev/tty), the TUI can't fully start. But with the
    // use-dev-tty feature enabled, the error should be about /dev/tty access
    // (expected in headless test), NOT "Failed to initialize input reader" which
    // would indicate crossterm tried to read from piped stdin.
    //
    // In a real tmux session, /dev/tty IS available and this works correctly.
    // This test just ensures the stdin pipe isn't the failure point.
    use std::io::Write;
    use std::process::Stdio;

    let diff = "\
diff --git a/test.rs b/test.rs
--- a/test.rs
+++ b/test.rs
@@ -1,3 +1,3 @@
 fn main() {
-    println!(\"hello\");
+    println!(\"hello world\");
 }
";

    let mut child = Command::new(binary_path())
        .args(["-p"])
        .env("TMUX", "/tmp/tmux-fake/default,12345,0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn binary");

    // Write diff to stdin and close it
    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(diff.as_bytes())
            .expect("Failed to write to stdin");
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("Failed to wait for binary");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The key assertion: it should NOT fail with "Failed to initialize input reader".
    // That error means crossterm tried to read keyboard events from piped stdin.
    // With use-dev-tty, crossterm reads from /dev/tty instead, which will fail in
    // this headless test with "Device not configured" but works in real tmux sessions.
    assert!(
        !stderr.contains("Failed to initialize input reader"),
        "Should not fail with input reader error when using -p. stderr: {}",
        stderr
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
