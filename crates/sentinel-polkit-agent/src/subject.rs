// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Build the `(sa{sv})` "subject" passed to
//! `Authority.RegisterAuthenticationAgent`. We always register as a
//! `unix-session` subject scoped to the current user's session.
//!
//! Resolution order (none of these require D-Bus calls — `GetSessionByPID`
//! on logind requires polkit authorization, which an unprivileged agent
//! doesn't have):
//!
//! 1. `--session-id` CLI override.
//! 2. `XDG_SESSION_ID` env var (set by `pam_systemd` at login, propagated
//!    through compositors that import login env into XDG autostart
//!    children).
//! 3. `/proc/self/sessionid` (kernel audit subsystem; set by
//!    `audit_setloginuid` at login and inherited through forks). For the
//!    agent under XDG autostart, this matches the compositor's sessionid
//!    — exactly what polkit's session-equality check needs.

use anyhow::{Result, bail};
use std::collections::HashMap;
use zvariant::OwnedValue;

#[derive(Debug, serde::Serialize, zvariant::Type)]
pub struct Subject {
    pub kind: String,
    pub details: HashMap<String, OwnedValue>,
}

fn resolve_session_id(override_value: Option<&str>) -> Result<String> {
    if let Some(s) = override_value {
        return Ok(s.to_string());
    }
    if let Ok(s) = std::env::var("XDG_SESSION_ID") {
        if !s.is_empty() {
            return Ok(s);
        }
    }
    if let Ok(s) = std::fs::read_to_string("/proc/self/sessionid") {
        let s = s.trim();
        if !s.is_empty() && s != "4294967295" {
            return Ok(s.to_string());
        }
    }
    bail!(
        "could not resolve session id; set XDG_SESSION_ID or pass --session-id. \
         (Hint: this agent must run inside a graphical session — typically \
         autostarted by your compositor via /etc/xdg/autostart/.)"
    );
}

pub fn current(session_id_override: Option<&str>) -> Result<Subject> {
    let session_id = resolve_session_id(session_id_override)?;
    let mut details = HashMap::new();
    details.insert(
        "session-id".to_string(),
        OwnedValue::try_from(zvariant::Value::from(session_id.clone()))?,
    );
    Ok(Subject {
        kind: "unix-session".to_string(),
        details,
    })
}
