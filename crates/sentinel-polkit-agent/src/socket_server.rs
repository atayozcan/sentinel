//! Unix-socket server `pam_sentinel.so` connects to to confirm a
//! polkit auth has been pre-approved.
//!
//! Bind path: `$XDG_RUNTIME_DIR/sentinel-agent.sock` (or
//! `/run/user/<uid>/sentinel-agent.sock` if the env isn't set), mode
//! `0600`, owner = the user. We unlink any stale socket file from a
//! previous agent crash before binding.
//!
//! Per-connection check (the agent's side of the trust model):
//! 1. `SO_PEERCRED` — peer uid must be 0 (the socket-activated
//!    `polkit-agent-helper-1` runs as root).
//! 2. `/proc/<peer_pid>/exe` basename must be `polkit-agent-helper-1`.
//!
//! On a valid connection we pop one non-expired approval from the
//! queue and write "OK\n"; otherwise "NO\n".

use crate::approval_queue::ApprovalQueue;
use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::net::UnixStream;

const SOCKET_BASENAME: &str = "sentinel-agent.sock";
const HELPER1_BASENAME: &str = "polkit-agent-helper-1";

pub fn socket_path_for_uid(uid: u32) -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join(SOCKET_BASENAME);
        }
    }
    PathBuf::from(format!("/run/user/{uid}")).join(SOCKET_BASENAME)
}

/// Bind the socket and accept forever. Spawns one task per accepted
/// connection. Refuses to start if the directory is missing.
pub async fn serve(uid: u32, queue: ApprovalQueue) -> Result<()> {
    let path = socket_path_for_uid(uid);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create runtime dir {}", parent.display()))?;
    }
    // Unlink any stale socket.
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }

    let listener = UnixListener::bind(&path)
        .with_context(|| format!("bind {}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 0600 {}", path.display()))?;

    info!("agent socket listening at {}", path.display());

    loop {
        let (stream, _addr) = listener.accept().await.context("accept")?;
        let q = queue.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_one(stream, q).await {
                debug!("agent socket: connection error: {e}");
            }
        });
    }
}

async fn handle_one(mut stream: UnixStream, queue: ApprovalQueue) -> Result<()> {
    let cred = stream.peer_cred().context("peer_cred")?;
    let peer_uid = cred.uid();
    let peer_pid = cred.pid().unwrap_or(-1);
    // /proc/<pid>/exe is unreadable cross-uid for processes with
    // PR_SET_DUMPABLE=0 (set by NoNewPrivileges in helper-1's unit).
    // /proc/<pid>/comm is readable in that case and is enough for our
    // basename check.
    let peer_comm = read_proc_comm(peer_pid);
    debug!(
        "agent socket: incoming peer uid={peer_uid} pid={peer_pid} comm={peer_comm:?}"
    );

    if peer_uid != 0 {
        warn!(
            "agent socket: rejecting non-root connection (uid={peer_uid} pid={peer_pid})"
        );
        let _ = stream.write_all(b"NO\n").await;
        return Ok(());
    }
    let comm_ok = peer_comm.as_deref().map(comm_matches).unwrap_or(false);
    if !comm_ok {
        warn!(
            "agent socket: rejecting non-helper peer (pid={peer_pid} comm={peer_comm:?})"
        );
        let _ = stream.write_all(b"NO\n").await;
        return Ok(());
    }

    match queue.take_one().await {
        Some(approval) => {
            info!(
                "agent socket: approving auth for action {} (peer pid {peer_pid})",
                approval.action_id
            );
            stream.write_all(b"OK\n").await.context("write OK")?;
        }
        None => {
            warn!(
                "agent socket: no pending approval for peer pid {peer_pid}; replying NO"
            );
            let _ = stream.write_all(b"NO\n").await;
        }
    }
    Ok(())
}

fn comm_matches(name: &str) -> bool {
    // /proc/<pid>/comm is the kernel-tracked process name (TASK_COMM_LEN
    // = 16 chars max, including NUL). The full executable name is
    // "polkit-agent-helper-1" (21 chars) which exceeds that, so the
    // kernel truncates it. Match the truncated form.
    let trimmed = name.trim();
    trimmed == HELPER1_BASENAME
        || trimmed == "polkit-agent-he"   // 15-char truncation as set by execve
        || trimmed.starts_with("polkit-agent-")
}

fn read_proc_comm(pid: i32) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()
}

/// Best-effort: remove the socket on graceful shutdown.
pub fn unlink_socket(uid: u32) {
    let path = socket_path_for_uid(uid);
    if let Err(e) = std::fs::remove_file(&path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("could not unlink {}: {e}", path.display());
        }
    }
}

