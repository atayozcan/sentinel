// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bypass check: when `pam_sentinel.so` is loaded inside
//! `polkit-agent-helper-1` (whether socket-activated by systemd or
//! invoked directly by an agent), connect to
//! `$XDG_RUNTIME_DIR/sentinel-agent.sock` for the requesting user. The
//! agent there has already shown the Sentinel dialog and pre-approved
//! this auth attempt; reading "OK\n" from the socket means we should
//! return `PAM_SUCCESS` directly so the dialog doesn't re-spawn.
//!
//! Identifying the requesting user:
//! - `pamh.get_user()` returns `Err(PAM_SUCCESS)` for some PAM stacks
//!   (notably polkit-1 from `polkit-agent-helper-1`), so we also try
//!   `pamh.get_item::<User>()` as a fallback.
//! - Inside `polkit-agent-helper-1` (socket-activated, runs as root)
//!   our own `geteuid()` is 0; only `PAM_USER` tells us whose session
//!   this auth is for.
//!
//! Trust model:
//! - The socket is owned by the requesting user (mode `0600`). Cross-uid
//!   attackers can't connect.
//! - Same-uid attackers can connect, but they can also already drive
//!   polkit directly. Sentinel is a UI confirmation, not a sandbox.
//! - The agent verifies its peer (us) via `SO_PEERCRED` + `/proc/<pid>/comm`
//!   so a same-uid attacker can't forge a "polkit-agent-helper-1"
//!   identity by connecting from a different binary.
//!
//! Fail-open: any failure (no socket, refused, NO response, timeout,
//! malformed) returns `None` so the caller falls through to the normal
//! dialog flow. We never `PAM_AUTH_ERR` from here — confused/missing
//! agent state must not block legitimate auth.
//!
//! ## Sandbox compatibility
//!
//! On modern systemd-socket-activated polkit setups,
//! `polkit-agent-helper@.service` ships with `ProtectHome=yes` which
//! masks `/run/user/<uid>` inside the sandbox. The `pam_sentinel`
//! installer drops a unit override to disable this so `pam_sentinel.so`
//! can reach our socket. See
//! `packaging/systemd/polkit-agent-helper@.service.d/sentinel.conf`.

use pam::constants::PamResultCode;
use pam::module::PamHandle;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const READ_TIMEOUT: Duration = Duration::from_millis(500);

pub fn check_agent_bypass(pamh: &PamHandle) -> Option<PamResultCode> {
    let user = resolve_user(pamh)?;
    let uid = match nix::unistd::User::from_name(&user) {
        Ok(Some(u)) => u.uid.as_raw(),
        _ => {
            log::debug!("agent_bypass: PAM_USER={user} has no passwd entry; falling through");
            return None;
        }
    };
    let path = sentinel_shared::bypass_socket_path(uid);

    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(e) => {
            log::debug!(
                "agent_bypass: cannot connect to {} ({e}); falling through",
                path.display()
            );
            return None;
        }
    };
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));

    if let Err(e) = stream.write_all(b"?\n") {
        log::debug!("agent_bypass: write failed: {e}");
        return None;
    }
    let _ = stream.flush();

    let mut buf = [0u8; 4];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(e) => {
            log::debug!("agent_bypass: read failed: {e}");
            return None;
        }
    };
    let resp = std::str::from_utf8(&buf[..n]).unwrap_or("").trim();
    match resp {
        "OK" => {
            log::info!("event=auth.allow source=bypass uid={uid}");
            Some(PamResultCode::PAM_SUCCESS)
        }
        other => {
            log::warn!("agent_bypass: agent declined (resp={other:?}); falling through to dialog");
            None
        }
    }
}

fn resolve_user(pamh: &PamHandle) -> Option<String> {
    if let Ok(s) = pamh.get_user(None) {
        return Some(s);
    }
    pamh.get_item::<pam::items::User>()
        .ok()
        .flatten()
        .and_then(|s| s.to_str().ok().map(str::to_owned))
}
