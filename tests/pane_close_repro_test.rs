/// Reproduction test for the bug: after `:wq` in vim, the TUI stays stuck on
/// "Editing in split pane..." because `wait_for_pane_close` never detects the
/// pane has closed.
///
/// Root cause: When a tmux pane's process exits, tmux destroys the pane immediately
/// (unless `remain-on-exit` is on). `tmux display-message -t <pane_id> -p '#{pane_dead}'`
/// on a non-existent pane returns empty string with exit code 0 on tmux 3.x,
/// so our `result == "1"` check never matches and `!output.status.success()` is false.
///
/// This test requires tmux (runs inside the current tmux session).
#[test]
#[ignore]
fn test_pane_close_detected_after_process_exits() {
    use stagent::editor::{open_editor, wait_for_pane_close};

    // Override editor to `true` which exits immediately
    let orig_visual = std::env::var("VISUAL").ok();
    let orig_editor = std::env::var("EDITOR").ok();
    std::env::set_var("VISUAL", "true");

    let tmpfile = tempfile::NamedTempFile::new().expect("create tmpfile");
    let path = tmpfile.path().to_str().unwrap().to_string();

    let pane_id = open_editor(&path).expect("should open tmux split");
    assert!(
        pane_id.starts_with('%'),
        "pane_id should start with %, got: {}",
        pane_id
    );

    // `true` exits instantly. Give tmux 1 second to destroy the pane.
    std::thread::sleep(std::time::Duration::from_secs(1));

    let rx = wait_for_pane_close(pane_id);

    // BUG: This should complete within 3 seconds but previously hung forever
    // because pane_dead detection was broken.
    let result = rx.recv_timeout(std::time::Duration::from_secs(5));
    assert!(
        result.is_ok(),
        "wait_for_pane_close should detect closed pane within 5 seconds"
    );

    // Restore env
    match orig_visual {
        Some(v) => std::env::set_var("VISUAL", v),
        None => std::env::remove_var("VISUAL"),
    }
    match orig_editor {
        Some(v) => std::env::set_var("EDITOR", v),
        None => std::env::remove_var("EDITOR"),
    }
}
