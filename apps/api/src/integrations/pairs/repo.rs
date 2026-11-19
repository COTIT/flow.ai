//! SQL for `integration_pairs`. Every workspace query filters by `workspace_id`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// One pair row.
#[derive(Debug, Clone)]
pub struct PairRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub workflow_key: String,
    pub source_connection_id: Uuid,
    pub target_connection_id: Uuid,
    pub status: String,
    pub paused_reason: Option<String>,
    pub config: Value,
    pub activated_at: Option<DateTime<Utc>>,
    pub last_polled_at: Option<DateTime<Utc>>,
    pub last_swept_at: Option<DateTime<Utc>>,
    pub last_full_scan_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One page, newest first, plus the total.
pub async fn list(
    db: impl PgExecutor<'_> + Copy,
    ws: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<PairRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        PairRow,
        "SELECT id, workspace_id, workflow_key, source_connection_id, target_connection_id,
                status, paused_reason, config, activated_at, last_polled_at, last_swept_at,
                last_full_scan_at, created_at, updated_at
         FROM integration_pairs WHERE workspace_id = $1 ORDER BY id DESC LIMIT $2 OFFSET $3",
        ws,
        limit,
        offset
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM integration_pairs WHERE workspace_id = $1"#,
        ws
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Every active pair, all workspaces (scheduler fan-out).
pub async fn active(db: impl PgExecutor<'_>) -> Result<Vec<PairRow>, sqlx::Error> {
    sqlx::query_as!(
        PairRow,
        "SELECT id, workspace_id, workflow_key, source_connection_id, target_connection_id,
                status, paused_reason, config, activated_at, last_polled_at, last_swept_at,
                last_full_scan_at, created_at, updated_at
         FROM integration_pairs WHERE status = 'active' ORDER BY id"
    )
    .fetch_all(db)
    .await
}

/// The pair, locked FOR UPDATE, or None.
pub async fn find(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<PairRow>, sqlx::Error> {
    sqlx::query_as!(
        PairRow,
        "SELECT id, workspace_id, workflow_key, source_connection_id, target_connection_id,
                status, paused_reason, config, activated_at, last_polled_at, last_swept_at,
                last_full_scan_at, created_at, updated_at
         FROM integration_pairs WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        ws,
        id
    )
    .fetch_optional(db)
    .await
}

/// New draft pair.
pub async fn insert(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    source: Uuid,
    target: Uuid,
    config: &Value,
    user: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO integration_pairs
           (workspace_id, workflow_key, source_connection_id, target_connection_id, config, created_by)
         VALUES ($1, 'wf01_purchase', $2, $3, $4, $5) RETURNING id",
        ws,
        source,
        target,
        config,
        user
    )
    .fetch_one(db)
    .await
}

/// Replace the config.
pub async fn set_config(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    config: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE integration_pairs SET config = $3, updated_at = now() WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        config
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Status + paused reason; `active` stamps `activated_at` once. True when the row exists.
pub async fn set_status(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    status: &str,
    reason: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE integration_pairs
         SET status = $3, paused_reason = $4, updated_at = now(),
             activated_at = CASE WHEN $3 = 'active' THEN coalesce(activated_at, now()) ELSE activated_at END
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        status,
        reason
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}
