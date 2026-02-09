use anyhow::{Result, bail};
use clap::Parser;
use git2::Repository;
use std::path::PathBuf;

use stagent::types::FileDiff;

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

    /// Spawn stagent in a tmux split pane and wait for completion
    #[arg(long)]
    spawn: bool,

    /// Read a unified diff from stdin instead of computing one from git
    #[arg(short = 'p', long = "patch")]
    patch: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Check tmux
    if std::env::var("TMUX").is_err() {
        bail!("stagent requires tmux. Please run inside a tmux session.");
    }

    // --patch + --spawn is not supported (stdin can't be forwarded through tmux split)
    if cli.patch && cli.spawn {
        bail!(
            "--patch and --spawn cannot be used together (stdin cannot be forwarded through a tmux split)"
        );
    }

    // Handle --spawn mode: spawn stagent in a split and wait for completion
    if cli.spawn {
        let opts = stagent::spawn::SpawnOptions {
            output: cli.output.clone(),
            files: cli.files.clone(),
            theme: cli.theme.clone(),
            context_lines: cli.context_lines,
            no_stage: cli.no_stage,
        };
        return stagent::spawn::spawn_in_split(&opts);
    }

    if cli.patch {
        return run_patch_mode(&cli);
    }

    run_git_mode(&cli)
}

/// Maximum patch input size (100 MB). Prevents OOM from unbounded stdin.
const MAX_PATCH_SIZE: u64 = 100 * 1024 * 1024;

/// Run in patch mode: read a unified diff from stdin and review it.
fn run_patch_mode(cli: &Cli) -> Result<()> {
    use std::io::{IsTerminal, Read};

    if std::io::stdin().is_terminal() {
        bail!("--patch requires piped input. Usage: git diff | stagent -p");
    }

    let mut input = String::new();
    std::io::stdin()
        .take(MAX_PATCH_SIZE + 1)
        .read_to_string(&mut input)?;
    if input.len() as u64 > MAX_PATCH_SIZE {
        bail!(
            "Patch input exceeds maximum size ({} MB)",
            MAX_PATCH_SIZE / (1024 * 1024)
        );
    }
    let files = stagent::patch::parse_unified_diff(&input)?;

    // Staging is disabled in patch mode — no git repo context
    run_review_pipeline(files, None, true, "No changes to review.", cli)
}

/// Run in normal git mode: compute diff from working tree and review/stage.
fn run_git_mode(cli: &Cli) -> Result<()> {
    let repo = stagent::git::open_repo(".")?;

    // Add untracked files with intent-to-add so they appear in the diff
    // and can be staged hunk-by-hunk.
    stagent::git::intent_to_add_untracked(&repo)?;

    let files = stagent::git::get_unstaged_diff(&repo)?;

    run_review_pipeline(
        files,
        Some(&repo),
        cli.no_stage,
        "No unstaged changes to review.",
        cli,
    )
}

/// Shared pipeline: filter files, run TUI, write feedback.
fn run_review_pipeline(
    mut files: Vec<FileDiff>,
    repo: Option<&Repository>,
    no_stage: bool,
    empty_message: &str,
    cli: &Cli,
) -> Result<()> {
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
        println!("{}", empty_message);
        return Ok(());
    }

    let feedback = stagent::app::run(files, repo, no_stage)?;

    if !feedback.is_empty() {
        let output = stagent::feedback::format_feedback(&feedback, cli.context_lines);
        stagent::feedback::write_feedback(&output, cli.output.as_deref())?;
    }

    Ok(())
}
