use ratatui::style::{Color, Modifier, Style};

// --- Diff-so-fancy inspired theme ---

/// Background for added lines
pub const ADDED_BG: Color = Color::Rgb(0, 60, 0);
/// Foreground for added line prefix (+)
pub const ADDED_FG: Color = Color::Green;

/// Background for removed lines
pub const REMOVED_BG: Color = Color::Rgb(80, 0, 0);
/// Foreground for removed line prefix (-)
pub const REMOVED_FG: Color = Color::Red;

/// Dimmed removed line (for syntax-highlighted view)
pub const REMOVED_DIM_FG: Color = Color::Red;
pub const REMOVED_DIM_BG: Color = Color::Rgb(40, 0, 0);

/// Context lines (unchanged)
pub const CONTEXT_FG: Color = Color::Gray;

/// Hunk header (@@ ... @@)
pub const HUNK_HEADER_FG: Color = Color::Cyan;

/// File path in header
pub const FILE_HEADER_FG: Color = Color::Yellow;

/// Selected item in file list
pub const SELECTED_BG: Color = Color::Rgb(40, 40, 80);
pub const SELECTED_FG: Color = Color::White;

/// Status indicators
pub const STATUS_STAGED_FG: Color = Color::Green;
pub const STATUS_SKIPPED_FG: Color = Color::DarkGray;
pub const STATUS_PENDING_FG: Color = Color::Yellow;
pub const STATUS_EDITED_FG: Color = Color::Cyan;
pub const STATUS_COMMENTED_FG: Color = Color::Magenta;

/// Status bar
pub const STATUS_BAR_BG: Color = Color::Rgb(30, 30, 30);
pub const STATUS_BAR_FG: Color = Color::White;

/// Panel borders
pub const BORDER_FOCUSED: Color = Color::Cyan;
pub const BORDER_UNFOCUSED: Color = Color::DarkGray;

/// Helper to create a style for added lines
pub fn added_style() -> Style {
    Style::default().fg(ADDED_FG).bg(ADDED_BG)
}

/// Helper to create a style for removed lines
pub fn removed_style() -> Style {
    Style::default()
        .fg(REMOVED_DIM_FG)
        .bg(REMOVED_DIM_BG)
        .add_modifier(Modifier::DIM)
}

/// Helper for context lines
pub fn context_style() -> Style {
    Style::default().fg(CONTEXT_FG)
}

/// Helper for hunk headers
pub fn hunk_header_style() -> Style {
    Style::default()
        .fg(HUNK_HEADER_FG)
        .add_modifier(Modifier::BOLD)
}

/// Helper for file headers
pub fn file_header_style() -> Style {
    Style::default()
        .fg(FILE_HEADER_FG)
        .add_modifier(Modifier::BOLD)
}

/// Helper for selected items
pub fn selected_style() -> Style {
    Style::default().fg(SELECTED_FG).bg(SELECTED_BG)
}

/// Helper for the status bar
pub fn status_bar_style() -> Style {
    Style::default().fg(STATUS_BAR_FG).bg(STATUS_BAR_BG)
}

/// Helper for focused borders
pub fn border_focused_style() -> Style {
    Style::default().fg(BORDER_FOCUSED)
}

/// Helper for unfocused borders
pub fn border_unfocused_style() -> Style {
    Style::default().fg(BORDER_UNFOCUSED)
}
