//! Build the `(sa{sv})` "subject" passed to
//! `Authority.RegisterAuthenticationAgent`. polkit insists that the
//! session passed in matches the session it sees our process running in,
//! so we ask logind for the session that owns our PID rather than reading
//! `XDG_SESSION_ID` (which isn't reliably set under `systemctl --user`).

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use zbus::Connection;
use zvariant::OwnedValue;

#[derive(Debug, serde::Serialize, zvariant::Type)]
pub struct Subject {
    pub kind: String,
    pub details: HashMap<String, OwnedValue>,
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LoginManager {
    fn get_session_by_pid(&self, pid: u32) -> zbus::Result<zvariant::OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1"
)]
trait LoginSession {
    #[zbus(property, name = "Id")]
    fn id(&self) -> zbus::Result<String>;
}

/// Resolve the session id by asking logind for our own PID's session.
/// CLI override wins if set.
async fn resolve_session_id(
    conn: &Connection,
    override_value: Option<&str>,
) -> Result<String> {
    if let Some(s) = override_value {
        return Ok(s.to_string());
    }
    let manager = LoginManagerProxy::new(conn)
        .await
        .context("LoginManager proxy")?;
    let pid = std::process::id();
    let path = manager
        .get_session_by_pid(pid)
        .await
        .with_context(|| format!("logind GetSessionByPID({pid})"))?;
    let session = LoginSessionProxy::builder(conn)
        .path(path.into_inner())?
        .build()
        .await?;
    let id = session.id().await.context("read Session.Id")?;
    if id.is_empty() {
        bail!("logind returned empty session id for pid {pid}");
    }
    Ok(id)
}

pub async fn current(
    conn: &Connection,
    session_id_override: Option<&str>,
) -> Result<Subject> {
    let session_id = resolve_session_id(conn, session_id_override)
        .await
        .context("resolve session id")?;
    let mut details = HashMap::new();
    details.insert(
        "session-id".to_string(),
        OwnedValue::try_from(zvariant::Value::from(session_id.clone()))
            .context("wrap session-id")?,
    );
    Ok(Subject {
        kind: "unix-session".to_string(),
        details,
    })
}
