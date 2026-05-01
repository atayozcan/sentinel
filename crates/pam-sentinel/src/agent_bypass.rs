//! Bypass-token check: when invoked under `polkit-agent-helper-1` from the
//! Sentinel polkit agent, the env carries an HMAC token proving the user
//! already authenticated via the agent's UI. We verify it and return
//! `PAM_SUCCESS` directly, breaking the recursion that would otherwise
//! spawn another dialog.
//!
//! Fail-open semantics: any verification failure (missing secret,
//! malformed token, mismatched HMAC) returns `None` so the caller falls
//! through to the normal dialog flow. We never `PAM_AUTH_ERR` here — a
//! tampered or stale env var should never block a user from authenticating
//! through the dialog.

use crate::caller_uid;
use crate::libc_getppid;
use pam::constants::PamResultCode;

pub fn check_agent_bypass() -> Option<PamResultCode> {
    let cookie = std::env::var("SENTINEL_AGENT_COOKIE").ok()?;
    let action = std::env::var("SENTINEL_AGENT_ACTION_ID").ok()?;
    let token = std::env::var("SENTINEL_AGENT_AUTH").ok()?;

    let uid = caller_uid(unsafe { libc_getppid() });

    let issuer = match sentinel_token::Issuer::load_for_uid(uid) {
        Ok(i) => i,
        Err(e) => {
            log::debug!(
                "agent secret not loadable for uid {uid} ({e}); falling through"
            );
            return None;
        }
    };

    if issuer.verify(&cookie, &action, &token) {
        log::info!(
            "agent bypass for cookie={} action={action}",
            cookie_prefix(&cookie)
        );
        Some(PamResultCode::PAM_SUCCESS)
    } else {
        log::warn!(
            "agent bypass token mismatch for cookie={} (uid {uid}); falling through to dialog",
            cookie_prefix(&cookie)
        );
        None
    }
}

fn cookie_prefix(cookie: &str) -> &str {
    let n = 8.min(cookie.len());
    &cookie[..n]
}
