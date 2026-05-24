// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bypass check over the **system D-Bus**: when `pam_sentinel.so` runs inside
//! `polkit-agent-helper-1`, ask the requesting user's agent whether this auth
//! was already approved (the user clicked Allow in the dialog). If so, return
//! `PAM_SUCCESS` directly so the dialog doesn't re-spawn.
//!
//! ## Why D-Bus and not a socket
//!
//! polkit 121+ forks the helper from polkitd; on SELinux systems (openSUSE
//! Tumbleweed) the helper runs as `policykit_t`, which is **denied writing an
//! arbitrary unix socket** (`var_run_t:sock_file write` isn't in policy). But
//! `policykit_t` *is* already allowed `dbus send_msg` to user domains — that's
//! the polkit agent protocol itself, and exactly how `pam_fprintd` does
//! passwordless auth. So talking to the agent over the system bus rides
//! existing MAC policy with no custom SELinux/AppArmor rules, on any version.
//!
//! ## Identifying the requesting user
//! - `pamh.get_user()` returns `Err` for some PAM stacks (polkit-1 via
//!   helper-1), so we fall back to `pamh.get_item::<User>()`.
//! - Our own euid is 0 inside the helper; only `PAM_USER` says whose session
//!   this auth is for.
//!
//! ## Trust model
//! - Only root may call the agent's method (enforced by the D-Bus policy in
//!   `packaging/dbus/org.sentinel.Agent.conf`), so a non-root local process
//!   can't drain the approval queue.
//! - Before trusting a reply we verify the `org.sentinel.Agent` bus name is
//!   owned by the uid we're authenticating (`GetConnectionUnixUser`), so a
//!   same-name squatter from another uid can't forge an approval.
//! - Fail-open: any error (no agent, wrong owner, refused) returns `None` and
//!   the stack falls through to the normal dialog/password flow. We never
//!   `PAM_AUTH_ERR` from here.

use pam::constants::PamResultCode;
use pam::module::PamHandle;

pub fn check_agent_bypass(pamh: &PamHandle) -> Option<PamResultCode> {
    let user = resolve_user(pamh)?;
    let uid = match nix::unistd::User::from_name(&user) {
        Ok(Some(u)) => u.uid.as_raw(),
        _ => {
            log::debug!("agent_bypass: PAM_USER={user} has no passwd entry; falling through");
            return None;
        }
    };
    log::debug!("agent_bypass: PAM_USER={user} uid={uid}");

    match query_agent(uid) {
        Ok(true) => {
            log::info!("event=auth.allow source=bypass uid={uid}");
            Some(PamResultCode::PAM_SUCCESS)
        }
        Ok(false) => None,
        Err(e) => {
            log::debug!("agent_bypass: query failed ({e}); falling through");
            None
        }
    }
}

/// Query the user's agent over the system bus. Returns `Ok(true)` only when
/// the `org.sentinel.Agent` name is owned by `uid` (anti-squat) AND the agent
/// hands back a one-shot approval.
fn query_agent(uid: u32) -> Result<bool, Box<dyn std::error::Error>> {
    use zbus::blocking::{Connection, Proxy, fdo::DBusProxy};

    let conn = Connection::system()?;

    // Anti-squat: the agent name must be owned by the user we're authing.
    let name: zbus::names::BusName = sentinel_shared::AGENT_BUS_NAME.try_into()?;
    let owner_uid = DBusProxy::new(&conn)?.get_connection_unix_user(name)?;
    if owner_uid != uid {
        log::warn!(
            "agent_bypass: {} owned by uid {owner_uid} != expected {uid}; refusing (squat?)",
            sentinel_shared::AGENT_BUS_NAME
        );
        return Ok(false);
    }

    let proxy = Proxy::new(
        &conn,
        sentinel_shared::AGENT_BUS_NAME,
        sentinel_shared::AGENT_OBJECT_PATH,
        sentinel_shared::AGENT_INTERFACE,
    )?;
    let approved: bool = proxy.call("TakeApproval", &())?;
    Ok(approved)
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
