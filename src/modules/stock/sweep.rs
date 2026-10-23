//! Warehouse stock sweep (job `stock_seed`): when a warehouse becomes the main one or a store,
//! every stock-tracked product gets its zero rows there, chunk by chunk on the job rail.
//! Replaces Laravel's nightly `products:ensure-warehouse-stock-entries`.

use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    error::AppError,
    jobs::{
        kinds::STOCK_SEED,
        repo::{self as jobs, Claimed, NewJob},
        runner::Step,
    },
    modules::stock::repo,
};

/// Products per chunk: one short transaction each.
const CHUNK: i64 = 5_000;

/// Queue the sweep for a workspace inside the caller's transaction. A queued one already
/// covers this change; a running one may have passed it, so a fresh one waits behind it.
pub async fn enqueue(conn: &mut PgConnection, workspace_id: Uuid) -> Result<(), sqlx::Error> {
    let job = NewJob {
        workspace_id: Some(workspace_id),
        concurrency_key: Some(format!("stock_seed:{workspace_id}")),
        ..NewJob::new(STOCK_SEED)
    };
    jobs::enqueue_coalesced(conn, &job).await.map(|_| ())
}

/// Run one chunk: cursor `after` and count `done` live in the payload.
pub async fn run(db: &PgPool, job: &Claimed) -> Result<Step, AppError> {
    let Some(workspace_id) = job.workspace_id else {
        return Err(AppError::BadRequest("stock_seed needs a workspace".into()));
    };
    let after = job
        .payload
        .get("after")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok());
    let done = job.payload.get("done").and_then(Value::as_i64).unwrap_or(0);
    let mut tx = db.begin().await?;
    sqlx::query!("SET LOCAL statement_timeout = '30s'")
        .execute(&mut *tx)
        .await?;
    let Some((last, n)) = repo::seed_chunk(&mut tx, workspace_id, after, CHUNK).await? else {
        return Ok(Step::Done);
    };
    let done = done + n as i64;
    if !job
        .checkpoint(
            &mut *tx,
            &json!({ "after": last, "done": done }),
            done,
            None,
        )
        .await?
    {
        // Lease lost: dropping `tx` rolls the chunk back; the new owner redoes it.
        return Err(AppError::Internal("stock_seed lease lost".into()));
    }
    tx.commit().await?;
    Ok(Step::More)
}
