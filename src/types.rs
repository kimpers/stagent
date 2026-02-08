use std::path::PathBuf;

/// Represents a file with unstaged changes and its collection of diff hunks.
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: PathBuf,
    pub hunks: Vec<Hunk>,
    pub status: DeltaStatus,
    pub is_binary: bool,
}

/// Maps to git2 Delta variants we care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

/// A single diff hunk with header, lines, and review status.
#[derive(Debug, Clone)]
pub struct Hunk {
    /// The @@ header line, e.g. "@@ -10,5 +10,7 @@ fn foo()"
    pub header: String,
    pub lines: Vec<DiffLine>,
    pub status: HunkStatus,
    /// Old file start line (from the @@ header)
    pub old_start: u32,
    /// Old file line count
    pub old_lines: u32,
    /// New file start line
    pub new_start: u32,
    /// New file line count
    #[allow(dead_code)]
    pub new_lines: u32,
}

/// A single line within a diff hunk.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
}

/// The type of a diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

impl LineKind {
    /// Returns the single-character prefix used in unified diff format.
    pub fn prefix(self) -> &'static str {
        match self {
            LineKind::Context => " ",
            LineKind::Added => "+",
            LineKind::Removed => "-",
        }
    }
}

impl std::fmt::Display for Hunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.header)
    }
}

/// Review status for a hunk during the interactive session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkStatus {
    Pending,
    Staged,
    Skipped,
    Edited,
    Commented,
}

/// The current mode of the TUI application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Browsing,
    WaitingForEditor,
    Help,
}

/// Which panel is focused in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    FileList,
    DiffView,
}

/// Feedback collected from user edits or comments on a hunk.
#[derive(Debug, Clone)]
pub struct HunkFeedback {
    pub file_path: String,
    pub hunk_header: String,
    pub kind: FeedbackKind,
    pub content: String,
    /// The diff lines from the hunk, used to provide context around comments.
    pub context_lines: Vec<DiffLine>,
    /// For comments: each comment's position (index into context_lines after
    /// which it appears) and text. Allows rendering comments inline at the
    /// correct location within the diff.
    pub comment_positions: Vec<(usize, String)>,
}

/// The type of feedback: an edit (unified diff) or a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackKind {
    Edit,
    Comment,
}
