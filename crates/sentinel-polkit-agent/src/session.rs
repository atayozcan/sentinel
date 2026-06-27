// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! One in-flight authentication: drive `sentinel-helper` for the user
//! decision, then satisfy polkit's cookie validation by enqueueing an
//! approval (consumed by `pam_sentinel.so` via the agent's Unix
//! socket) and connecting to `/run/polkit/agent-helper.socket`.

use crate::approval_queue::ApprovalQueue;
use crate::helper_ui;
use crate::helper1;
use crate::remember::RememberCache;
use anyhow::{Context, Result};
use log::{info, warn};
use sentinel_shared::log_kv::quote as q;
use sentinel_shared::logfmt_session_for_pid;
use sentinel_shared::{Outcome, PolicyDecision, ServiceConfig};
use std::time::Instant;

pub struct AuthInputs<'a> {
    pub action_id: &'a str,
    pub cookie: &'a str,
    pub username: &'a str,
    /// Effective `polkit-1` config loaded by the caller (per-call, so
    /// edits to `/etc/security/sentinel.conf` take effect on the next
    /// auth without restarting the agent).
    pub cfg: &'a ServiceConfig,
    pub process_exe: Option<&'a str>,
    pub process_cmdline: Option<&'a str>,
    pub process_pid: Option<i32>,
    pub process_cwd: Option<&'a str>,
    pub requesting_user: Option<&'a str>,
}

/// The generic polkit action id for "run an arbitrary program as root".
const GENERIC_EXEC_ACTION: &str = "org.freedesktop.policykit.exec";

/// Whether a polkit action may participate in the "remember" window.
///
/// The remember cache keys grants on `(action_id, exe)` only — it does
/// **not** include the command line ([`crate::remember`]). For the
/// generic [`GENERIC_EXEC_ACTION`] (`pkexec` — run *any* command as
/// root) that would let a single ticked grant auto-allow *any* later
/// root command from the same caller within the window. So pkexec is
/// excluded from remember entirely: no checkbox, no recording, no
/// auto-allow. Specific actions (e.g. a package-install action) stay
/// eligible — their action id already bounds what the grant covers.
fn remember_eligible(action_id: &str) -> bool {
    action_id != GENERIC_EXEC_ACTION
}

pub async fn run(
    queue: ApprovalQueue,
    remember: RememberCache,
    inputs: AuthInputs<'_>,
) -> Result<bool> {
    // Static [policy] allow/deny, evaluated before the dialog. Matches
    // on the subject's resolved exe path and/or the polkit action id;
    // `deny` wins over `allow`. An allow short-circuits straight to the
    // helper-1 hand-off (no dialog); a deny rejects without one.
    match inputs
        .cfg
        .policy
        .decide(inputs.process_exe, Some(inputs.action_id))
    {
        PolicyDecision::Deny => {
            let process_name = inputs
                .process_exe
                .and_then(sentinel_shared::process_basename)
                .unwrap_or("unknown");
            let session = inputs
                .process_pid
                .map(logfmt_session_for_pid)
                .unwrap_or_default();
            info!(
                "event=auth.deny source=policy user={} action={} process={}{}",
                q(inputs.username),
                q(inputs.action_id),
                q(process_name),
                session
            );
            if inputs.cfg.notify_on_deny {
                sentinel_shared::desktop_notify(
                    "Privilege request blocked",
                    &format!("Policy denied an elevation request from {process_name}."),
                );
            }
            return Ok(false);
        }
        PolicyDecision::Allow => {
            let process_name = inputs
                .process_exe
                .and_then(sentinel_shared::process_basename)
                .unwrap_or("unknown");
            let session = inputs
                .process_pid
                .map(logfmt_session_for_pid)
                .unwrap_or_default();
            info!(
                "event=auth.allow source=policy user={} action={} process={}{}",
                q(inputs.username),
                q(inputs.action_id),
                q(process_name),
                session
            );
            queue.push(inputs.action_id.to_string()).await;
            let success = helper1::run(helper1::Run {
                username: inputs.username,
                cookie: inputs.cookie,
            })
            .await
            .context("run polkit-agent-helper-1")?;
            if !success {
                warn!(
                    "event=auth.error source=agent.helper1 action={} note=\"helper-1 reported FAILURE — PAM stack rejected policy approval?\"",
                    q(inputs.action_id)
                );
            }
            return Ok(success);
        }
        PolicyDecision::Ask => {}
    }

    // Effective remember window for this request. The generic pkexec
    // action is carved out (see `remember_eligible`) — for it this
    // collapses to 0, which transitively disables the checkbox (passed
    // to the helper below), the auto-allow short-circuit, and the
    // recording on Allow.
    let remember_secs = if remember_eligible(inputs.action_id) {
        inputs.cfg.remember_seconds
    } else {
        0
    };

    // In-memory "remember" cache (the polkit-path complement to the root
    // timestamp store, which the PAM module owns for sudo/su). A fresh
    // grant for this (action, exe) auto-allows without a dialog.
    if remember_secs > 0
        && remember
            .is_fresh(inputs.action_id, inputs.process_exe, remember_secs)
            .await
    {
        let process_name = inputs
            .process_exe
            .and_then(sentinel_shared::process_basename)
            .unwrap_or("unknown");
        let session = inputs
            .process_pid
            .map(logfmt_session_for_pid)
            .unwrap_or_default();
        info!(
            "event=auth.allow source=remember user={} action={} process={}{}",
            q(inputs.username),
            q(inputs.action_id),
            q(process_name),
            session
        );
        queue.push(inputs.action_id.to_string()).await;
        let success = helper1::run(helper1::Run {
            username: inputs.username,
            cookie: inputs.cookie,
        })
        .await
        .context("run polkit-agent-helper-1")?;
        return Ok(success);
    }

    let req = helper_ui::Request::for_action(helper_ui::ForAction {
        action_id: inputs.action_id,
        cfg: inputs.cfg,
        remember_secs,
        username: inputs.username,
        process_exe: inputs.process_exe,
        process_cmdline: inputs.process_cmdline,
        process_pid: inputs.process_pid,
        process_cwd: inputs.process_cwd,
        requesting_user: inputs.requesting_user,
    });
    let dialog_started = Instant::now();
    let verdict = helper_ui::run(req).await.context("run sentinel-helper")?;
    let outcome = verdict.outcome;
    let latency_ms = dialog_started.elapsed().as_millis();

    let process_name = inputs
        .process_exe
        .and_then(sentinel_shared::process_basename)
        .unwrap_or("unknown");
    // Session enrichment via the polkit subject's process env. The
    // subject is the user's actual process (the GUI app or shell
    // requesting the privileged action), which is what we want.
    let session = inputs
        .process_pid
        .map(logfmt_session_for_pid)
        .unwrap_or_default();

    match outcome {
        Outcome::Deny => {
            info!(
                "event=auth.deny source=agent user={} action={} process={} latency_ms={}{}",
                q(inputs.username),
                q(inputs.action_id),
                q(process_name),
                latency_ms,
                session
            );
            if inputs.cfg.notify_on_deny {
                sentinel_shared::desktop_notify(
                    "Authentication denied",
                    &format!("You denied an elevation request from {process_name}."),
                );
            }
            return Ok(false);
        }
        Outcome::Timeout => {
            info!(
                "event=auth.timeout source=agent user={} action={} process={} latency_ms={}{}",
                q(inputs.username),
                q(inputs.action_id),
                q(process_name),
                latency_ms,
                session
            );
            if inputs.cfg.notify_on_timeout {
                sentinel_shared::desktop_notify(
                    "Authentication timed out",
                    &format!(
                        "An elevation request from {process_name} was auto-denied (no response)."
                    ),
                );
            }
            return Ok(false);
        }
        Outcome::Allow => {
            info!(
                "event=auth.allow source=agent user={} action={} process={} latency_ms={}{}",
                q(inputs.username),
                q(inputs.action_id),
                q(process_name),
                latency_ms,
                session
            );
        }
    }

    // Record this Allow so repeat (action, exe) requests within the
    // window skip the dialog — but only if the user ticked the
    // "remember" checkbox (verdict.remember), not on every allow.
    if verdict.remember && remember_secs > 0 {
        remember
            .remember(inputs.action_id, inputs.process_exe)
            .await;
    }

    // Pre-approve before handing off to helper-1. helper-1 → PAM →
    // pam_sentinel.so will dequeue this from our socket within a few
    // milliseconds.
    queue.push(inputs.action_id.to_string()).await;

    let success = helper1::run(helper1::Run {
        username: inputs.username,
        cookie: inputs.cookie,
    })
    .await
    .context("run polkit-agent-helper-1")?;

    if !success {
        warn!(
            "event=auth.error source=agent.helper1 action={} note=\"helper-1 reported FAILURE — PAM stack rejected approval?\"",
            q(inputs.action_id)
        );
    }
    Ok(success)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_exec_action_is_not_remember_eligible() {
        // pkexec / "run any command as root" must never be remembered:
        // the (action_id, exe) key is command-blind, so one grant would
        // blanket arbitrary later root commands from the same caller.
        assert!(!remember_eligible(GENERIC_EXEC_ACTION));
        assert!(!remember_eligible("org.freedesktop.policykit.exec"));
    }

    #[test]
    fn specific_actions_are_remember_eligible() {
        // Specific actions are bounded by their own id, so remembering
        // them is appropriately scoped.
        assert!(remember_eligible(
            "org.freedesktop.packagekit.package-install"
        ));
        assert!(remember_eligible("org.freedesktop.systemd1.manage-units"));
    }
}
