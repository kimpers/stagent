use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::types::{AppMode, FileDiff, HunkStatus};
use crate::ui::theme;

/// Render the status bar at the bottom of the screen.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    files: &[FileDiff],
    mode: AppMode,
    message: Option<&str>,
) {
    let line = match mode {
        AppMode::WaitingForEditor => Line::from(vec![
            Span::styled(" Editing in split pane... ", theme::status_bar_style()),
            Span::styled("(waiting for editor to close)", theme::status_bar_style()),
        ]),
        AppMode::Help => Line::from(Span::styled(
            " Press any key to dismiss help ",
            theme::status_bar_style(),
        )),
        AppMode::Browsing => {
            if let Some(msg) = message {
                Line::from(Span::styled(
                    format!(" {} ", msg),
                    theme::status_bar_style(),
                ))
            } else {
                let progress = compute_progress(files);
                Line::from(vec![
                    Span::styled(
                        " y:stage  n:skip  s:split  e:edit  c:comment  q:quit  ?:help ",
                        theme::status_bar_style(),
                    ),
                    Span::styled(
                        format!(" [{}/{}] ", progress.0, progress.1),
                        theme::status_bar_style(),
                    ),
                ])
            }
        }
    };

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Compute (reviewed_hunks, total_hunks) for progress display.
fn compute_progress(files: &[FileDiff]) -> (usize, usize) {
    let total: usize = files.iter().map(|f| f.hunks.len()).sum();
    let reviewed: usize = files
        .iter()
        .flat_map(|f| &f.hunks)
        .filter(|h| h.status != HunkStatus::Pending)
        .count();
    (reviewed, total)
}
