//! One in-flight authentication: drive `sentinel-helper` for the user
//! decision, then `polkit-agent-helper-1` to satisfy polkit if Allow.

use crate::helper1;
use crate::helper_ui::{self, Outcome};
use anyhow::{Context, Result};
use log::{info, warn};
use sentinel_token::Issuer;
use std::sync::Arc;

pub struct AuthInputs<'a> {
    pub action_id: &'a str,
    pub message: &'a str,
    pub cookie: &'a str,
    pub username: &'a str,
}

pub async fn run(issuer: Arc<Issuer>, inputs: AuthInputs<'_>) -> Result<bool> {
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

    let token = issuer.token(inputs.cookie, inputs.action_id);
    let success = helper1::run(helper1::Run {
        username: inputs.username,
        cookie: inputs.cookie,
        auth_token_b64: &token,
        action_id: inputs.action_id,
    })
    .await
    .context("run polkit-agent-helper-1")?;

    if !success {
        warn!(
            "polkit-agent-helper-1 reported FAILURE for action {} \
             (bypass token mismatch? PAM stack misconfigured?)",
            inputs.action_id
        );
    }
    Ok(success)
}
