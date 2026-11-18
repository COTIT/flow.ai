//! Enqueue the execute job for a new execution. C4b fills the WF-01 arm (contracts §4.6, §6).

use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::AppError;

/// Called by decide in the approve transaction, right after the execution row is inserted.
pub async fn enqueue_execution(
    _tx: &mut PgConnection,
    workflow_key: &str,
    _execution_id: Uuid,
) -> Result<(), AppError> {
    match workflow_key {
        // shortcut: no job yet; C4b replaces this arm with the `wf01_execute` enqueue.
        "wf01_purchase" => Ok(()),
        other => Err(AppError::Internal(format!("unknown workflow_key {other}"))),
    }
}
