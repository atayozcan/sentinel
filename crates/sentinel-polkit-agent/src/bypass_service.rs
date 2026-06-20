// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! System-bus interface that `pam_sentinel.so` calls to consume a one-shot
//! pre-approval (replaces the old `/run/...` unix-socket server).
//!
//! Why D-Bus: polkit 121+ forks `polkit-agent-helper-1` from polkitd, and on
//! SELinux systems the helper runs as `policykit_t`, which is denied writing
//! an arbitrary unix socket — but `policykit_t` is already allowed
//! `dbus send_msg` to user domains (that's the polkit agent protocol itself,
//! and how `pam_fprintd` works). So this rides existing MAC policy with no
//! custom rules.
//!
//! Trust model:
//! - Callers are restricted to root by the D-Bus policy shipped at
//!   `packaging/dbus/org.sentinel.Agent.conf` (only root may
//!   `send_destination=org.sentinel.Agent`), so a non-root local process
//!   can't drain the queue.
//! - `pam_sentinel` independently verifies that this service's bus name is
//!   owned by the uid it's authenticating before trusting a reply, so a
//!   same-name squatter can't forge an approval. See `agent_bypass.rs`.

use crate::approval_queue::ApprovalQueue;
use log::{info, warn};
use sentinel_shared::log_kv::quote as q;

pub struct BypassService {
    pub queue: ApprovalQueue,
}

// NOTE: the interface name must equal `sentinel_shared::AGENT_INTERFACE`
// ("org.sentinel.Agent"); the macro needs a string literal.
#[zbus::interface(name = "org.sentinel.Agent")]
impl BypassService {
    /// Consume one non-expired pre-approval (pushed when the user clicked
    /// Allow). Returns `true` and consumes it, or `false` if none is pending.
    /// The D-Bus policy restricts callers to root, so we don't re-check here.
    async fn take_approval(&self) -> bool {
        match self.queue.take_one().await {
            Some(a) => {
                info!(
                    "event=auth.allow source=agent.bypass action={}",
                    q(&a.action_id)
                );
                true
            }
            None => {
                warn!("agent.bypass: TakeApproval with no pending approval; replying false");
                false
            }
        }
    }
}
