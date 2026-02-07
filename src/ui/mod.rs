pub mod diff_view;
pub mod file_list;
pub mod status_bar;
pub mod theme;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::App;
use crate::highlight::Highlighter;

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

    // Store file list area for mouse click mapping
    app.file_list_area = file_list_area;

    // Render file list
    file_list::render(
        frame,
        file_list_area,
        &app.files,
        app.selected_file,
        app.focus == crate::types::FocusPanel::FileList,
    );

    // Render diff view
    let current_file = app.current_file();
    diff_view::render(
        frame,
        diff_view_area,
        current_file,
        app.selected_hunk,
        app.scroll_offset,
        app.focus == crate::types::FocusPanel::DiffView,
        highlighter,
    );

    // Render status bar
    status_bar::render(
        frame,
        status_area,
        &app.files,
        app.mode,
        app.message.as_deref(),
    );
}
