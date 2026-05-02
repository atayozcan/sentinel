//! Spawn `sentinel-helper` to render the confirmation dialog and parse its
//! ALLOW / DENY / TIMEOUT verdict from stdout.
//!
//! Unlike `pam-sentinel`'s helper.rs, the agent already runs as the
//! requesting user — no fork/setuid dance needed. Just `tokio::process`.

use sentinel_config::{ServiceConfig, format_message};
use std::process::Stdio;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const HELPER_PATH: &str = env!("SENTINEL_HELPER_PATH");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Allow,
    Deny,
    Timeout,
}

#[derive(Debug, Error)]
pub enum HelperError {
    #[error("spawn {HELPER_PATH}: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("helper produced no verdict")]
    NoOutput,
    #[error("helper i/o: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Request {
    pub title: String,
    pub message: String,
    pub secondary: String,
    pub timeout: u64,
    pub min_time: u64,
    pub randomize: bool,
    pub process_exe: Option<String>,
    pub process_cmdline: Option<String>,
    pub process_pid: Option<i32>,
    pub process_cwd: Option<String>,
    pub requesting_user: Option<String>,
    pub action: Option<String>,
}

pub struct ForAction<'a> {
    pub action_id: &'a str,
    /// Effective config for the `polkit-1` PAM service. Drives
    /// title/timeout/randomize and provides the message/secondary
    /// templates.
    pub cfg: &'a ServiceConfig,
    /// Username being authenticated. Substituted as `%u` in templates.
    pub username: &'a str,
    pub process_exe: Option<&'a str>,
    pub process_cmdline: Option<&'a str>,
    pub process_pid: Option<i32>,
    pub process_cwd: Option<&'a str>,
    pub requesting_user: Option<&'a str>,
}

impl Request {
    /// Build a [`Request`] from a polkit `BeginAuthentication` invocation
    /// combined with the loaded `polkit-1` config. Token substitution
    /// (`%u`/`%s`/`%p`) is applied to title/message/secondary.
    ///
    /// We deliberately ignore polkit's per-action message string
    /// (`message` arg of `BeginAuthentication`). Polkit localizes it
    /// via gettext and the user's glibc locale install state, which is
    /// out of our control — on systems where the requested locale
    /// isn't fully installed at the glibc level, polkit silently falls
    /// back to English even when our helper has the locale baked in.
    /// Always using the templated config message keeps the dialog's
    /// language consistent. Admins who want polkit's per-action
    /// phrasing can disable Sentinel for that specific action via PAM
    /// config; the action_id is also surfaced in the expandable
    /// details panel.
    pub fn for_action(args: ForAction<'_>) -> Self {
        // Process name for %p — basename of the requesting exe path.
        // Falls back to "unknown" so substitutions never produce empty
        // tokens that look like a bug.
        let process_name = args
            .process_exe
            .and_then(|p| std::path::Path::new(p).file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        const SERVICE: &str = "polkit-1";
        let title = format_message(&args.cfg.title, args.username, SERVICE, process_name);
        let body = format_message(&args.cfg.message, args.username, SERVICE, process_name);
        let secondary = format_message(&args.cfg.secondary, args.username, SERVICE, process_name);

        Self {
            title,
            message: body,
            secondary,
            timeout: args.cfg.timeout as u64,
            min_time: args.cfg.min_display_time_ms as u64,
            randomize: args.cfg.randomize_buttons,
            process_exe: args.process_exe.map(str::to_string),
            process_cmdline: args.process_cmdline.map(str::to_string),
            process_pid: args.process_pid,
            process_cwd: args.process_cwd.map(str::to_string),
            requesting_user: args.requesting_user.map(str::to_string),
            action: Some(args.action_id.to_string()),
        }
    }
}

/// Spawn the helper, await its outcome.
pub async fn run(req: Request) -> Result<Outcome, HelperError> {
    let mut cmd = Command::new(HELPER_PATH);
    cmd.arg("--title")
        .arg(&req.title)
        .arg("--message")
        .arg(&req.message)
        .arg("--secondary")
        .arg(&req.secondary)
        .arg("--timeout")
        .arg(req.timeout.to_string())
        .arg("--min-time")
        .arg(req.min_time.to_string());
    if req.randomize {
        cmd.arg("--randomize");
    }
    if let Some(exe) = &req.process_exe {
        cmd.arg("--process-exe").arg(exe);
    }
    if let Some(cmdline) = &req.process_cmdline {
        cmd.arg("--process-cmdline").arg(cmdline);
    }
    if let Some(pid) = req.process_pid {
        cmd.arg("--process-pid").arg(pid.to_string());
    }
    if let Some(cwd) = &req.process_cwd {
        cmd.arg("--process-cwd").arg(cwd);
    }
    if let Some(user) = &req.requesting_user {
        cmd.arg("--requesting-user").arg(user);
    }
    if let Some(action) = &req.action {
        cmd.arg("--action").arg(action);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(HelperError::Spawn)?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut lines = BufReader::new(stdout).lines();

    let mut verdict: Option<Outcome> = None;
    while let Some(line) = lines.next_line().await? {
        match line.trim() {
            "ALLOW" => {
                verdict = Some(Outcome::Allow);
                break;
            }
            "DENY" => {
                verdict = Some(Outcome::Deny);
                break;
            }
            "TIMEOUT" => {
                verdict = Some(Outcome::Timeout);
                break;
            }
            _ => continue,
        }
    }

    let _ = child.wait().await;
    verdict.ok_or(HelperError::NoOutput)
}
