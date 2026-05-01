//! `org.freedesktop.PolicyKit1.AuthenticationAgent` server side.

use crate::identity::{self, Identity};
use crate::session::{self, AuthInputs};
use log::{error, info, warn};
use sentinel_token::Issuer;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use zbus::fdo;

pub struct Agent {
    issuer: Arc<Issuer>,
    own_uid: u32,
    sessions: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl Agent {
    pub fn new(issuer: Arc<Issuer>, own_uid: u32) -> Self {
        Self {
            issuer,
            own_uid,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    /// Polkit calls this when an action requires user authentication.
    /// We must not return until the auth attempt is fully resolved.
    async fn begin_authentication(
        &self,
        action_id: String,
        message: String,
        _icon_name: String,
        _details: HashMap<String, String>,
        cookie: String,
        identities: Vec<Identity>,
    ) -> fdo::Result<()> {
        info!("BeginAuthentication action={action_id} cookie={}", cookie_prefix(&cookie));

        let Some(uid) = identity::pick(&identities, self.own_uid) else {
            warn!("no usable unix-user identity in BeginAuthentication");
            return Err(fdo::Error::Failed(
                "no acceptable identities".to_string(),
            ));
        };
        let username = match nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid)) {
            Ok(Some(u)) => u.name,
            _ => {
                error!("uid {uid} has no passwd entry");
                return Err(fdo::Error::Failed(format!("uid {uid} unknown")));
            }
        };

        let issuer = self.issuer.clone();
        let cookie_for_task = cookie.clone();
        let action_for_task = action_id.clone();
        let message_for_task = message.clone();

        // Run the auth flow in a task so CancelAuthentication can abort it.
        let handle = tokio::spawn(async move {
            let _ = session::run(
                issuer,
                AuthInputs {
                    action_id: &action_for_task,
                    message: &message_for_task,
                    cookie: &cookie_for_task,
                    username: &username,
                },
            )
            .await;
        });

        // Register the task so CancelAuthentication can find it.
        {
            let mut sessions = self.sessions.lock().await;
            sessions.insert(cookie.clone(), handle);
        }

        // Wait for the task to finish (or be cancelled). Either way, we
        // remove ourselves from the map and return Ok — polkit's cookie
        // validation outcome was reported by helper-1's privileged D-Bus
        // channel, not by our return value.
        let join_result = {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(&cookie)
        };
        if let Some(handle) = join_result {
            let _ = handle.await;
        }

        info!("BeginAuthentication complete cookie={}", cookie_prefix(&cookie));
        Ok(())
    }

    /// Polkit calls this if auth is no longer needed (another agent
    /// approved, the calling app gave up, etc).
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
