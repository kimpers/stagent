use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

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
            let status_icon = file_status_icon(file);
            let delta_icon = delta_status_icon(file.status);
            let path_str = file.path.to_string_lossy();

            let style = if i == selected {
                theme::selected_style().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let line = Line::from(vec![
                Span::styled(status_icon, status_color(file)),
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

/// Get a status icon for the overall file review state.
fn file_status_icon(file: &FileDiff) -> &'static str {
    if file.hunks.is_empty() {
        return " ";
    }

    let all_staged = file.hunks.iter().all(|h| h.status == HunkStatus::Staged);
    let all_done = file.hunks.iter().all(|h| h.status != HunkStatus::Pending);
    let any_staged = file.hunks.iter().any(|h| h.status == HunkStatus::Staged);

    if all_staged {
        "✓"
    } else if all_done {
        "●"
    } else if any_staged {
        "◐"
    } else {
        "○"
    }
}

fn status_color(file: &FileDiff) -> Style {
    if file.hunks.is_empty() {
        return Style::default();
    }

    let all_staged = file.hunks.iter().all(|h| h.status == HunkStatus::Staged);
    let all_done = file.hunks.iter().all(|h| h.status != HunkStatus::Pending);

    if all_staged {
        Style::default().fg(theme::STATUS_STAGED_FG)
    } else if all_done {
        Style::default().fg(theme::STATUS_EDITED_FG)
    } else {
        Style::default().fg(theme::STATUS_PENDING_FG)
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
