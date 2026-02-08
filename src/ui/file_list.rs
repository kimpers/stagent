use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::types::{DeltaStatus, FileDiff, HunkStatus};
use crate::ui::theme;

/// Render the file list panel.
pub fn render(frame: &mut Frame, area: Rect, files: &[FileDiff], selected: usize, focused: bool) {
    let border_style = if focused {
        theme::border_focused_style()
    } else {
        theme::border_unfocused_style()
    };

    let block = Block::default()
        .title(" Files ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let (status_icon, status_style) = file_review_status(file);
            let delta_icon = delta_status_icon(file.status);
            let path_str = file.path.to_string_lossy();

            let style = if i == selected {
                theme::selected_style().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let line = Line::from(vec![
                Span::styled(status_icon, status_style),
                Span::raw(" "),
                Span::styled(delta_icon, delta_color(file.status)),
                Span::raw(" "),
                Span::styled(path_str.to_string(), style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(selected));

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected_style())
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state);
}

/// Compute the file's review status icon and style in a single pass over hunks.
fn file_review_status(file: &FileDiff) -> (&'static str, Style) {
    if file.hunks.is_empty() {
        return (" ", Style::default());
    }

    let mut all_staged = true;
    let mut all_done = true;
    let mut any_staged = false;

    for h in &file.hunks {
        match h.status {
            HunkStatus::Staged => {
                any_staged = true;
            }
            HunkStatus::Pending => {
                all_staged = false;
                all_done = false;
            }
            _ => {
                all_staged = false;
            }
        }
    }

    if all_staged {
        ("✓", Style::default().fg(theme::STATUS_STAGED_FG))
    } else if all_done {
        ("●", Style::default().fg(theme::STATUS_EDITED_FG))
    } else if any_staged {
        ("◐", Style::default().fg(theme::STATUS_PENDING_FG))
    } else {
        ("○", Style::default().fg(theme::STATUS_PENDING_FG))
    }
}

fn delta_status_icon(status: DeltaStatus) -> &'static str {
    match status {
        DeltaStatus::Modified => "M",
        DeltaStatus::Added => "A",
        DeltaStatus::Deleted => "D",
        DeltaStatus::Renamed => "R",
        DeltaStatus::Untracked => "?",
    }
}

fn delta_color(status: DeltaStatus) -> Style {
    match status {
        DeltaStatus::Modified => Style::default().fg(theme::FILE_HEADER_FG),
        DeltaStatus::Added => Style::default().fg(theme::STATUS_STAGED_FG),
        DeltaStatus::Deleted => Style::default().fg(theme::REMOVED_FG),
        DeltaStatus::Renamed => Style::default().fg(theme::HUNK_HEADER_FG),
        DeltaStatus::Untracked => Style::default().fg(theme::STATUS_PENDING_FG),
    }
}
