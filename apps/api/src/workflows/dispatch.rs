//! Enqueue the execute job for an execution (contracts §4.6, §6); WF-01 arm by C4b.

use sqlx::PgConnection;
use uuid::Uuid;

use crate::{error::AppError, jobs::repo as jobs, workflows::wf01_purchase::execute};

/// Called in the approve / retry / allow_retry transaction. A live job for the execution is
/// reused (dedupe on the execution id), never stacked.
pub async fn enqueue_execution(
    tx: &mut PgConnection,
    workflow_key: &str,
    execution_id: Uuid,
) -> Result<(), AppError> {
    match workflow_key {
        "wf01_purchase" => {
            let ws = sqlx::query_scalar!(
                "SELECT workspace_id FROM workflow_executions WHERE id = $1",
                execution_id
            )
            .fetch_one(&mut *tx)
            .await?;
            jobs::enqueue(tx, &execute::job(ws, execution_id)).await?;
            Ok(())
        }
        other => Err(AppError::Internal(format!("unknown workflow_key {other}"))),
    }
}
