//! `org.freedesktop.PolicyKit1.AuthenticationAgent` server side.

use crate::approval_queue::ApprovalQueue;
use crate::identity::{self, Identity};
use crate::session::{self, AuthInputs};
use log::{error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use zbus::fdo;

/// PAM service name we present to the shared config schema. Polkit's
/// helper-1 uses this as its PAM service, and it's what an admin sets
/// `[services.<name>]` overrides under.
const POLKIT_PAM_SERVICE: &str = "polkit-1";

pub struct Agent {
    own_uid: u32,
    queue: ApprovalQueue,
    sessions: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    /// Serializes BeginAuthentication so only one dialog + helper-1
    /// handoff is in flight at any time. This bounds the bypass-queue
    /// race window: the approval pushed by a given session can only be
    /// consumed by that session's helper-1 invocation, because no other
    /// `session::run` can push a competing approval until we drop the
    /// guard. Polkit doesn't pipeline BeginAuthentication in practice,
    /// so this is invisible to the user.
    inflight: Arc<Mutex<()>>,
}

impl Agent {
    pub fn new(own_uid: u32, queue: ApprovalQueue) -> Self {
        Self {
            own_uid,
            queue,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            inflight: Arc::new(Mutex::new(())),
        }
    }
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    /// Polkit calls this when an action requires user authentication.
    /// We must not return until the auth attempt is fully resolved (or
    /// CancelAuthentication aborts it).
    async fn begin_authentication(
        &self,
        action_id: String,
        // Polkit's per-action message string. Intentionally ignored —
        // see `helper_ui::Request::for_action` for why.
        _message: String,
        _icon_name: String,
        details: HashMap<String, String>,
        cookie: String,
        identities: Vec<Identity>,
    ) -> fdo::Result<()> {
        info!(
            "BeginAuthentication action={action_id} cookie={}",
            cookie_prefix(&cookie)
        );

        // Serialize: only one dialog + helper-1 handoff at a time. See
        // the field comment on `Agent::inflight` for why.
        let _serialize_guard = self.inflight.lock().await;

        let Some(uid) = identity::pick(&identities, self.own_uid) else {
            warn!("no usable unix-user identity in BeginAuthentication");
            return Err(fdo::Error::Failed("no acceptable identities".to_string()));
        };
        let username = match nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid)) {
            Ok(Some(u)) => u.name,
            _ => {
                error!("uid {uid} has no passwd entry");
                return Err(fdo::Error::Failed(format!("uid {uid} unknown")));
            }
        };

        let subject_pid = details
            .get("polkit.subject-pid")
            .or_else(|| details.get("polkit.caller-pid"))
            .and_then(|s| s.parse::<i32>().ok());
        let process_exe = subject_pid.and_then(read_proc_exe);
        let process_cmdline = subject_pid.and_then(read_proc_cmdline);
        let process_cwd = subject_pid.and_then(read_proc_cwd);
        let username_for_task = username.clone();

        // Re-read config per call so an admin's edit to
        // /etc/security/sentinel.conf takes effect on the next polkit
        // auth, no agent restart required. Same per-call discipline as
        // pam_sentinel.so. `enabled = false` on `polkit-1` is logged
        // but not honoured here: the agent has already registered with
        // polkitd so we can't disable ourselves mid-session, and a
        // refusal would leave polkit with no agent at all. Rendering
        // the dialog is the safer default.
        let cfg = sentinel_config::load(POLKIT_PAM_SERVICE);
        if !cfg.enabled {
            warn!(
                "[services.{POLKIT_PAM_SERVICE}].enabled = false in config — \
                 agent is registered, ignoring and rendering the dialog anyway"
            );
        }

        let queue = self.queue.clone();
        let cookie_for_task = cookie.clone();
        let action_for_task = action_id.clone();
        let exe_for_task = process_exe.clone();
        let cmdline_for_task = process_cmdline.clone();
        let cwd_for_task = process_cwd.clone();

        // Done channel: the spawned task signals completion. If the
        // handle is aborted (CancelAuthentication), the sender is
        // dropped and `done_rx.await` returns Err — we still proceed
        // to the cleanup step.
        let (done_tx, done_rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            let _ = session::run(
                queue,
                AuthInputs {
                    action_id: &action_for_task,
                    cookie: &cookie_for_task,
                    username: &username,
                    cfg: &cfg,
                    process_exe: exe_for_task.as_deref(),
                    process_cmdline: cmdline_for_task.as_deref(),
                    process_pid: subject_pid,
                    process_cwd: cwd_for_task.as_deref(),
                    requesting_user: Some(&username_for_task),
                },
            )
            .await;
            let _ = done_tx.send(());
        });

        // Insert and KEEP the handle in the map for the duration of
        // the auth — that's what makes CancelAuthentication able to
        // actually abort us.
        {
            let mut sessions = self.sessions.lock().await;
            sessions.insert(cookie.clone(), handle);
        }

        let _ = done_rx.await;

        {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(&cookie);
        }

        info!(
            "BeginAuthentication complete cookie={}",
            cookie_prefix(&cookie)
        );
        Ok(())
    }

    async fn cancel_authentication(&self, cookie: String) -> fdo::Result<()> {
        info!("CancelAuthentication cookie={}", cookie_prefix(&cookie));
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.remove(&cookie) {
            handle.abort();
        }
        Ok(())
    }
}

fn cookie_prefix(cookie: &str) -> &str {
    let n = 8.min(cookie.len());
    &cookie[..n]
}

/// Polkit's `details` dict only carries `polkit.subject-pid` and
/// `polkit.caller-pid`, never the exe path or argv — resolve those from
/// `/proc` so the helper can render the sudo-style process card.
fn read_proc_exe(pid: i32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .and_then(|p| p.into_os_string().into_string().ok())
}

/// Read `/proc/<pid>/cmdline` and convert NUL-separated argv into a
/// shell-printable single line.
fn read_proc_cmdline(pid: i32) -> Option<String> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let parts: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn read_proc_cwd(pid: i32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .and_then(|p| p.into_os_string().into_string().ok())
}
