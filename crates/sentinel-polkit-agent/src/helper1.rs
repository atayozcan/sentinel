//! Spawn `polkit-agent-helper-1` (setuid root, ships with polkit) to
//! satisfy polkit's cookie validation.
//!
//! Wire protocol of helper-1: argv is `polkit-agent-helper-1 <username>`,
//! the cookie is sent as the first line on stdin. helper-1 emits PAM
//! prompts and info lines on stdout (`PAM_PROMPT_ECHO_OFF <prompt>`,
//! `PAM_TEXT_INFO <msg>`, `PAM_ERROR_MSG <msg>`) and finally `SUCCESS` or
//! `FAILURE`. Because `pam_sentinel` returns `PAM_SUCCESS` immediately
//! when the bypass token validates, no real prompts arrive — we just
//! consume stdout until the terminator.

use anyhow::{Context, Result, anyhow, bail};
use log::{debug, warn};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const CANDIDATES_RAW: &str = env!("POLKIT_AGENT_HELPER_CANDIDATES");

fn locate() -> Result<&'static str> {
    for cand in CANDIDATES_RAW.split(';').filter(|s| !s.is_empty()) {
        if Path::new(cand).is_file() {
            // SAFETY of the `'static` cast: CANDIDATES_RAW lives for the
            // program's lifetime (it's an env! literal baked into rodata),
            // so `cand` does too.
            let static_str: &'static str = unsafe { &*(cand as *const str) };
            return Ok(static_str);
        }
    }
    bail!(
        "polkit-agent-helper-1 not found in any of: {}",
        CANDIDATES_RAW
    )
}

pub struct Run<'a> {
    pub username: &'a str,
    pub cookie: &'a str,
    pub auth_token_b64: &'a str,
    pub action_id: &'a str,
}

/// Spawn polkit-agent-helper-1 and wait for SUCCESS/FAILURE.
pub async fn run(args: Run<'_>) -> Result<bool> {
    let path = locate().context("locate polkit-agent-helper-1")?;
    debug!("invoking {path} for {}", args.username);

    let mut child = Command::new(path)
        .arg(args.username)
        .env("SENTINEL_AGENT_AUTH", args.auth_token_b64)
        .env("SENTINEL_AGENT_COOKIE", args.cookie)
        .env("SENTINEL_AGENT_ACTION_ID", args.action_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {path}"))?;

    {
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        stdin.write_all(args.cookie.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        // Drop stdin so helper-1 sees EOF if it ever asks for more.
    }

    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let mut lines = BufReader::new(stdout).lines();

    let mut verdict: Option<bool> = None;
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("PAM_") {
            // PAM_PROMPT_ECHO_OFF / ON / TEXT_INFO / ERROR_MSG. With our
            // bypass the stack should never prompt — log if it does so
            // we can debug.
            debug!("helper-1 said: PAM_{rest}");
            continue;
        }
        match trimmed {
            "SUCCESS" => { verdict = Some(true); break; }
            "FAILURE" => { verdict = Some(false); break; }
            other => warn!("helper-1 emitted unknown line: {other}"),
        }
    }

    let status = child.wait().await.context("wait helper-1")?;
    match verdict {
        Some(v) => Ok(v),
        None => {
            warn!("helper-1 exited {status} without SUCCESS/FAILURE");
            Ok(status.success())
        }
    }
}
