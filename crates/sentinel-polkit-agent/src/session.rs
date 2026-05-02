//! One in-flight authentication: drive `sentinel-helper` for the user
//! decision, then satisfy polkit's cookie validation by enqueueing an
//! approval (consumed by `pam_sentinel.so` via the agent's Unix
//! socket) and connecting to `/run/polkit/agent-helper.socket`.

use crate::approval_queue::ApprovalQueue;
use crate::helper_ui;
use crate::helper1;
use anyhow::{Context, Result};
use log::{info, warn};
use sentinel_config::log_kv::quote as q;
use sentinel_config::{Outcome, ServiceConfig};
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

pub async fn run(queue: ApprovalQueue, inputs: AuthInputs<'_>) -> Result<bool> {
    let req = helper_ui::Request::for_action(helper_ui::ForAction {
        action_id: inputs.action_id,
        cfg: inputs.cfg,
        username: inputs.username,
        process_exe: inputs.process_exe,
        process_cmdline: inputs.process_cmdline,
        process_pid: inputs.process_pid,
        process_cwd: inputs.process_cwd,
        requesting_user: inputs.requesting_user,
    });
    let dialog_started = Instant::now();
    let outcome = helper_ui::run(req).await.context("run sentinel-helper")?;
    let latency_ms = dialog_started.elapsed().as_millis();

    let process_name = inputs
        .process_exe
        .and_then(sentinel_config::process_basename)
        .unwrap_or("unknown");

    match outcome {
        Outcome::Deny => {
            info!(
                "event=auth.deny source=agent user={} action={} process={} latency_ms={}",
                q(inputs.username),
                q(inputs.action_id),
                q(process_name),
                latency_ms
            );
            return Ok(false);
        }
        Outcome::Timeout => {
            info!(
                "event=auth.timeout source=agent user={} action={} process={} latency_ms={}",
                q(inputs.username),
                q(inputs.action_id),
                q(process_name),
                latency_ms
            );
            return Ok(false);
        }
        Outcome::Allow => {
            info!(
                "event=auth.allow source=agent user={} action={} process={} latency_ms={}",
                q(inputs.username),
                q(inputs.action_id),
                q(process_name),
                latency_ms
            );
        }
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
