use ratatui::style::{Color, Modifier, Style};
use std::sync::OnceLock;

// --- Theme infrastructure ---

/// Which color variant is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeVariant {
    Dark,
    Light,
}

/// All configurable colors for the TUI.
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub variant: ThemeVariant,

    pub added_bg: Color,
    pub added_fg: Color,

    pub removed_bg: Color,
    pub removed_fg: Color,

    pub removed_dim_fg: Color,
    pub removed_dim_bg: Color,

    pub context_fg: Color,

    pub hunk_header_fg: Color,
    pub file_header_fg: Color,

    pub selected_bg: Color,
    pub selected_fg: Color,

    pub status_staged_fg: Color,
    pub status_skipped_fg: Color,
    pub status_pending_fg: Color,
    pub status_edited_fg: Color,
    pub status_commented_fg: Color,

    pub status_bar_bg: Color,
    pub status_bar_fg: Color,

    pub border_focused: Color,
    pub border_unfocused: Color,

    /// The syntect theme name to use for syntax highlighting.
    pub syntect_theme: &'static str,
}

impl ThemeColors {
    /// Dark theme (diff-so-fancy inspired, for dark terminal backgrounds).
    pub fn dark() -> Self {
        Self {
            variant: ThemeVariant::Dark,

            added_bg: Color::Rgb(0, 60, 0),
            added_fg: Color::Green,

            removed_bg: Color::Rgb(80, 0, 0),
            removed_fg: Color::Red,

            removed_dim_fg: Color::Red,
            removed_dim_bg: Color::Rgb(40, 0, 0),

            context_fg: Color::Gray,

            hunk_header_fg: Color::Cyan,
            file_header_fg: Color::Yellow,

            selected_bg: Color::Rgb(40, 40, 80),
            selected_fg: Color::White,

            status_staged_fg: Color::Green,
            status_skipped_fg: Color::DarkGray,
            status_pending_fg: Color::Yellow,
            status_edited_fg: Color::Cyan,
            status_commented_fg: Color::Magenta,

            status_bar_bg: Color::Rgb(30, 30, 30),
            status_bar_fg: Color::White,

            border_focused: Color::Cyan,
            border_unfocused: Color::DarkGray,

            syntect_theme: "base16-ocean.dark",
        }
    }

    /// Light theme (for light terminal backgrounds).
    pub fn light() -> Self {
        Self {
            variant: ThemeVariant::Light,

            added_bg: Color::Rgb(210, 255, 210),
            added_fg: Color::Rgb(0, 120, 0),

            removed_bg: Color::Rgb(255, 210, 210),
            removed_fg: Color::Rgb(180, 0, 0),

            removed_dim_fg: Color::Rgb(180, 0, 0),
            removed_dim_bg: Color::Rgb(255, 225, 225),

            context_fg: Color::DarkGray,

            hunk_header_fg: Color::Rgb(0, 130, 130),
            file_header_fg: Color::Rgb(150, 100, 0),

            selected_bg: Color::Rgb(200, 210, 240),
            selected_fg: Color::Black,

            status_staged_fg: Color::Rgb(0, 140, 0),
            status_skipped_fg: Color::Gray,
            status_pending_fg: Color::Rgb(180, 130, 0),
            status_edited_fg: Color::Rgb(0, 130, 130),
            status_commented_fg: Color::Rgb(160, 0, 160),

            status_bar_bg: Color::Rgb(225, 225, 225),
            status_bar_fg: Color::Black,

            border_focused: Color::Rgb(0, 130, 130),
            border_unfocused: Color::Gray,

            syntect_theme: "InspiredGitHub",
        }
    }
}

/// Global active theme, initialised once at startup.
static THEME: OnceLock<ThemeColors> = OnceLock::new();

/// Initialise the global theme. Call once from main before the TUI starts.
/// Accepts the `--theme` CLI value: "dark", "light", or "auto"/"default".
pub fn init(name: &str) {
    let colors = match name {
        "light" => ThemeColors::light(),
        "dark" => ThemeColors::dark(),
        _ => {
            // Auto-detect via COLORFGBG (set by many terminals).
            // Format: "fg;bg" — bg >= 8 usually means light background.
            if let Ok(val) = std::env::var("COLORFGBG") {
                if let Some(bg) = val.rsplit(';').next().and_then(|s| s.parse::<u8>().ok()) {
                    if bg >= 8 {
                        ThemeColors::light()
                    } else {
                        ThemeColors::dark()
                    }
                } else {
                    ThemeColors::dark()
                }
            } else {
                ThemeColors::dark()
            }
        }
    };
    let _ = THEME.set(colors);
}

/// Return the active theme. Falls back to dark if `init()` was not called.
pub fn current() -> &'static ThemeColors {
    THEME.get_or_init(ThemeColors::dark)
}

// --- Accessor functions (replace the old constants) ---

pub fn added_bg() -> Color {
    current().added_bg
}
pub fn added_fg() -> Color {
    current().added_fg
}
pub fn removed_bg() -> Color {
    current().removed_bg
}
pub fn removed_fg() -> Color {
    current().removed_fg
}
pub fn removed_dim_fg() -> Color {
    current().removed_dim_fg
}
pub fn removed_dim_bg() -> Color {
    current().removed_dim_bg
}
pub fn context_fg() -> Color {
    current().context_fg
}
pub fn hunk_header_fg() -> Color {
    current().hunk_header_fg
}
pub fn file_header_fg() -> Color {
    current().file_header_fg
}
pub fn selected_bg() -> Color {
    current().selected_bg
}
pub fn selected_fg() -> Color {
    current().selected_fg
}
pub fn status_staged_fg() -> Color {
    current().status_staged_fg
}
pub fn status_skipped_fg() -> Color {
    current().status_skipped_fg
}
pub fn status_pending_fg() -> Color {
    current().status_pending_fg
}
pub fn status_edited_fg() -> Color {
    current().status_edited_fg
}
pub fn status_commented_fg() -> Color {
    current().status_commented_fg
}
pub fn status_bar_bg() -> Color {
    current().status_bar_bg
}
pub fn status_bar_fg() -> Color {
    current().status_bar_fg
}
pub fn border_focused() -> Color {
    current().border_focused
}
pub fn border_unfocused() -> Color {
    current().border_unfocused
}

/// Name of the syntect theme to use for syntax highlighting.
pub fn syntect_theme() -> &'static str {
    current().syntect_theme
}

// --- Style helpers ---

pub fn added_style() -> Style {
    Style::default().fg(added_fg()).bg(added_bg())
}

pub fn removed_style() -> Style {
    Style::default()
        .fg(removed_dim_fg())
        .bg(removed_dim_bg())
        .add_modifier(Modifier::DIM)
}

pub fn context_style() -> Style {
    Style::default().fg(context_fg())
}

pub fn hunk_header_style() -> Style {
    Style::default()
        .fg(hunk_header_fg())
        .add_modifier(Modifier::BOLD)
}

pub fn file_header_style() -> Style {
    Style::default()
        .fg(file_header_fg())
        .add_modifier(Modifier::BOLD)
}

pub fn selected_style() -> Style {
    Style::default().fg(selected_fg()).bg(selected_bg())
}

pub fn status_bar_style() -> Style {
    Style::default().fg(status_bar_fg()).bg(status_bar_bg())
}

pub fn border_focused_style() -> Style {
    Style::default().fg(border_focused())
}

pub fn border_unfocused_style() -> Style {
    Style::default().fg(border_unfocused())
}
