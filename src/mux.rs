use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::io::ErrorKind;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cmux,
    Tmux,
    FullScreen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmuxTarget {
    pub workspace_ref: String,
    pub surface_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitHandle {
    Cmux(CmuxTarget),
    TmuxPane(String),
    Immediate,
}

#[derive(Debug, Deserialize)]
struct CmuxIdentifyOutput {
    caller: Option<CmuxIdentifyCaller>,
}

#[derive(Debug, Deserialize)]
struct CmuxIdentifyCaller {
    workspace_ref: String,
    surface_ref: String,
}

pub fn select_backend(cmux_active: bool, tmux_active: bool) -> Backend {
    if cmux_active {
        Backend::Cmux
    } else if tmux_active {
        Backend::Tmux
    } else {
        Backend::FullScreen
    }
}

pub fn detect_backend() -> Result<Backend> {
    let cmux_active = current_cmux_target()?.is_some();
    let tmux_active = std::env::var("TMUX").is_ok();
    Ok(select_backend(cmux_active, tmux_active))
}

pub fn current_cmux_target() -> Result<Option<CmuxTarget>> {
    let output = match Command::new("cmux").arg("identify").output() {
        Ok(output) => output,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("Failed to run `cmux identify`"),
    };

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_cmux_identify_output(&stdout)
}

pub fn parse_cmux_identify_output(output: &str) -> Result<Option<CmuxTarget>> {
    let parsed: CmuxIdentifyOutput =
        serde_json::from_str(output).context("Failed to parse `cmux identify` output")?;

    Ok(parsed.caller.map(|caller| CmuxTarget {
        workspace_ref: caller.workspace_ref,
        surface_ref: caller.surface_ref,
    }))
}

pub fn parse_cmux_new_split_output(output: &str) -> Result<CmuxTarget> {
    let mut workspace_ref = None;
    let mut surface_ref = None;

    for token in output.split_whitespace() {
        if token.starts_with("workspace:") {
            workspace_ref = Some(token.to_string());
        } else if token.starts_with("surface:") {
            surface_ref = Some(token.to_string());
        }
    }

    match (workspace_ref, surface_ref) {
        (Some(workspace_ref), Some(surface_ref)) => Ok(CmuxTarget {
            workspace_ref,
            surface_ref,
        }),
        _ => bail!("Failed to parse cmux split output: {}", output.trim()),
    }
}

pub fn build_tmux_split_command(argv: &[String]) -> Vec<String> {
    let mut cmd = vec![
        "tmux".to_string(),
        "split-window".to_string(),
        "-h".to_string(),
        "-p".to_string(),
        "50".to_string(),
        "-P".to_string(),
        "-F".to_string(),
        "#{pane_id}".to_string(),
        "--".to_string(),
    ];
    cmd.extend(argv.iter().cloned());
    cmd
}

pub fn build_cmux_respawn_command(target: &CmuxTarget, argv: &[String]) -> Vec<String> {
    vec![
        "cmux".to_string(),
        "respawn-pane".to_string(),
        "--workspace".to_string(),
        target.workspace_ref.clone(),
        "--surface".to_string(),
        target.surface_ref.clone(),
        "--command".to_string(),
        build_shell_command(argv),
    ]
}

pub fn open_command_in_preferred_split(argv: &[String]) -> Result<Option<SplitHandle>> {
    match detect_backend()? {
        Backend::Cmux => {
            let caller = current_cmux_target()?.context("cmux backend selected without caller")?;
            Ok(Some(open_cmux_split(&caller, argv)?))
        }
        Backend::Tmux => Ok(Some(open_tmux_split(argv)?)),
        Backend::FullScreen => Ok(None),
    }
}

pub fn handle_exists(handle: &SplitHandle) -> bool {
    match handle {
        SplitHandle::Cmux(target) => cmux_surface_exists(target),
        SplitHandle::TmuxPane(pane_id) => tmux_pane_exists(pane_id),
        SplitHandle::Immediate => false,
    }
}

pub fn poll_handle_close(handle: &SplitHandle, max_iterations: u32, interval: Duration) -> bool {
    for _ in 0..max_iterations {
        if !handle_exists(handle) {
            return true;
        }
        std::thread::sleep(interval);
    }

    false
}

fn open_tmux_split(argv: &[String]) -> Result<SplitHandle> {
    let cmd = build_tmux_split_command(argv);
    let output = Command::new(&cmd[0])
        .args(&cmd[1..])
        .output()
        .context("Failed to run tmux split-window")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("tmux split-window failed: {}", stderr.trim());
    }

    let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if pane_id.is_empty() {
        bail!("tmux split-window did not return a pane ID");
    }

    Ok(SplitHandle::TmuxPane(pane_id))
}

fn open_cmux_split(caller: &CmuxTarget, argv: &[String]) -> Result<SplitHandle> {
    let create_output = Command::new("cmux")
        .args([
            "new-split",
            "right",
            "--workspace",
            &caller.workspace_ref,
            "--surface",
            &caller.surface_ref,
        ])
        .output()
        .context("Failed to run `cmux new-split`")?;

    if !create_output.status.success() {
        let stderr = String::from_utf8_lossy(&create_output.stderr);
        bail!("cmux new-split failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&create_output.stdout);
    let target = parse_cmux_new_split_output(&stdout)?;

    let cmd = build_cmux_respawn_command(&target, argv);
    let output = Command::new(&cmd[0])
        .args(&cmd[1..])
        .output()
        .context("Failed to run `cmux respawn-pane`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("cmux respawn-pane failed: {}", stderr.trim());
    }

    Ok(SplitHandle::Cmux(target))
}

fn tmux_pane_exists(pane_id: &str) -> bool {
    let output = Command::new("tmux")
        .args(["list-panes", "-a", "-F", "#{pane_id}"])
        .output();

    match output {
        Ok(output) => {
            let pane_list = String::from_utf8_lossy(&output.stdout);
            pane_list.lines().any(|line| line.trim() == pane_id)
        }
        Err(_) => false,
    }
}

fn cmux_surface_exists(target: &CmuxTarget) -> bool {
    match Command::new("cmux")
        .args([
            "read-screen",
            "--workspace",
            &target.workspace_ref,
            "--surface",
            &target.surface_ref,
            "--lines",
            "1",
        ])
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

fn build_shell_command(argv: &[String]) -> String {
    let mut command = String::from("exec");
    for arg in argv {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    command
}

fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', r#"'"'"'"#))
}
