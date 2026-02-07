use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::types::LineKind;
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
}
