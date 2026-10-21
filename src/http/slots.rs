//! Requests in flight per workspace and per user, taken by `WorkspaceAccess` after the
//! membership check: one tenant or user can't hold the whole DB pool, and only members can
//! take a workspace's slots. A request over a cap waits (FIFO) for a slot, then answers 503.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::error::AppError;

/// How often idle entries are dropped, so the maps hold only keys in use.
const CLEANUP_EVERY: Duration = Duration::from_secs(60);

/// Caps: requests one workspace and one user may have in flight, and how long one more waits.
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceSlots {
    pub per_workspace: u32,
    pub per_user: u32,
    pub wait: Duration,
}

/// The shared slot registry (cheap to clone).
#[derive(Clone)]
pub struct Slots(Arc<Registry>);

struct Registry {
    limits: WorkspaceSlots,
    workspaces: Mutex<HashMap<Uuid, Arc<Semaphore>>>,
    users: Mutex<HashMap<Uuid, Arc<Semaphore>>>,
}

/// A request's slots; released on drop (when the handler's `WorkspaceAccess` goes).
#[derive(Debug)]
pub struct Slot {
    _user: OwnedSemaphorePermit,
    _workspace: OwnedSemaphorePermit,
}

impl Slots {
    /// Empty registry with these caps.
    pub fn new(limits: WorkspaceSlots) -> Self {
        Self(Arc::new(Registry {
            limits,
            workspaces: Mutex::new(HashMap::new()),
            users: Mutex::new(HashMap::new()),
        }))
    }

    /// One slot of `user` and one of `workspace`, waiting up to the configured time; else 503.
    pub async fn take(&self, user: Uuid, workspace: Uuid) -> Result<Slot, AppError> {
        let r = &self.0;
        let user_sem = semaphore(&r.users, user, r.limits.per_user);
        let workspace_sem = semaphore(&r.workspaces, workspace, r.limits.per_workspace);
        let both = async {
            let user = user_sem.acquire_owned().await.ok()?;
            let workspace = workspace_sem.acquire_owned().await.ok()?;
            Some(Slot {
                _user: user,
                _workspace: workspace,
            })
        };
        match tokio::time::timeout(r.limits.wait, both).await {
            Ok(Some(slot)) => Ok(slot),
            _ => {
                tracing::warn!(%user, %workspace, "request slots full");
                Err(AppError::Overloaded)
            }
        }
    }

    /// Drop idle entries every minute while the registry lives. Needs a tokio runtime.
    pub fn spawn_cleanup(&self) {
        let registry: Weak<Registry> = Arc::downgrade(&self.0);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(CLEANUP_EVERY);
            loop {
                tick.tick().await;
                let Some(r) = registry.upgrade() else { break };
                retain_busy(&r.workspaces);
                retain_busy(&r.users);
            }
        });
    }
}

fn semaphore(map: &Mutex<HashMap<Uuid, Arc<Semaphore>>>, key: Uuid, cap: u32) -> Arc<Semaphore> {
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(key)
        .or_insert_with(|| Arc::new(Semaphore::new(cap.max(1) as usize)))
        .clone()
}

/// Keep entries someone holds or waits on (an `Arc` clone outside the map).
fn retain_busy(map: &Mutex<HashMap<Uuid, Arc<Semaphore>>>) {
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, s| Arc::strong_count(s) > 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(per_workspace: u32, per_user: u32) -> Slots {
        Slots::new(WorkspaceSlots {
            per_workspace,
            per_user,
            wait: Duration::from_millis(20),
        })
    }

    #[tokio::test]
    async fn caps_per_workspace_and_per_user() {
        let s = slots(1, 2);
        let (ada, bob) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let (a, b) = (Uuid::from_u128(10), Uuid::from_u128(20));
        let held = s.take(ada, a).await.unwrap();
        assert!(s.take(bob, a).await.is_err(), "workspace A full");
        let other = s.take(ada, b).await.unwrap();
        assert!(
            s.take(ada, Uuid::from_u128(30)).await.is_err(),
            "Ada's 2 slots used"
        );
        drop(held);
        assert!(s.take(bob, a).await.is_ok(), "freed");
        drop(other);
    }

    #[tokio::test]
    async fn idle_entries_are_dropped() {
        let s = slots(2, 2);
        let held = s.take(Uuid::nil(), Uuid::nil()).await.unwrap();
        drop(s.take(Uuid::max(), Uuid::max()).await.unwrap());
        retain_busy(&s.0.workspaces);
        retain_busy(&s.0.users);
        assert_eq!(s.0.workspaces.lock().unwrap().len(), 1);
        assert_eq!(s.0.users.lock().unwrap().len(), 1);
        drop(held);
    }
}
