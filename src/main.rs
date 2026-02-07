use anyhow::{bail, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "stagent",
    about = "Interactive TUI code review tool for staged diffs"
)]
pub struct Cli {
    /// Write feedback output to a file instead of stdout
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Don't actually stage hunks (review-only mode)
    #[arg(long)]
    no_stage: bool,

    /// Only show files matching this glob pattern
    #[arg(long, value_name = "GLOB")]
    files: Option<String>,

    /// Color theme name
    #[arg(long, default_value = "default")]
    theme: String,

    /// Number of context lines to show around changes in comment feedback
    #[arg(short = 'C', long = "context-lines", default_value_t = stagent::feedback::DEFAULT_CONTEXT_LINES)]
    context_lines: usize,

    /// Add untracked files with intent-to-add so they appear in the diff
    #[arg(short = 'N', long = "intent-to-add")]
    intent_to_add: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Check tmux
    if std::env::var("TMUX").is_err() {
        bail!("stagent requires tmux. Please run inside a tmux session.");
    }

    // Open repo
    let repo = stagent::git::open_repo(".")?;

    // Intent-to-add untracked files if requested
    if cli.intent_to_add {
        stagent::git::intent_to_add_untracked(&repo)?;
    }

    // Get unstaged diff
    let mut files = stagent::git::get_unstaged_diff(&repo)?;

    // Filter by glob if specified
    if let Some(ref glob_pattern) = cli.files {
        match glob::Pattern::new(glob_pattern) {
            Ok(pattern) => {
                files.retain(|f| pattern.matches_path(&f.path));
            }
            Err(e) => {
                eprintln!("Warning: invalid glob pattern '{}': {}", glob_pattern, e);
            }
        }
    }

    // Filter out binary files
    files.retain(|f| {
        if f.is_binary {
            eprintln!("Skipping binary file: {}", f.path.display());
            false
        } else {
            true
        }
    });

    if files.is_empty() {
        println!("No unstaged changes to review.");
        return Ok(());
    }

    // Run TUI
    let feedback = stagent::app::run(files, &repo, cli.no_stage)?;

    // Output feedback
    if !feedback.is_empty() {
        let output = stagent::feedback::format_feedback(&feedback, cli.context_lines);
        stagent::feedback::write_feedback(&output, cli.output.as_deref())?;
    }

    Ok(())
}
