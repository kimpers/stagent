use ratatui::Terminal;
use ratatui::backend::TestBackend;

use stagent::app::App;
use stagent::highlight::Highlighter;
use stagent::types::*;
use stagent::ui;

/// Helper to set app to Browsing mode (since App::new starts in Help mode).
fn set_browsing(app: &mut App) {
    app.mode = AppMode::Browsing;
}

fn make_test_files() -> Vec<FileDiff> {
    vec![
        FileDiff {
            path: "src/main.rs".into(),
            hunks: vec![Hunk {
                header: "@@ -1,3 +1,4 @@".to_string(),
                lines: vec![
                    DiffLine {
                        kind: LineKind::Context,
                        content: "use std::io;\n".to_string(),
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                    },
                    DiffLine {
                        kind: LineKind::Removed,
                        content: "let x = 1;\n".to_string(),
                        old_lineno: Some(2),
                        new_lineno: None,
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        content: "let x = 42;\n".to_string(),
                        old_lineno: None,
                        new_lineno: Some(2),
                    },
                    DiffLine {
                        kind: LineKind::Context,
                        content: "println!(\"hello\");\n".to_string(),
                        old_lineno: Some(3),
                        new_lineno: Some(3),
                    },
                ],
                status: HunkStatus::Pending,
                old_start: 1,
                old_lines: 3,
                new_start: 1,
                new_lines: 4,
            }],
            status: DeltaStatus::Modified,
            is_binary: false,
        },
        FileDiff {
            path: "src/lib.rs".into(),
            hunks: vec![Hunk {
                header: "@@ -5,3 +5,3 @@".to_string(),
                lines: vec![
                    DiffLine {
                        kind: LineKind::Removed,
                        content: "old_fn()\n".to_string(),
                        old_lineno: Some(6),
                        new_lineno: None,
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        content: "new_fn()\n".to_string(),
                        old_lineno: None,
                        new_lineno: Some(6),
                    },
                ],
                status: HunkStatus::Pending,
                old_start: 5,
                old_lines: 3,
                new_start: 5,
                new_lines: 3,
            }],
            status: DeltaStatus::Modified,
            is_binary: false,
        },
    ]
}

/// Helper: render the UI into a TestBackend buffer and return the buffer content as a string.
fn render_to_string(width: u16, height: u16, app: &mut App) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let highlighter = Highlighter::new();

    terminal
        .draw(|frame| {
            ui::render(frame, app, &highlighter);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    let mut output = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn test_file_list_render() {
    let mut app = App::new(make_test_files(), false);
    set_browsing(&mut app);
    let output = render_to_string(80, 24, &mut app);

    // File names should appear in the rendered output
    assert!(
        output.contains("src/main.rs"),
        "Expected 'src/main.rs' in output:\n{}",
        output
    );
    assert!(
        output.contains("src/lib.rs"),
        "Expected 'src/lib.rs' in output:\n{}",
        output
    );
}

#[test]
fn test_diff_view_render() {
    let mut app = App::new(make_test_files(), false);
    set_browsing(&mut app);
    let output = render_to_string(100, 30, &mut app);

    // The diff view should show the hunk header
    assert!(
        output.contains("@@ -1,3 +1,4 @@"),
        "Expected hunk header in output:\n{}",
        output
    );

    // Should show specific content from the diff lines
    assert!(
        output.contains("let x = 42"),
        "Expected added line 'let x = 42' in output:\n{}",
        output
    );
    assert!(
        output.contains("let x = 1"),
        "Expected removed line 'let x = 1' in output:\n{}",
        output
    );
}

#[test]
fn test_status_bar_render() {
    let mut app = App::new(make_test_files(), false);
    set_browsing(&mut app);
    let output = render_to_string(120, 24, &mut app);

    // Status bar should contain keybinding hints
    assert!(
        output.contains("y:stage"),
        "Expected 'y:stage' in status bar:\n{}",
        output
    );
    assert!(
        output.contains("n:skip"),
        "Expected 'n:skip' in status bar:\n{}",
        output
    );
    assert!(
        output.contains("e:edit"),
        "Expected 'e:edit' in status bar:\n{}",
        output
    );
    assert!(
        output.contains("q:quit"),
        "Expected 'q:quit' in status bar:\n{}",
        output
    );
}

#[test]
fn test_layout_proportions() {
    let mut app = App::new(make_test_files(), false);
    set_browsing(&mut app);

    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let highlighter = Highlighter::new();

    terminal
        .draw(|frame| {
            ui::render(frame, &mut app, &highlighter);
        })
        .unwrap();

    // After rendering, app.file_list_area should be populated
    let fl_area = app.file_list_area;

    // File list should be roughly 25% of total width (100)
    // With integer rounding, 25% of 100 = 25
    assert!(
        fl_area.width >= 20 && fl_area.width <= 30,
        "File list width {} should be ~25% of 100",
        fl_area.width
    );

    // The remaining 75% should be for diff view
    let diff_width = 100 - fl_area.width;
    assert!(
        diff_width >= 70 && diff_width <= 80,
        "Diff view width {} should be ~75% of 100",
        diff_width
    );
}

// --- Help overlay integration tests ---

#[test]
fn test_help_overlay_render() {
    let mut app = App::new_with_help(make_test_files(), false, true);
    assert_eq!(app.mode, AppMode::Help);
    let output = render_to_string(100, 30, &mut app);

    // Overlay should contain keybinding info
    assert!(
        output.contains("Navigation"),
        "Expected 'Navigation' in help overlay:\n{}",
        output
    );
    assert!(
        output.contains("Stage hunk"),
        "Expected 'Stage hunk' in help overlay:\n{}",
        output
    );
}

#[test]
fn test_help_overlay_dismissed_shows_normal_ui() {
    let mut app = App::new_with_help(make_test_files(), false, true);
    // Dismiss help
    app.mode = AppMode::Browsing;
    let output = render_to_string(100, 30, &mut app);

    // Normal UI should be visible: diff hunk header
    assert!(
        output.contains("@@ -1,3 +1,4 @@"),
        "Expected hunk header after dismissing help:\n{}",
        output
    );
    // Help overlay text should not be present
    assert!(
        !output.contains("Press any key to start"),
        "Help overlay should not be visible after dismissing:\n{}",
        output
    );
}

#[test]
fn test_status_bar_shows_help_hint() {
    let mut app = App::new(make_test_files(), false);
    set_browsing(&mut app);
    let output = render_to_string(120, 24, &mut app);

    assert!(
        output.contains("?:help"),
        "Expected '?:help' in status bar:\n{}",
        output
    );
}
