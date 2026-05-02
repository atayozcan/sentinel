//! In-memory queue of pre-approved auths.
//!
//! On user-Allow click, the agent calls `push(action_id)`. When
//! `pam_sentinel.so` (running inside `polkit-agent-helper-1`) connects
//! to the agent's Unix socket, the server calls `take_one()` to dequeue
//! a non-expired approval. Each approval is one-shot (consumed on
//! first read) and short-lived (5 s TTL by default).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const DEFAULT_TTL: Duration = Duration::from_secs(5);

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
}
