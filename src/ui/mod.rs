pub mod diff_view;
pub mod file_list;
pub mod help_overlay;
pub mod status_bar;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

use crate::app::App;
use crate::highlight::Highlighter;
use crate::types::AppMode;

/// Render the full TUI layout.
pub fn render(frame: &mut Frame, app: &mut App, highlighter: &Highlighter) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main content area
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    let main_area = chunks[0];
    let status_area = chunks[1];

    // Split main area into file list + diff view
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // File list
            Constraint::Percentage(75), // Diff view
        ])
        .split(main_area);

    let file_list_area = main_chunks[0];
    let diff_view_area = main_chunks[1];

    // Store areas for mouse click mapping and page scroll calculations
    app.file_list_area = file_list_area;
    app.diff_view_area = diff_view_area;

    // Render file list
    file_list::render(
        frame,
        file_list_area,
        &app.files,
        app.selected_file,
        app.focus == crate::types::FocusPanel::FileList,
    );

    // Rebuild highlight cache if needed
    let needs_rebuild = match &app.highlight_cache {
        Some((idx, _)) => *idx != app.selected_file,
        None => true,
    };
    if needs_rebuild && let Some(file) = app.current_file() {
        let path_str = file.path.to_string_lossy().to_string();
        let lines = highlighter.highlight_file_lines(&path_str, &file.hunks);
        app.highlight_cache = Some((app.selected_file, lines));
    }
    let cached = app.highlight_cache.as_ref().map(|(_, lines)| lines);

    // Render diff view
    let current_file = app.current_file();
    diff_view::render(
        frame,
        diff_view_area,
        current_file,
        app.selected_hunk,
        app.scroll_offset,
        app.focus == crate::types::FocusPanel::DiffView,
        cached,
    );

    // Render status bar
    status_bar::render(
        frame,
        status_area,
        &app.files,
        app.mode,
        app.message.as_deref(),
    );

    // Render help overlay on top of everything
    if app.mode == AppMode::Help {
        help_overlay::render(frame, frame.area());
    }
}
