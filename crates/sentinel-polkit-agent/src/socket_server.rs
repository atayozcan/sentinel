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
//! 2. `/proc/<peer_pid>/comm` must equal `polkit-agent-helper-1` or
//!    its 15-char kernel truncation `polkit-agent-he`. We read `comm`
//!    rather than `exe` because helper-1's systemd sandbox
//!    (`NoNewPrivileges`) sets `PR_SET_DUMPABLE=0`, making `exe`
//!    unreadable cross-uid.
//!
//! On a valid connection we pop one non-expired approval from the
//! queue and write "OK\n"; otherwise "NO\n".

use crate::approval_queue::ApprovalQueue;
use anyhow::{Context, Result};
use log::{debug, info, warn};
use sentinel_shared::{bypass_socket_path, procfs};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::net::UnixStream;

const HELPER1_BASENAME: &str = "polkit-agent-helper-1";

/// Bind the socket and accept forever. Spawns one task per accepted
/// connection. Refuses to start if the directory is missing.
pub async fn serve(uid: u32, queue: ApprovalQueue) -> Result<()> {
    let path = bypass_socket_path(uid);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create runtime dir {}", parent.display()))?;
    }
    // Unlink any stale socket. We only `remove_file` if it really *is*
    // a socket — refusing to clobber a regular file that someone (or
    // some misconfigured agent) planted at the same path. If a stray
    // non-socket exists we let `bind()` fail loudly with a clear
    // message rather than silently rm-ing it.
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_socket() {
            let _ = std::fs::remove_file(&path);
        } else {
            warn!(
                "{} exists but is not a socket ({:?}); not unlinking — \
                 bind will fail and you'll need to investigate manually",
                path.display(),
                metadata.file_type()
            );
        }
    }

    let listener = UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))?;
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
    let peer_comm = procfs::read_comm(peer_pid);
    debug!("agent socket: incoming peer uid={peer_uid} pid={peer_pid} comm={peer_comm:?}");

    if peer_uid != 0 {
        warn!("agent socket: rejecting non-root connection (uid={peer_uid} pid={peer_pid})");
        let _ = stream.write_all(b"NO\n").await;
        return Ok(());
    }
    let comm_ok = peer_comm.as_deref().map(comm_matches).unwrap_or(false);
    if !comm_ok {
        warn!("agent socket: rejecting non-helper peer (pid={peer_pid} comm={peer_comm:?})");
        let _ = stream.write_all(b"NO\n").await;
        return Ok(());
    }

    match queue.take_one().await {
        Some(approval) => {
            info!(
                "event=auth.allow source=agent.bypass action={} peer_pid={peer_pid}",
                sentinel_shared::log_kv::quote(&approval.action_id)
            );
            stream.write_all(b"OK\n").await.context("write OK")?;
        }
        None => {
            warn!("agent socket: no pending approval for peer pid {peer_pid}; replying NO");
            let _ = stream.write_all(b"NO\n").await;
        }
    }
    Ok(())
}

fn comm_matches(name: &str) -> bool {
    // /proc/<pid>/comm is the kernel-tracked process name (TASK_COMM_LEN
    // = 16 chars max, including NUL). The full executable name is
    // "polkit-agent-helper-1" (21 chars) which exceeds that, so the
    // kernel truncates it to 15 chars. We accept either form, exactly,
    // and nothing else — the previous `starts_with("polkit-agent-")`
    // matched arbitrary tools (polkit-agent-foo, polkit-agent-debugger)
    // which is broader than necessary. SO_PEERCRED already constrains
    // the peer to root, but defense in depth is cheap.
    const HELPER1_COMM_TRUNCATED: &str = "polkit-agent-he";
    let trimmed = name.trim();
    trimmed == HELPER1_BASENAME || trimmed == HELPER1_COMM_TRUNCATED
}

/// Best-effort: remove the socket on graceful shutdown.
pub fn unlink_socket(uid: u32) {
    let path = bypass_socket_path(uid);
    if let Err(e) = std::fs::remove_file(&path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("could not unlink {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comm_matches_full_name() {
        assert!(comm_matches("polkit-agent-helper-1"));
    }

    #[test]
    fn comm_matches_kernel_truncation() {
        assert!(comm_matches("polkit-agent-he"));
    }

    #[test]
    fn comm_matches_strips_trailing_newline() {
        // /proc/<pid>/comm always ends with \n.
        assert!(comm_matches("polkit-agent-helper-1\n"));
    }

    #[test]
    fn comm_matches_rejects_other_polkit_tools() {
        assert!(!comm_matches("polkit-agent-foo"));
        assert!(!comm_matches("polkit-agent-debugger"));
        assert!(!comm_matches("polkitd"));
    }

    #[test]
    fn comm_matches_rejects_unrelated() {
        assert!(!comm_matches("bash"));
        assert!(!comm_matches(""));
        assert!(!comm_matches("polkit"));
    }
}
