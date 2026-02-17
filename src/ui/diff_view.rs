use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::types::{FileDiff, Hunk, HunkStatus, LineKind};
use crate::ui::theme;

/// Render the diff view panel showing hunks for the selected file.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    file: Option<&FileDiff>,
    selected_hunk: usize,
    scroll_offset: u32,
    focused: bool,
    highlighted_lines: Option<&Vec<Vec<Line<'static>>>>,
) {
    let border_style = if focused {
        theme::border_focused_style()
    } else {
        theme::border_unfocused_style()
    };

    let title = match file {
        Some(f) => format!(" {} ", f.path.display()),
        None => " No file selected ".to_string(),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let file = match file {
        Some(f) => f,
        None => {
            let paragraph = Paragraph::new("No unstaged changes to display.").block(block);
            frame.render_widget(paragraph, area);
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::new();

    for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
        let is_selected = hunk_idx == selected_hunk;

        // Hunk header line
        let header_style = if is_selected {
            theme::hunk_header_style().bg(theme::selected_bg())
        } else {
            theme::hunk_header_style()
        };

        let status_indicator = hunk_status_indicator(hunk);
        lines.push(Line::from(vec![
            Span::styled(status_indicator, hunk_status_style(hunk)),
            Span::raw(" "),
            Span::styled(&hunk.header, header_style),
        ]));

        // Hunk lines
        for (line_idx, diff_line) in hunk.lines.iter().enumerate() {
            let prefix = diff_line.kind.prefix();

            // Build line number gutter
            let old_no = diff_line
                .old_lineno
                .map(|n| format!("{:>4}", n))
                .unwrap_or_else(|| "    ".to_string());
            let new_no = diff_line
                .new_lineno
                .map(|n| format!("{:>4}", n))
                .unwrap_or_else(|| "    ".to_string());

            let gutter_style = Style::default()
                .fg(theme::context_fg())
                .add_modifier(Modifier::DIM);

            // Use cached syntax highlighting
            let highlighted = highlighted_lines
                .and_then(|h| h.get(hunk_idx))
                .and_then(|h| h.get(line_idx))
                .cloned()
                .unwrap_or_else(|| Line::from(diff_line.content.clone()));

            let mut spans = vec![
                Span::styled(old_no, gutter_style),
                Span::styled(" ", gutter_style),
                Span::styled(new_no, gutter_style),
                Span::styled(" ", gutter_style),
                Span::styled(
                    prefix,
                    match diff_line.kind {
                        LineKind::Added => Style::default()
                            .fg(theme::added_fg())
                            .add_modifier(Modifier::BOLD),
                        LineKind::Removed => Style::default()
                            .fg(theme::removed_fg())
                            .add_modifier(Modifier::BOLD),
                        LineKind::Context => Style::default().fg(theme::context_fg()),
                    },
                ),
            ];
            spans.extend(highlighted.spans);

            lines.push(Line::from(spans));
        }

        // Separator between hunks
        if hunk_idx < file.hunks.len() - 1 {
            lines.push(Line::from(Span::styled(
                "─".repeat(area.width.saturating_sub(2) as usize),
                Style::default().fg(theme::border_unfocused()),
            )));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((scroll_offset as u16, 0));

    frame.render_widget(paragraph, area);
}

fn hunk_status_indicator(hunk: &Hunk) -> &'static str {
    match hunk.status {
        HunkStatus::Pending => "○",
        HunkStatus::Staged => "✓",
        HunkStatus::Skipped => "✗",
        HunkStatus::Edited => "✎",
        HunkStatus::Commented => "💬",
    }
}

fn hunk_status_style(hunk: &Hunk) -> Style {
    match hunk.status {
        HunkStatus::Pending => Style::default().fg(theme::status_pending_fg()),
        HunkStatus::Staged => Style::default().fg(theme::status_staged_fg()),
        HunkStatus::Skipped => Style::default().fg(theme::status_skipped_fg()),
        HunkStatus::Edited => Style::default().fg(theme::status_edited_fg()),
        HunkStatus::Commented => Style::default().fg(theme::status_commented_fg()),
    }
}
