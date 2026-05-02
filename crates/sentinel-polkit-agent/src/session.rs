//! One in-flight authentication: drive `sentinel-helper` for the user
//! decision, then satisfy polkit's cookie validation by enqueueing an
//! approval (consumed by `pam_sentinel.so` via the agent's Unix
//! socket) and connecting to `/run/polkit/agent-helper.socket`.

use crate::approval_queue::ApprovalQueue;
use crate::helper1;
use crate::helper_ui::{self, Outcome};
use anyhow::{Context, Result};
use log::{info, warn};

pub struct AuthInputs<'a> {
    pub action_id: &'a str,
    pub message: &'a str,
    pub cookie: &'a str,
    pub username: &'a str,
}

pub async fn run(queue: ApprovalQueue, inputs: AuthInputs<'_>) -> Result<bool> {
    let req = helper_ui::Request::for_action(inputs.action_id, inputs.message);
    let outcome = helper_ui::run(req)
        .await
        .context("run sentinel-helper")?;

    match outcome {
        Outcome::Deny | Outcome::Timeout => {
            info!(
                "user denied (outcome={outcome:?}) for action {}",
                inputs.action_id
            );
            return Ok(false);
        }
        Outcome::Allow => {}
    }

    // Pre-approve before handing off to helper-1. helper-1 → PAM →
    // pam_sentinel.so will dequeue this from our socket within a few
    // milliseconds.
    queue.push(inputs.action_id.to_string()).await;
    info!("queued approval for action {}", inputs.action_id);

    let success = helper1::run(helper1::Run {
        username: inputs.username,
        cookie: inputs.cookie,
    })
    .await
    .context("run polkit-agent-helper-1")?;

    if !success {
        warn!(
            "polkit-agent-helper-1 reported FAILURE for action {} \
             (PAM stack didn't accept our approval?)",
            inputs.action_id
        );
    }
    Ok(success)
}
