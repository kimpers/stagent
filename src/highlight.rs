use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::types::{Hunk, LineKind};
use crate::ui::theme;

/// Highlighter wraps syntect for syntax highlighting of diff lines.
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Detect the syntax for a file path.
    #[allow(dead_code)]
    pub fn detect_syntax(&self, path: &str) -> &str {
        let syntax = self
            .syntax_set
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        syntax.name.as_str()
    }

    /// Highlight a single line of code, returning ratatui Spans.
    /// The `kind` parameter controls the background color overlay.
    pub fn highlight_line(&self, path: &str, content: &str, kind: LineKind) -> Line<'static> {
        let bg = match kind {
            LineKind::Added => Some(theme::ADDED_BG),
            LineKind::Removed => Some(theme::REMOVED_DIM_BG),
            LineKind::Context => None,
        };

        // For removed lines, use simple dimmed red without syntax highlighting
        if kind == LineKind::Removed {
            let style = Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::DIM)
                .bg(theme::REMOVED_DIM_BG);
            return Line::from(Span::styled(content.to_string(), style));
        }

        // Try syntax highlighting for context and added lines
        let syntax = self
            .syntax_set
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = match self.theme_set.themes.get("base16-ocean.dark") {
            Some(t) => t,
            None => {
                // Fallback to plain text if theme not found
                let style = match bg {
                    Some(bg_color) => Style::default().bg(bg_color),
                    None => Style::default(),
                };
                return Line::from(Span::styled(content.to_string(), style));
            }
        };
        let mut h = HighlightLines::new(syntax, theme);

        let line_with_newline = if content.ends_with('\n') {
            content.to_string()
        } else {
            format!("{}\n", content)
        };

        match h.highlight_line(&line_with_newline, &self.syntax_set) {
            Ok(ranges) => {
                let spans: Vec<Span> = ranges
                    .iter()
                    .map(|(style, text)| {
                        let fg =
                            Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                        let mut ratatui_style = Style::default().fg(fg);

                        if style.font_style.contains(FontStyle::BOLD) {
                            ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                        }
                        if style.font_style.contains(FontStyle::ITALIC) {
                            ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                        }

                        if let Some(bg_color) = bg {
                            ratatui_style = ratatui_style.bg(bg_color);
                        }

                        Span::styled(text.to_string(), ratatui_style)
                    })
                    .collect();
                Line::from(spans)
            }
            Err(_) => {
                // Fallback to plain text
                let style = match bg {
                    Some(bg_color) => Style::default().bg(bg_color),
                    None => Style::default(),
                };
                Line::from(Span::styled(content.to_string(), style))
            }
        }
    }

    /// Highlight all lines for a file at once, reusing a single `HighlightLines`
    /// instance across context/added lines for correct multi-line syntax state.
    /// Returns `Vec<Vec<Line>>` — outer = per hunk, inner = per `DiffLine`.
    pub fn highlight_file_lines(&self, path: &str, hunks: &[Hunk]) -> Vec<Vec<Line<'static>>> {
        let syntax = self
            .syntax_set
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = match self.theme_set.themes.get("base16-ocean.dark") {
            Some(t) => t,
            None => {
                // Fallback: return plain lines
                return hunks
                    .iter()
                    .map(|hunk| {
                        hunk.lines
                            .iter()
                            .map(|dl| Line::from(dl.content.clone()))
                            .collect()
                    })
                    .collect();
            }
        };

        let mut h = HighlightLines::new(syntax, theme);
        let mut result = Vec::with_capacity(hunks.len());

        for hunk in hunks {
            let mut hunk_lines = Vec::with_capacity(hunk.lines.len());
            for diff_line in &hunk.lines {
                if diff_line.kind == LineKind::Removed {
                    // Removed lines: dimmed red, no syntax highlighting
                    let style = Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::DIM)
                        .bg(theme::REMOVED_DIM_BG);
                    hunk_lines.push(Line::from(Span::styled(diff_line.content.clone(), style)));
                } else {
                    // Context and Added lines: syntax highlight with shared state
                    let bg = match diff_line.kind {
                        LineKind::Added => Some(theme::ADDED_BG),
                        LineKind::Context => None,
                        LineKind::Removed => unreachable!(),
                    };

                    let line_with_newline = if diff_line.content.ends_with('\n') {
                        diff_line.content.clone()
                    } else {
                        format!("{}\n", diff_line.content)
                    };

                    match h.highlight_line(&line_with_newline, &self.syntax_set) {
                        Ok(ranges) => {
                            let spans: Vec<Span> = ranges
                                .iter()
                                .map(|(style, text)| {
                                    let fg = Color::Rgb(
                                        style.foreground.r,
                                        style.foreground.g,
                                        style.foreground.b,
                                    );
                                    let mut ratatui_style = Style::default().fg(fg);

                                    if style.font_style.contains(FontStyle::BOLD) {
                                        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                                    }
                                    if style.font_style.contains(FontStyle::ITALIC) {
                                        ratatui_style =
                                            ratatui_style.add_modifier(Modifier::ITALIC);
                                    }

                                    if let Some(bg_color) = bg {
                                        ratatui_style = ratatui_style.bg(bg_color);
                                    }

                                    Span::styled(text.to_string(), ratatui_style)
                                })
                                .collect();
                            hunk_lines.push(Line::from(spans));
                        }
                        Err(_) => {
                            let style = match bg {
                                Some(bg_color) => Style::default().bg(bg_color),
                                None => Style::default(),
                            };
                            hunk_lines
                                .push(Line::from(Span::styled(diff_line.content.clone(), style)));
                        }
                    }
                }
            }
            result.push(hunk_lines);
        }

        result
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_syntax_rs() {
        let h = Highlighter::new();
        assert_eq!(h.detect_syntax("foo.rs"), "Rust");
    }

    #[test]
    fn test_detect_syntax_py() {
        let h = Highlighter::new();
        assert_eq!(h.detect_syntax("bar.py"), "Python");
    }

    #[test]
    fn test_detect_syntax_unknown() {
        let h = Highlighter::new();
        assert_eq!(h.detect_syntax("data.xyz"), "Plain Text");
    }

    #[test]
    fn test_highlight_rust_line() {
        let h = Highlighter::new();
        let line = h.highlight_line("foo.rs", "let x = 42;", LineKind::Context);
        // Should produce multiple spans (keyword, variable, number, etc.)
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_highlight_added_line() {
        let h = Highlighter::new();
        let line = h.highlight_line("foo.rs", "let x = 42;", LineKind::Added);
        // All spans should have green background
        for span in &line.spans {
            assert_eq!(span.style.bg, Some(theme::ADDED_BG));
        }
    }

    #[test]
    fn test_highlight_removed_line() {
        let h = Highlighter::new();
        let line = h.highlight_line("foo.rs", "let x = 42;", LineKind::Removed);
        // Should be dimmed red
        assert!(!line.spans.is_empty());
        assert_eq!(line.spans[0].style.fg, Some(Color::Red));
    }

    #[test]
    fn test_highlight_context_line() {
        let h = Highlighter::new();
        let line = h.highlight_line("foo.rs", "let x = 42;", LineKind::Context);
        // Should have syntax colors, no special background
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_highlight_empty_line() {
        let h = Highlighter::new();
        let line = h.highlight_line("foo.rs", "", LineKind::Context);
        // Should not panic
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_highlight_file_lines_basic() {
        use crate::types::{DiffLine, HunkStatus};

        let h = Highlighter::new();
        let hunks = vec![
            Hunk {
                header: "@@ -1,3 +1,4 @@".to_string(),
                lines: vec![
                    DiffLine {
                        kind: LineKind::Context,
                        content: "use std::io;\n".to_string(),
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                    },
                    DiffLine {
                        kind: LineKind::Removed,
                        content: "let x = 1;\n".to_string(),
                        old_lineno: Some(2),
                        new_lineno: None,
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        content: "let x = 42;\n".to_string(),
                        old_lineno: None,
                        new_lineno: Some(2),
                    },
                ],
                status: HunkStatus::Pending,
                old_start: 1,
                old_lines: 3,
                new_start: 1,
                new_lines: 4,
            },
            Hunk {
                header: "@@ -10,3 +11,3 @@".to_string(),
                lines: vec![DiffLine {
                    kind: LineKind::Context,
                    content: "fn main() {}\n".to_string(),
                    old_lineno: Some(10),
                    new_lineno: Some(11),
                }],
                status: HunkStatus::Pending,
                old_start: 10,
                old_lines: 3,
                new_start: 11,
                new_lines: 3,
            },
        ];

        let result = h.highlight_file_lines("foo.rs", &hunks);

        // Should have one entry per hunk
        assert_eq!(result.len(), 2);
        // First hunk has 3 lines
        assert_eq!(result[0].len(), 3);
        // Second hunk has 1 line
        assert_eq!(result[1].len(), 1);

        // Removed line (index 1 of first hunk) should be red
        let removed_line = &result[0][1];
        assert!(!removed_line.spans.is_empty());
        assert_eq!(removed_line.spans[0].style.fg, Some(Color::Red));

        // Added line (index 2 of first hunk) should have ADDED_BG
        let added_line = &result[0][2];
        assert!(!added_line.spans.is_empty());
        for span in &added_line.spans {
            assert_eq!(span.style.bg, Some(theme::ADDED_BG));
        }

        // Context lines should have syntax colors
        let context_line = &result[0][0];
        assert!(!context_line.spans.is_empty());
    }
}
