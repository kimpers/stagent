use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, MouseButton, MouseEventKind};
use git2::Repository;
use ratatui::layout::Rect;
use ratatui::text::Line;
use std::io;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crate::diff;
use crate::editor;
use crate::highlight::Highlighter;
use crate::staging;
use crate::types::{AppMode, FileDiff, FocusPanel, Hunk, HunkFeedback, HunkStatus};
use crate::ui;

/// Pending editor state while waiting for the user to close a tmux split pane.
pub struct EditorState {
    pub tmpfile: tempfile::NamedTempFile,
    pub rx: Receiver<()>,
    pub is_comment: bool,
    pub original_content: String,
}

/// Application state for the TUI.
pub struct App {
    pub files: Vec<FileDiff>,
    pub selected_file: usize,
    pub selected_hunk: usize,
    pub scroll_offset: u32,
    pub feedback: Vec<HunkFeedback>,
    pub mode: AppMode,
    pub focus: FocusPanel,
    pub message: Option<String>,
    pub no_stage: bool,
    /// Cached file list area for mouse click mapping.
    pub file_list_area: Rect,
    /// Whether the UI needs to be redrawn.
    pub dirty: bool,
    /// Cached highlighted lines: (file_index, per-hunk lines).
    pub highlight_cache: Option<(usize, Vec<Vec<Line<'static>>>)>,
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
            dirty: true,
            highlight_cache: None,
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

    /// Select the next file (wraps around).
    pub fn select_next_file(&mut self) {
        if self.files.is_empty() {
            return;
        }
        if self.selected_file + 1 < self.files.len() {
            self.selected_file += 1;
        } else {
            self.selected_file = 0;
        }
        self.selected_hunk = 0;
        self.scroll_offset = 0;
        self.dirty = true;
    }

    /// Select the previous file (wraps around).
    pub fn select_prev_file(&mut self) {
        if self.files.is_empty() {
            return;
        }
        if self.selected_file > 0 {
            self.selected_file -= 1;
        } else {
            self.selected_file = self.files.len() - 1;
        }
        self.selected_hunk = 0;
        self.scroll_offset = 0;
        self.dirty = true;
    }

    /// Select the next hunk (advances to next file if at end, wraps at last file).
    pub fn select_next_hunk(&mut self) {
        if let Some(file) = self.files.get(self.selected_file) {
            if self.selected_hunk + 1 < file.hunks.len() {
                self.selected_hunk += 1;
            } else if self.selected_file + 1 < self.files.len() {
                self.selected_file += 1;
                self.selected_hunk = 0;
            } else {
                // Wrap to first hunk of first file
                self.selected_file = 0;
                self.selected_hunk = 0;
            }
        }
        self.scroll_to_selected_hunk();
        self.dirty = true;
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
        self.dirty = true;
    }

    /// Scroll the diff view down.
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
        self.dirty = true;
    }

    /// Scroll the diff view up.
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
        self.dirty = true;
    }

    /// Toggle focus between file list and diff view.
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            FocusPanel::FileList => FocusPanel::DiffView,
            FocusPanel::DiffView => FocusPanel::FileList,
        };
        self.dirty = true;
    }

    /// Compute the line offset for the current hunk caused by previously staged
    /// hunks in the same file. Each staged hunk that appears before this one
    /// shifts line numbers by (new_lines - old_lines).
    fn compute_line_offset(&self, file_idx: usize, hunk_idx: usize) -> i32 {
        let mut offset: i32 = 0;
        if let Some(file) = self.files.get(file_idx) {
            for (idx, h) in file.hunks.iter().enumerate() {
                if idx == hunk_idx {
                    break;
                }
                if h.status == HunkStatus::Staged {
                    offset += h.new_lines as i32 - h.old_lines as i32;
                }
            }
        }
        offset
    }

    /// Access the current pending hunk mutably and execute a closure on it.
    /// Returns `true` if the closure was executed (hunk exists and is Pending).
    fn with_current_pending_hunk<F>(&mut self, repo: Option<&Repository>, f: F) -> Result<bool>
    where
        F: FnOnce(&mut Self, usize, usize, Option<&Repository>) -> Result<()>,
    {
        let file_idx = self.selected_file;
        let hunk_idx = self.selected_hunk;

        let is_pending = self
            .files
            .get(file_idx)
            .and_then(|file| file.hunks.get(hunk_idx))
            .is_some_and(|hunk| hunk.status == HunkStatus::Pending);

        if is_pending {
            f(self, file_idx, hunk_idx, repo)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Stage the current hunk.
    pub fn stage_current_hunk(&mut self, repo: &Repository) -> Result<()> {
        self.with_current_pending_hunk(Some(repo), |app, fi, hi, repo| {
            if !app.no_stage {
                let offset = app.compute_line_offset(fi, hi);
                staging::stage_hunk(
                    repo.unwrap(),
                    &app.files[fi],
                    &app.files[fi].hunks[hi],
                    offset,
                )?;
            }
            app.files[fi].hunks[hi].status = HunkStatus::Staged;
            app.message = Some("Hunk staged".to_string());
            app.select_next_hunk();
            Ok(())
        })?;
        Ok(())
    }

    /// Skip the current hunk.
    pub fn skip_current_hunk(&mut self) {
        let _ = self.with_current_pending_hunk(None, |app, fi, hi, _| {
            app.files[fi].hunks[hi].status = HunkStatus::Skipped;
            app.message = Some("Hunk skipped".to_string());
            app.select_next_hunk();
            Ok(())
        });
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
                    self.highlight_cache = None;
                } else {
                    self.message = Some("Cannot split hunk further".to_string());
                }
            }
        }
        self.dirty = true;
    }

    /// Start the editor flow for the current hunk (edit or comment).
    fn start_editor_flow(
        &mut self,
        prepare_fn: fn(&Hunk) -> Result<tempfile::NamedTempFile>,
        is_comment: bool,
    ) -> Result<Option<EditorState>> {
        if let Some(hunk) = self.current_hunk() {
            let tmpfile = prepare_fn(hunk)?;
            let original_content = std::fs::read_to_string(tmpfile.path())?;
            let tmp_path = tmpfile.path().to_string_lossy().to_string();
            let pane_id = editor::open_editor(&tmp_path)?;
            let rx = editor::wait_for_pane_close(pane_id);
            self.mode = AppMode::WaitingForEditor;
            self.dirty = true;
            Ok(Some(EditorState {
                tmpfile,
                rx,
                is_comment,
                original_content,
            }))
        } else {
            Ok(None)
        }
    }

    /// Start the edit flow for the current hunk.
    pub fn start_edit(&mut self) -> Result<Option<EditorState>> {
        self.start_editor_flow(editor::prepare_edit_tempfile, false)
    }

    /// Start the comment flow for the current hunk.
    pub fn start_comment(&mut self) -> Result<Option<EditorState>> {
        self.start_editor_flow(editor::prepare_comment_tempfile, true)
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
                self.dirty = true;
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

        if let Some(file) = self.current_file() {
            let file_path = file.path.to_string_lossy().to_string();
            if let Some(hunk) = self.current_hunk() {
                let hunk_header = hunk.header.clone();
                let hunk_lines = hunk.lines.clone();

                if is_comment {
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
                } else {
                    let original = editor::extract_new_side_content(&hunk_lines);
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
        }
        self.mode = AppMode::Browsing;
        self.dirty = true;
        captured
    }

    /// Estimate scroll position for the currently selected hunk.
    fn scroll_to_selected_hunk(&mut self) {
        let mut line_count: u32 = 0;
        if let Some(file) = self.files.get(self.selected_file) {
            for (idx, hunk) in file.hunks.iter().enumerate() {
                if idx == self.selected_hunk {
                    self.scroll_offset = line_count;
                    return;
                }
                line_count += 1; // header
                line_count += hunk.lines.len() as u32;
                line_count += 1; // separator
            }
        }
    }
}

/// Guard that restores terminal state on drop (including panics).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
        );
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

    // Guard ensures terminal is restored even on panic
    let _guard = TerminalGuard;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut app = App::new(files, no_stage);
    let highlighter = Highlighter::new();

    let mut editor_state: Option<EditorState> = None;

    let result = loop {
        // Draw only when state has changed
        if app.dirty {
            terminal.draw(|frame| {
                ui::render(frame, &mut app, &highlighter);
            })?;
            app.dirty = false;
        }

        // Check if editor has closed
        if let Some(ref state) = editor_state {
            if state.rx.try_recv().is_ok() {
                // Take ownership to process
                let state = editor_state.take().unwrap();
                let captured = app.flush_pending_editor_state(
                    state.tmpfile.path(),
                    state.is_comment,
                    &state.original_content,
                );
                app.message = Some(if captured {
                    if state.is_comment {
                        "Comment captured".to_string()
                    } else {
                        "Edit captured".to_string()
                    }
                } else {
                    "No changes detected".to_string()
                });
                app.dirty = true;
            }
        }

        // Handle events
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if app.mode == AppMode::WaitingForEditor {
                        // Only allow quit while waiting for editor
                        if key.code == KeyCode::Char('q') {
                            if let Some(state) = editor_state.take() {
                                app.flush_pending_editor_state(
                                    state.tmpfile.path(),
                                    state.is_comment,
                                    &state.original_content,
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
                            Ok(Some(state)) => {
                                editor_state = Some(state);
                            }
                            Ok(None) => {
                                app.message = Some("No hunk selected".to_string());
                            }
                            Err(e) => {
                                app.message = Some(format!("Edit error: {}", e));
                            }
                        },
                        KeyCode::Char('c') => match app.start_comment() {
                            Ok(Some(state)) => {
                                editor_state = Some(state);
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
                Event::Resize(_, _) => {
                    app.dirty = true;
                }
                _ => {}
            }
        }
    };

    // _guard will restore terminal on drop
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DeltaStatus, DiffLine, HunkStatus, LineKind};

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
    fn test_next_hunk_wraps_at_end() {
        let mut app = App::new(make_test_files(), false);
        // Navigate to last hunk of last file
        app.selected_file = 1;
        app.selected_hunk = 0; // only one hunk
        app.select_next_hunk();
        // Should wrap to first hunk of first file
        assert_eq!(app.selected_file, 0);
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
        let mut app = App::new(make_test_files(), true);
        // Stage first file's hunks via skip (since no_stage=true)
        app.selected_file = 1;
        app.skip_current_hunk();
        let file = &app.files[1];
        assert!(file.hunks.iter().all(|h| h.status != HunkStatus::Pending));
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

    #[test]
    fn test_dirty_flag_set_on_navigation() {
        let mut app = App::new(make_test_files(), false);
        assert!(app.dirty, "dirty should start true");
        app.dirty = false;

        app.select_next_file();
        assert!(app.dirty, "dirty should be true after select_next_file");
        app.dirty = false;

        app.select_prev_file();
        assert!(app.dirty, "dirty should be true after select_prev_file");
        app.dirty = false;

        app.select_next_hunk();
        assert!(app.dirty, "dirty should be true after select_next_hunk");
        app.dirty = false;

        app.select_prev_hunk();
        assert!(app.dirty, "dirty should be true after select_prev_hunk");
        app.dirty = false;

        app.scroll_down();
        assert!(app.dirty, "dirty should be true after scroll_down");
        app.dirty = false;

        app.scroll_up();
        assert!(app.dirty, "dirty should be true after scroll_up");
        app.dirty = false;

        app.toggle_focus();
        assert!(app.dirty, "dirty should be true after toggle_focus");
        app.dirty = false;

        app.skip_current_hunk();
        assert!(app.dirty, "dirty should be true after skip_current_hunk");
        app.dirty = false;

        app.split_current_hunk();
        assert!(app.dirty, "dirty should be true after split_current_hunk");
    }

    #[test]
    fn test_compute_line_offset_no_staged() {
        let app = App::new(make_test_files(), false);
        assert_eq!(app.compute_line_offset(0, 1), 0);
    }

    #[test]
    fn test_compute_line_offset_with_staged() {
        let mut app = App::new(make_test_files(), false);
        // First hunk: old_lines=3, new_lines=3 → offset 0
        app.files[0].hunks[0].status = HunkStatus::Staged;
        assert_eq!(app.compute_line_offset(0, 1), 0);

        // Change first hunk to have different new_lines
        app.files[0].hunks[0].new_lines = 5;
        // offset = 5 - 3 = 2
        assert_eq!(app.compute_line_offset(0, 1), 2);
    }
}
