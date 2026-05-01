//! Spawn `sentinel-helper` to render the confirmation dialog and parse its
//! ALLOW / DENY / TIMEOUT verdict from stdout.
//!
//! Unlike `pam-sentinel`'s helper.rs, the agent already runs as the
//! requesting user — no fork/setuid dance needed. Just `tokio::process`.

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
}

impl Request {
    pub fn for_action(action_id: &str, message: &str) -> Self {
        Self {
            title: "Authentication Required".to_string(),
            message: if message.is_empty() {
                format!("An application is requesting authorisation for {action_id}.")
            } else {
                message.to_string()
            },
            secondary: "Click Allow to continue or Deny to cancel.".to_string(),
            timeout: 30,
            min_time: 500,
            randomize: true,
            process_exe: None,
        }
    }
}

/// Spawn the helper, await its outcome.
pub async fn run(req: Request) -> Result<Outcome, HelperError> {
    let mut cmd = Command::new(HELPER_PATH);
    cmd.arg("--title").arg(&req.title)
        .arg("--message").arg(&req.message)
        .arg("--secondary").arg(&req.secondary)
        .arg("--timeout").arg(req.timeout.to_string())
        .arg("--min-time").arg(req.min_time.to_string());
    if req.randomize {
        cmd.arg("--randomize");
    }
    if let Some(exe) = &req.process_exe {
        cmd.arg("--process-exe").arg(exe);
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
            "ALLOW" => { verdict = Some(Outcome::Allow); break; }
            "DENY" => { verdict = Some(Outcome::Deny); break; }
            "TIMEOUT" => { verdict = Some(Outcome::Timeout); break; }
            _ => continue,
        }
    }

    let _ = child.wait().await;
    verdict.ok_or(HelperError::NoOutput)
}
