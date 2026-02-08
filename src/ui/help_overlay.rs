use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Render a centered help overlay listing all keybindings.
pub fn render(frame: &mut Frame, area: Rect) {
    // Size: 60 wide, 22 tall, centered
    let width = 60u16.min(area.width.saturating_sub(4));
    let height = 22u16.min(area.height.saturating_sub(2));
    let overlay = centered_rect(width, height, area);

    // Clear the area behind the overlay
    frame.render_widget(Clear, overlay);

    let title_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let section_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let footer_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC);

    // Inner width is overlay width minus 2 for borders
    let inner_width = width.saturating_sub(2) as usize;

    let lines = vec![
        centered_line("Keyboard Shortcuts", title_style, inner_width),
        Line::from(""),
        centered_line("── Navigation ──", section_style, inner_width),
        key_line(
            "j / k",
            "Scroll diff (DiffView) / Navigate files (FileList)",
            key_style,
            desc_style,
        ),
        key_line(
            "J / K  { / }",
            "Next / previous hunk",
            key_style,
            desc_style,
        ),
        key_line("H / L", "Previous / next file", key_style, desc_style),
        key_line(
            "h / l",
            "Focus file list / diff view",
            key_style,
            desc_style,
        ),
        key_line("gg", "Scroll to top", key_style, desc_style),
        key_line("G", "Scroll to bottom", key_style, desc_style),
        key_line(
            "Ctrl+d / Ctrl+u",
            "Half-page down / up",
            key_style,
            desc_style,
        ),
        key_line(
            "Ctrl+f / Ctrl+b",
            "Full-page down / up",
            key_style,
            desc_style,
        ),
        key_line("Tab", "Toggle panel focus", key_style, desc_style),
        key_line("↑ / ↓", "Navigate hunks/files", key_style, desc_style),
        Line::from(""),
        centered_line("── Actions ──", section_style, inner_width),
        key_line("y", "Stage hunk", key_style, desc_style),
        key_line("n", "Skip hunk", key_style, desc_style),
        key_line("s", "Split hunk", key_style, desc_style),
        key_line("e", "Edit hunk", key_style, desc_style),
        key_line("c", "Comment on hunk", key_style, desc_style),
        key_line("q", "Quit", key_style, desc_style),
        Line::from(""),
        centered_line("Press any key to start", footer_style, inner_width),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ")
        .title_style(title_style);

    let paragraph = Paragraph::new(lines).block(block);

    frame.render_widget(paragraph, overlay);
}

fn key_line<'a>(key: &'a str, desc: &'a str, key_style: Style, desc_style: Style) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {:<18}", key), key_style),
        Span::styled(desc, desc_style),
    ])
}

fn centered_line(text: &str, style: Style, width: usize) -> Line<'static> {
    let text_len = text.chars().count();
    let padding = width.saturating_sub(text_len) / 2;
    let padded = format!("{:>width$}", text, width = padding + text_len);
    Line::from(Span::styled(padded, style))
}

/// Create a centered rect of given width and height within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1]);

    horizontal[1]
}
