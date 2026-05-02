//! Talk to systemd's socket-activated `polkit-agent-helper-1` to
//! satisfy polkit's cookie validation.
//!
//! Modern systemd-based distros ship `polkit-agent-helper-1` mode `755`
//! (NOT setuid) and route auth via `/run/polkit/agent-helper.socket`.
//! The socket is `Accept=yes`, so each connection spawns a per-request
//! `polkit-agent-helper@<n>.service` running helper-1 as root.
//!
//! Wire protocol (empirically determined; matches polkit upstream
//! `--socket-activated` mode):
//! ```text
//! → <user>\n<cookie>\n
//! ← PAM_PROMPT_ECHO_OFF Password:    (or other PAM_* lines, ignored)
//! ← PAM_TEXT_INFO ...
//! ← PAM_ERROR_MSG ...
//! ← SUCCESS    or    FAILURE
//! ```
//!
//! Because `pam_sentinel.so` short-circuits to `PAM_SUCCESS` via the
//! agent socket bypass, the PAM stack inside helper-1 should never
//! prompt — we just consume the stream until `SUCCESS`/`FAILURE`.

use anyhow::{Context, Result};
use log::{debug, warn};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const HELPER_SOCKET_PATH: &str = "/run/polkit/agent-helper.socket";

pub struct Run<'a> {
    pub username: &'a str,
    pub cookie: &'a str,
}

pub async fn run(args: Run<'_>) -> Result<bool> {
    if !Path::new(HELPER_SOCKET_PATH).exists() {
        anyhow::bail!(
            "polkit helper socket {HELPER_SOCKET_PATH} not found — \
             this build only supports systemd-socket-activated polkit"
        );
    }

    let stream = UnixStream::connect(HELPER_SOCKET_PATH)
        .await
        .with_context(|| format!("connect {HELPER_SOCKET_PATH}"))?;

    let (reader, mut writer) = stream.into_split();
    writer
        .write_all(format!("{}\n{}\n", args.username, args.cookie).as_bytes())
        .await
        .context("write user/cookie")?;
    writer.flush().await.context("flush")?;

    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let mut verdict: Option<bool> = None;

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await.context("read helper-1")?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("PAM_") {
            debug!("helper-1 said PAM_{rest}");
            if rest.starts_with("PROMPT_ECHO_OFF") || rest.starts_with("PROMPT_ECHO_ON") {
                warn!(
                    "helper-1 prompted for input we can't supply — \
                     pam_sentinel.so didn't approve. Sending empty reply \
                     so the stack fails out cleanly."
                );
                let _ = writer.write_all(b"\n").await;
                let _ = writer.flush().await;
            }
        } else if trimmed == "SUCCESS" {
            verdict = Some(true);
            break;
        } else if trimmed == "FAILURE" {
            verdict = Some(false);
            break;
        } else {
            warn!("helper-1 emitted unknown line: {trimmed}");
        }
    }

    Ok(verdict.unwrap_or(false))
}
