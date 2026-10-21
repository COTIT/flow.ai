//! Cap on requests in flight per workspace (`/workspace/{id}/…`): one tenant can't hold the
//! whole DB pool. A request over the cap waits (FIFO) for a slot, then answers 503.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    extract::Request,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{AppState, error::AppError};

/// How often idle workspace entries are dropped, so the map can't grow without bound.
const CLEANUP_EVERY: Duration = Duration::from_secs(60);

/// Requests one workspace may have in flight, and how long one more waits for a slot.
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceSlots {
    pub per_workspace: u32,
    pub wait: Duration,
}

struct Slots {
    limit: WorkspaceSlots,
    open: Mutex<HashMap<Uuid, Arc<Semaphore>>>,
}

impl Slots {
    fn semaphore(&self, workspace: Uuid) -> Arc<Semaphore> {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let permits = usize::try_from(self.limit.per_workspace.max(1)).unwrap_or(1);
        open.entry(workspace)
            .or_insert_with(|| Arc::new(Semaphore::new(permits)))
            .clone()
    }

    /// Drop entries nobody holds or waits on.
    fn retain_busy(&self) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        open.retain(|_, s| Arc::strong_count(s) > 1);
    }
}

/// Wrap `router` in the cap. Spawns a cleanup task, so call inside a tokio runtime.
pub fn apply(router: Router<AppState>, limit: WorkspaceSlots) -> Router<AppState> {
    let slots = Arc::new(Slots {
        limit,
        open: Mutex::new(HashMap::new()),
    });
    let cleanup = Arc::clone(&slots);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(CLEANUP_EVERY);
        loop {
            tick.tick().await;
            cleanup.retain_busy();
        }
    });
    router.layer(middleware::from_fn(move |req: Request, next: Next| {
        let slots = Arc::clone(&slots);
        async move { run(&slots, req, next).await }
    }))
}

async fn run(slots: &Slots, req: Request, next: Next) -> Response {
    let Some(workspace) = workspace_of(req.uri().path()) else {
        return next.run(req).await;
    };
    let semaphore = slots.semaphore(workspace);
    match tokio::time::timeout(slots.limit.wait, semaphore.acquire_owned()).await {
        Ok(Ok(_permit)) => next.run(req).await,
        _ => {
            tracing::warn!(%workspace, "workspace request slots full");
            AppError::Overloaded.into_response()
        }
    }
}

/// The workspace id of a `/workspace/{id}/…` path (prefix-stripped inside `/v1`).
fn workspace_of(path: &str) -> Option<Uuid> {
    let mut segments = path.split('/');
    segments.find(|s| *s == "workspace")?;
    segments.next().and_then(|id| Uuid::parse_str(id).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_workspace_segment() {
        let id = "01900000-0000-7000-8000-000000000001";
        let want = Uuid::parse_str(id).ok();
        assert_eq!(workspace_of(&format!("/workspace/{id}/relations")), want);
        assert_eq!(workspace_of(&format!("/v1/workspace/{id}")), want);
        assert_eq!(workspace_of("/me"), None);
        assert_eq!(workspace_of("/workspace/not-a-uuid/relations"), None);
        assert_eq!(workspace_of("/workspace"), None);
    }

    #[test]
    fn idle_entries_are_dropped() {
        let slots = Slots {
            limit: WorkspaceSlots {
                per_workspace: 2,
                wait: Duration::from_millis(1),
            },
            open: Mutex::new(HashMap::new()),
        };
        let held = slots.semaphore(Uuid::nil());
        slots.semaphore(Uuid::max());
        slots.retain_busy();
        let open = slots.open.lock().unwrap();
        assert_eq!(open.len(), 1, "only the held entry stays");
        assert!(open.contains_key(&Uuid::nil()));
        drop(held);
    }
}
