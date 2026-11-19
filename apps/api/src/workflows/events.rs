//! Append-only workflow timeline (`workflow_events`). Inserts are never fenced: a stale worker
//! may still leave evidence (P-2).
// shortcut: C4a/C4b are the callers of this contract API (§4.6); drop the allow when they land.
#![allow(dead_code)]

use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::error::AppError;

/// What an event belongs to. At least one of the two ids is set (DB CHECK).
#[derive(Debug, Clone, Copy, Default)]
pub struct EventRef {
    pub execution_id: Option<Uuid>,
    pub approval_request_id: Option<Uuid>,
    /// Set on `api_result` / `stale_worker_result` rows.
    pub attempt_no: Option<i16>,
}

/// Who caused the event.
#[derive(Debug, Clone, Copy)]
pub enum Actor {
    System,
    User(Uuid),
}

/// Insert one event. `kind` per contracts §3.7; `payload` an object, never secrets.
pub async fn record(
    conn: impl PgExecutor<'_>,
    ws: Uuid,
    at: EventRef,
    kind: &str,
    payload: Value,
    actor: Actor,
) -> Result<(), AppError> {
    let (actor_kind, actor_user_id) = match actor {
        Actor::System => ("system", None),
        Actor::User(u) => ("user", Some(u)),
    };
    sqlx::query!(
        "INSERT INTO workflow_events (workspace_id, execution_id, approval_request_id, attempt_no,
             kind, payload, actor_kind, actor_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        ws,
        at.execution_id,
        at.approval_request_id,
        at.attempt_no,
        kind,
        payload,
        actor_kind,
        actor_user_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}
