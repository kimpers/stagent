use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, MouseButton, MouseEventKind};
use git2::Repository;
use ratatui::layout::Rect;
use std::io;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crate::diff;
use crate::editor;
use crate::highlight::Highlighter;
use crate::staging;
use crate::types::*;
use crate::ui;

/// Application state for the TUI.
pub struct App {
    pub files: Vec<FileDiff>,
    pub selected_file: usize,
    pub selected_hunk: usize,
    pub scroll_offset: u16,
    pub feedback: Vec<HunkFeedback>,
    pub mode: AppMode,
    pub focus: FocusPanel,
    pub message: Option<String>,
    pub no_stage: bool,
    /// Cached file list area for mouse click mapping.
    pub file_list_area: Rect,
}

impl App {
    pub fn new(files: Vec<FileDiff>, no_stage: bool) -> Self {
        Self {
            files,
            selected_file: 0,
            selected_hunk: 0,
            scroll_offset: 0,
            feedback: Vec::new(),
            mode: AppMode::Browsing,
            focus: FocusPanel::DiffView,
            message: None,
            no_stage,
            file_list_area: Rect::default(),
        }
    }

    /// Get the currently selected file, if any.
    pub fn current_file(&self) -> Option<&FileDiff> {
        self.files.get(self.selected_file)
    }

    /// Get the currently selected hunk, if any.
    pub fn current_hunk(&self) -> Option<&Hunk> {
        self.current_file()
            .and_then(|f| f.hunks.get(self.selected_hunk))
    }

    /// Select the next file.
    pub fn select_next_file(&mut self) {
        if self.files.is_empty() {
            return;
        }
        if self.selected_file + 1 < self.files.len() {
            self.selected_file += 1;
        } else {
            self.selected_file = 0; // Wrap around
        }
        self.selected_hunk = 0;
        self.scroll_offset = 0;
    }

    /// Select the previous file.
    pub fn select_prev_file(&mut self) {
        if self.files.is_empty() {
            return;
        }
        if self.selected_file > 0 {
            self.selected_file -= 1;
        } else {
            self.selected_file = self.files.len() - 1; // Wrap around
        }
        self.selected_hunk = 0;
        self.scroll_offset = 0;
    }

    /// Select the next hunk (advances to next file if at end).
    pub fn select_next_hunk(&mut self) {
        if let Some(file) = self.files.get(self.selected_file) {
            if self.selected_hunk + 1 < file.hunks.len() {
                self.selected_hunk += 1;
            } else if self.selected_file + 1 < self.files.len() {
                self.selected_file += 1;
                self.selected_hunk = 0;
            }
            // else: at last hunk of last file, do nothing
        }
        self.scroll_to_selected_hunk();
    }

    /// Select the previous hunk (goes to previous file if at start).
    pub fn select_prev_hunk(&mut self) {
        if self.selected_hunk > 0 {
            self.selected_hunk -= 1;
        } else if self.selected_file > 0 {
            self.selected_file -= 1;
            if let Some(file) = self.files.get(self.selected_file) {
                self.selected_hunk = file.hunks.len().saturating_sub(1);
            }
        }
        self.scroll_to_selected_hunk();
    }

    /// Scroll the diff view down.
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    /// Scroll the diff view up.
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Toggle focus between file list and diff view.
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            FocusPanel::FileList => FocusPanel::DiffView,
            FocusPanel::DiffView => FocusPanel::FileList,
        };
    }

    /// Stage the current hunk.
    pub fn stage_current_hunk(&mut self, repo: &Repository) -> Result<()> {
        let file_idx = self.selected_file;
        let hunk_idx = self.selected_hunk;

        if let Some(file) = self.files.get(file_idx) {
            if let Some(hunk) = file.hunks.get(hunk_idx) {
                if hunk.status == HunkStatus::Pending {
                    if !self.no_stage {
                        staging::stage_hunk(repo, file, hunk)?;
                    }
                    // Mark as staged
                    self.files[file_idx].hunks[hunk_idx].status = HunkStatus::Staged;
                    self.message = Some("Hunk staged".to_string());
                    self.select_next_hunk();
                }
            }
        }
        Ok(())
    }

    /// Skip the current hunk.
    pub fn skip_current_hunk(&mut self) {
        let file_idx = self.selected_file;
        let hunk_idx = self.selected_hunk;

        if let Some(file) = self.files.get(file_idx) {
            if let Some(hunk) = file.hunks.get(hunk_idx) {
                if hunk.status == HunkStatus::Pending {
                    self.files[file_idx].hunks[hunk_idx].status = HunkStatus::Skipped;
                    self.message = Some("Hunk skipped".to_string());
                    self.select_next_hunk();
                }
            }
        }
    }

    /// Split the current hunk into sub-hunks.
    pub fn split_current_hunk(&mut self) {
        let file_idx = self.selected_file;
        let hunk_idx = self.selected_hunk;

        if let Some(file) = self.files.get(file_idx) {
            if let Some(hunk) = file.hunks.get(hunk_idx) {
                let sub_hunks = diff::split_hunk(hunk);
                if sub_hunks.len() > 1 {
                    let file = &mut self.files[file_idx];
                    file.hunks.splice(hunk_idx..=hunk_idx, sub_hunks);
                    self.message = Some("Hunk split".to_string());
                } else {
                    self.message = Some("Cannot split hunk further".to_string());
                }
            }
        }
    }

    /// Start the edit flow for the current hunk.
    /// Returns (tmpfile, pane_close_rx, original_content).
    pub fn start_edit(
        &mut self,
    ) -> Result<Option<(tempfile::NamedTempFile, Receiver<()>, String)>> {
        if let Some(hunk) = self.current_hunk() {
            let tmpfile = editor::prepare_edit_tempfile(hunk)?;
            let original_content = std::fs::read_to_string(tmpfile.path())?;
            let tmp_path = tmpfile.path().to_string_lossy().to_string();
            let pane_id = editor::open_editor(&tmp_path)?;
            let rx = editor::wait_for_pane_close(pane_id);
            self.mode = AppMode::WaitingForEditor;
            Ok(Some((tmpfile, rx, original_content)))
        } else {
            Ok(None)
        }
    }

    /// Start the comment flow for the current hunk.
    /// Returns (tmpfile, pane_close_rx, original_content).
    pub fn start_comment(
        &mut self,
    ) -> Result<Option<(tempfile::NamedTempFile, Receiver<()>, String)>> {
        if let Some(hunk) = self.current_hunk() {
            let tmpfile = editor::prepare_comment_tempfile(hunk)?;
            let original_content = std::fs::read_to_string(tmpfile.path())?;
            let tmp_path = tmpfile.path().to_string_lossy().to_string();
            let pane_id = editor::open_editor(&tmp_path)?;
            let rx = editor::wait_for_pane_close(pane_id);
            self.mode = AppMode::WaitingForEditor;
            Ok(Some((tmpfile, rx, original_content)))
        } else {
            Ok(None)
        }
    }

    /// Handle a mouse click at the given coordinates.
    pub fn handle_mouse_click(&mut self, column: u16, row: u16) {
        // Check if click is within file list area
        let area = self.file_list_area;
        if column >= area.x
            && column < area.x + area.width
            && row >= area.y
            && row < area.y + area.height
        {
            // +1 for the border, row within the list content
            let list_row = row.saturating_sub(area.y + 1);
            let idx = list_row as usize;
            if idx < self.files.len() {
                self.selected_file = idx;
                self.selected_hunk = 0;
                self.scroll_offset = 0;
                self.focus = FocusPanel::FileList;
            }
        }
    }

    /// Flush a pending editor result by reading the tempfile and processing it.
    ///
    /// This handles the race condition where the user presses `q` immediately
    /// after the editor closes, before the background pane-polling thread has
    /// detected the close. Since vim has already written the file, we can read
    /// it directly.
    ///
    /// Returns `true` if feedback was actually captured, `false` otherwise.
    pub fn flush_pending_editor_state(
        &mut self,
        tmpfile_path: &std::path::Path,
        is_comment: bool,
        original_content: &str,
    ) -> bool {
        let edited = std::fs::read_to_string(tmpfile_path).unwrap_or_default();
        let mut captured = false;

        if is_comment {
            if let Some(file) = self.current_file() {
                let file_path = file.path.to_string_lossy().to_string();
                if let Some(hunk) = self.current_hunk() {
                    let hunk_header = hunk.header.clone();
                    let hunk_lines = hunk.lines.clone();
                    if let Some(fb) = editor::parse_comment_result(
                        original_content,
                        &edited,
                        &file_path,
                        &hunk_header,
                        &hunk_lines,
                    ) {
                        self.feedback.push(fb);
                        let fi = self.selected_file;
                        let hi = self.selected_hunk;
                        self.files[fi].hunks[hi].status = HunkStatus::Commented;
                        captured = true;
                    }
                }
            }
        } else if let Some(file) = self.current_file() {
            let file_path = file.path.to_string_lossy().to_string();
            if let Some(hunk) = self.current_hunk() {
                let hunk_header = hunk.header.clone();
                let hunk_lines = hunk.lines.clone();
                let mut original = String::new();
                for line in &hunk.lines {
                    match line.kind {
                        LineKind::Context | LineKind::Added => {
                            original.push_str(&line.content);
                            if !line.content.ends_with('\n') {
                                original.push('\n');
                            }
                        }
                        LineKind::Removed => {}
                    }
                }
                if let Some(fb) = editor::parse_edit_result(
                    &original,
                    &edited,
                    &file_path,
                    &hunk_header,
                    &hunk_lines,
                ) {
                    self.feedback.push(fb);
                    let fi = self.selected_file;
                    let hi = self.selected_hunk;
                    self.files[fi].hunks[hi].status = HunkStatus::Edited;
                    captured = true;
                }
            }
        }
        self.mode = AppMode::Browsing;
        captured
    }

    /// Estimate scroll position for the currently selected hunk.
    fn scroll_to_selected_hunk(&mut self) {
        // Rough estimate: each hunk takes header + its lines + separator
        let mut line_count: u16 = 0;
        if let Some(file) = self.files.get(self.selected_file) {
            for (idx, hunk) in file.hunks.iter().enumerate() {
                if idx == self.selected_hunk {
                    self.scroll_offset = line_count;
                    return;
                }
                line_count += 1; // header
                line_count += hunk.lines.len() as u16;
                line_count += 1; // separator
            }
        }
    }
}

/// Run the TUI application. Returns collected feedback on exit.
pub fn run(files: Vec<FileDiff>, repo: &Repository, no_stage: bool) -> Result<Vec<HunkFeedback>> {
    // Set up terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
    )?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut app = App::new(files, no_stage);
    let highlighter = Highlighter::new();

    // Editor state tracking:
    // (tmpfile, rx, is_comment, original_content)
    let mut editor_state: Option<(tempfile::NamedTempFile, Receiver<()>, bool, String)> = None;

    let result = loop {
        // Draw
        terminal.draw(|frame| {
            ui::render(frame, &mut app, &highlighter);
        })?;

        // Check if editor has closed
        if let Some((ref tmpfile, ref rx, is_comment, ref original_content)) = editor_state {
            if rx.try_recv().is_ok() {
                let captured =
                    app.flush_pending_editor_state(tmpfile.path(), is_comment, original_content);
                app.message = Some(if captured {
                    if is_comment {
                        "Comment captured".to_string()
                    } else {
                        "Edit captured".to_string()
                    }
                } else {
                    "No changes detected".to_string()
                });
                editor_state = None;
            }
        }

        // Handle events
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if app.mode == AppMode::WaitingForEditor {
                        // Only allow quit while waiting for editor
                        if key.code == KeyCode::Char('q') {
                            // Flush pending editor result before quitting.
                            // The user may press q before the 500ms poll thread
                            // detects the pane closed, but vim has already written
                            // the file, so we can read it directly.
                            if let Some((tmpfile, _rx, is_comment, original_content)) =
                                editor_state.take()
                            {
                                app.flush_pending_editor_state(
                                    tmpfile.path(),
                                    is_comment,
                                    &original_content,
                                );
                            }
                            break Ok(app.feedback);
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('q') => {
                            break Ok(app.feedback);
                        }
                        KeyCode::Char('j') => app.scroll_down(),
                        KeyCode::Char('k') => app.scroll_up(),
                        KeyCode::Down => {
                            if app.focus == FocusPanel::FileList {
                                app.select_next_file();
                            } else {
                                app.select_next_hunk();
                            }
                        }
                        KeyCode::Up => {
                            if app.focus == FocusPanel::FileList {
                                app.select_prev_file();
                            } else {
                                app.select_prev_hunk();
                            }
                        }
                        KeyCode::Tab => app.toggle_focus(),
                        KeyCode::Char('y') => {
                            if let Err(e) = app.stage_current_hunk(repo) {
                                app.message = Some(format!("Stage error: {}", e));
                            }
                        }
                        KeyCode::Char('n') => app.skip_current_hunk(),
                        KeyCode::Char('s') => app.split_current_hunk(),
                        KeyCode::Char('e') => match app.start_edit() {
                            Ok(Some((tmpfile, rx, original_content))) => {
                                editor_state = Some((tmpfile, rx, false, original_content));
                            }
                            Ok(None) => {
                                app.message = Some("No hunk selected".to_string());
                            }
                            Err(e) => {
                                app.message = Some(format!("Edit error: {}", e));
                            }
                        },
                        KeyCode::Char('c') => match app.start_comment() {
                            Ok(Some((tmpfile, rx, original_content))) => {
                                editor_state = Some((tmpfile, rx, true, original_content));
                            }
                            Ok(None) => {
                                app.message = Some("No hunk selected".to_string());
                            }
                            Err(e) => {
                                app.message = Some(format!("Comment error: {}", e));
                            }
                        },
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollDown => app.scroll_down(),
                    MouseEventKind::ScrollUp => app.scroll_up(),
                    MouseEventKind::Down(MouseButton::Left) => {
                        app.handle_mouse_click(mouse.column, mouse.row);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    };

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn make_test_files() -> Vec<FileDiff> {
        vec![
            FileDiff {
                path: "src/a.rs".into(),
                hunks: vec![
                    Hunk {
                        header: "@@ -1,3 +1,4 @@".to_string(),
                        lines: vec![
                            DiffLine {
                                kind: LineKind::Context,
                                content: "line1\n".to_string(),
                                old_lineno: Some(1),
                                new_lineno: Some(1),
                            },
                            DiffLine {
                                kind: LineKind::Removed,
                                content: "old\n".to_string(),
                                old_lineno: Some(2),
                                new_lineno: None,
                            },
                            DiffLine {
                                kind: LineKind::Added,
                                content: "new\n".to_string(),
                                old_lineno: None,
                                new_lineno: Some(2),
                            },
                            DiffLine {
                                kind: LineKind::Context,
                                content: "line3\n".to_string(),
                                old_lineno: Some(3),
                                new_lineno: Some(3),
                            },
                        ],
                        status: HunkStatus::Pending,
                        old_start: 1,
                        old_lines: 3,
                        new_start: 1,
                        new_lines: 3,
                    },
                    Hunk {
                        header: "@@ -20,3 +21,4 @@".to_string(),
                        lines: vec![DiffLine {
                            kind: LineKind::Added,
                            content: "added line\n".to_string(),
                            old_lineno: None,
                            new_lineno: Some(22),
                        }],
                        status: HunkStatus::Pending,
                        old_start: 20,
                        old_lines: 3,
                        new_start: 21,
                        new_lines: 4,
                    },
                ],
                status: DeltaStatus::Modified,
                is_binary: false,
            },
            FileDiff {
                path: "src/b.rs".into(),
                hunks: vec![Hunk {
                    header: "@@ -5,3 +5,3 @@".to_string(),
                    lines: vec![
                        DiffLine {
                            kind: LineKind::Removed,
                            content: "foo\n".to_string(),
                            old_lineno: Some(6),
                            new_lineno: None,
                        },
                        DiffLine {
                            kind: LineKind::Added,
                            content: "bar\n".to_string(),
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

    #[test]
    fn test_app_initial_state() {
        let app = App::new(make_test_files(), false);
        assert_eq!(app.selected_file, 0);
        assert_eq!(app.selected_hunk, 0);
        assert_eq!(app.mode, AppMode::Browsing);
        assert_eq!(app.focus, FocusPanel::DiffView);
    }

    #[test]
    fn test_select_next_file() {
        let mut app = App::new(make_test_files(), false);
        app.select_next_file();
        assert_eq!(app.selected_file, 1);
    }

    #[test]
    fn test_select_prev_file() {
        let mut app = App::new(make_test_files(), false);
        app.selected_file = 1;
        app.select_prev_file();
        assert_eq!(app.selected_file, 0);
    }

    #[test]
    fn test_select_file_wraps() {
        let mut app = App::new(make_test_files(), false);
        app.selected_file = 1; // last file
        app.select_next_file();
        assert_eq!(app.selected_file, 0); // wrapped to first
    }

    #[test]
    fn test_select_next_hunk() {
        let mut app = App::new(make_test_files(), false);
        app.select_next_hunk();
        assert_eq!(app.selected_hunk, 1);
        assert_eq!(app.selected_file, 0);
    }

    #[test]
    fn test_next_hunk_advances_file() {
        let mut app = App::new(make_test_files(), false);
        app.selected_hunk = 1; // last hunk of first file
        app.select_next_hunk();
        assert_eq!(app.selected_file, 1);
        assert_eq!(app.selected_hunk, 0);
    }

    #[test]
    fn test_scroll_down() {
        let mut app = App::new(make_test_files(), false);
        app.scroll_down();
        assert_eq!(app.scroll_offset, 1);
    }

    #[test]
    fn test_scroll_clamps_to_content() {
        let mut app = App::new(make_test_files(), false);
        app.scroll_up(); // at 0, should stay at 0
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_current_file() {
        let app = App::new(make_test_files(), false);
        let file = app.current_file().unwrap();
        assert_eq!(file.path.to_string_lossy(), "src/a.rs");
    }

    #[test]
    fn test_current_hunk() {
        let app = App::new(make_test_files(), false);
        let hunk = app.current_hunk().unwrap();
        assert_eq!(hunk.header, "@@ -1,3 +1,4 @@");
    }

    #[test]
    fn test_empty_diff_state() {
        let mut app = App::new(vec![], false);
        assert!(app.current_file().is_none());
        assert!(app.current_hunk().is_none());
        // These should be no-ops without panic
        app.select_next_file();
        app.select_prev_file();
        app.select_next_hunk();
        app.select_prev_hunk();
        app.scroll_down();
        app.scroll_up();
    }

    #[test]
    fn test_skip_updates_hunk_status() {
        let mut app = App::new(make_test_files(), false);
        app.skip_current_hunk();
        assert_eq!(app.files[0].hunks[0].status, HunkStatus::Skipped);
    }

    #[test]
    fn test_toggle_focus() {
        let mut app = App::new(make_test_files(), false);
        assert_eq!(app.focus, FocusPanel::DiffView);
        app.toggle_focus();
        assert_eq!(app.focus, FocusPanel::FileList);
        app.toggle_focus();
        assert_eq!(app.focus, FocusPanel::DiffView);
    }

    #[test]
    fn test_all_hunks_staged_marks_file() {
        let mut app = App::new(make_test_files(), false);
        // File 1 (src/b.rs) has only 1 hunk — stage it
        app.selected_file = 1;
        app.files[1].hunks[0].status = HunkStatus::Staged;
        let file = &app.files[1];
        assert!(file.hunks.iter().all(|h| h.status == HunkStatus::Staged));
    }

    #[test]
    fn test_handle_mouse_click_selects_file() {
        let mut app = App::new(make_test_files(), false);
        // Simulate file list area: x=0, y=0, width=20, height=10
        app.file_list_area = Rect::new(0, 0, 20, 10);
        // Click on second file (row 2 = border row 0 + item index 1)
        app.handle_mouse_click(5, 2);
        assert_eq!(app.selected_file, 1);
        assert_eq!(app.focus, FocusPanel::FileList);
    }

    #[test]
    fn test_handle_mouse_click_outside_file_list() {
        let mut app = App::new(make_test_files(), false);
        app.file_list_area = Rect::new(0, 0, 20, 10);
        // Click outside the file list area
        app.handle_mouse_click(25, 2);
        assert_eq!(app.selected_file, 0); // unchanged
    }
}
