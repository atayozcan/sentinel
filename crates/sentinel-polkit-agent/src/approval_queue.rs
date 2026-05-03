//! In-memory queue of pre-approved auths.
//!
//! On user-Allow click, the agent calls `push(action_id)`. When
//! `pam_sentinel.so` (running inside `polkit-agent-helper-1`) connects
//! to the agent's Unix socket, the server calls `take_one()` to dequeue
//! a non-expired approval. Each approval is one-shot (consumed on
//! first read) and short-lived.
//!
//! TTL is intentionally tight (1 s). The bypass socket is consumed by
//! `polkit-agent-helper-1` within milliseconds of `agent::session::run`
//! pushing the approval — anything past 1 s means helper-1 isn't going
//! to consume it, and we'd rather the approval expire than be claimed
//! by an unrelated auth that races in. Combined with the per-Agent
//! `inflight` serialization mutex, the practical window for cross-
//! action mis-pairing is bounded by the helper-1 setup time.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const DEFAULT_TTL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct Approval {
    pub action_id: String,
    pub expires_at: Instant,
}

#[derive(Clone, Default)]
pub struct ApprovalQueue {
    inner: Arc<Mutex<VecDeque<Approval>>>,
}

impl ApprovalQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue an approval that lives for `DEFAULT_TTL`.
    pub async fn push(&self, action_id: String) {
        let mut q = self.inner.lock().await;
        q.push_back(Approval {
            action_id,
            expires_at: Instant::now() + DEFAULT_TTL,
        });
    }

    /// Dequeue the next non-expired approval, if any. Side effect:
    /// drops any expired entries it walks past.
    pub async fn take_one(&self) -> Option<Approval> {
        let mut q = self.inner.lock().await;
        let now = Instant::now();
        while let Some(front) = q.front() {
            if front.expires_at > now {
                return q.pop_front();
            }
            q.pop_front();
        }
        None
    }

    /// Drop every queued approval. Called by `Agent::cancel_authentication`
    /// to invalidate any approval the user pushed for the cookie polkit
    /// is now canceling. Without this, a `BeginAuthentication → Allow →
    /// CancelAuthentication → BeginAuthentication → Allow` sequence
    /// could leave the first push live in the queue with up to 1 s TTL,
    /// and the second flow's `polkit-agent-helper-1` would consume it
    /// — auditing the second action under the first action's id.
    ///
    /// Correctness rests on the per-Agent `inflight` mutex: at any
    /// moment ≤1 approval is queued, and that approval belongs to the
    /// session currently being processed. Cancel ⇒ drain ⇒ next push
    /// is fresh.
    pub async fn drain(&self) {
        let mut q = self.inner.lock().await;
        q.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn push_then_take_returns_it() {
        let q = ApprovalQueue::new();
        q.push("org.example.foo".into()).await;
        let a = q.take_one().await.expect("should have one");
        assert_eq!(a.action_id, "org.example.foo");
        assert!(q.take_one().await.is_none());
    }

    #[tokio::test]
    async fn expired_entries_skipped() {
        let q = ApprovalQueue::new();
        // Manually insert an already-expired entry.
        {
            let mut inner = q.inner.lock().await;
            inner.push_back(Approval {
                action_id: "stale".into(),
                expires_at: Instant::now() - Duration::from_secs(1),
            });
        }
        q.push("fresh".into()).await;
        let a = q.take_one().await.expect("fresh expected");
        assert_eq!(a.action_id, "fresh");
    }

    #[tokio::test]
    async fn fifo_order() {
        let q = ApprovalQueue::new();
        q.push("first".into()).await;
        q.push("second".into()).await;
        assert_eq!(q.take_one().await.unwrap().action_id, "first");
        assert_eq!(q.take_one().await.unwrap().action_id, "second");
    }

    #[tokio::test]
    async fn drain_clears_queue() {
        let q = ApprovalQueue::new();
        q.push("stale".into()).await;
        q.drain().await;
        assert!(q.take_one().await.is_none());
    }

    #[tokio::test]
    async fn push_drain_push_only_returns_second() {
        // Models the cross-action race: a leftover approval from a
        // canceled session must not be picked up by the next one.
        let q = ApprovalQueue::new();
        q.push("canceled".into()).await;
        q.drain().await;
        q.push("fresh".into()).await;
        let a = q.take_one().await.expect("fresh expected");
        assert_eq!(a.action_id, "fresh");
        assert!(q.take_one().await.is_none());
    }
}
