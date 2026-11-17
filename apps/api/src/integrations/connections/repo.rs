//! SQL for `integration_connections`. Every query filters by `workspace_id`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// One connection row (secrets live in `integration_secrets`).
#[derive(Debug, Clone)]
pub struct ConnectionRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub provider: String,
    pub label: String,
    pub config: Value,
    pub status: String,
    pub status_reason: Option<String>,
    pub credential_expires_at: Option<DateTime<Utc>>,
    pub last_probe: Option<Value>,
    pub last_probe_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One page, newest first, plus the total.
pub async fn list(
    db: impl PgExecutor<'_> + Copy,
    ws: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ConnectionRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        ConnectionRow,
        "SELECT id, workspace_id, provider, label, config, status, status_reason,
                credential_expires_at, last_probe, last_probe_at, created_at, updated_at
         FROM integration_connections WHERE workspace_id = $1
         ORDER BY id DESC LIMIT $2 OFFSET $3",
        ws,
        limit,
        offset
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM integration_connections WHERE workspace_id = $1"#,
        ws
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// The row, or None when missing or another workspace's. `lock` takes FOR UPDATE.
pub async fn find(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    lock: bool,
) -> Result<Option<ConnectionRow>, sqlx::Error> {
    // Two statements: FOR UPDATE can't be a bound parameter.
    if lock {
        sqlx::query_as!(
            ConnectionRow,
            "SELECT id, workspace_id, provider, label, config, status, status_reason,
                    credential_expires_at, last_probe, last_probe_at, created_at, updated_at
             FROM integration_connections WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
            ws,
            id
        )
        .fetch_optional(db)
        .await
    } else {
        sqlx::query_as!(
            ConnectionRow,
            "SELECT id, workspace_id, provider, label, config, status, status_reason,
                    credential_expires_at, last_probe, last_probe_at, created_at, updated_at
             FROM integration_connections WHERE workspace_id = $1 AND id = $2",
            ws,
            id
        )
        .fetch_optional(db)
        .await
    }
}

/// New draft connection.
pub async fn insert(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    provider: &str,
    label: &str,
    config: &Value,
    expires_at: Option<DateTime<Utc>>,
    user: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO integration_connections
           (workspace_id, provider, label, config, credential_expires_at, created_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        ws,
        provider,
        label,
        config,
        expires_at,
        user
    )
    .fetch_one(db)
    .await
}

/// Write the editable fields and status.
#[allow(clippy::too_many_arguments)]
pub async fn update(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    label: &str,
    config: &Value,
    expires_at: Option<DateTime<Utc>>,
    status: &str,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE integration_connections
         SET label = $3, config = $4, credential_expires_at = $5, status = $6,
             status_reason = $7, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        label,
        config,
        expires_at,
        status,
        reason
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Status + reason; true when the row exists.
pub async fn mark(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    status: &str,
    reason: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE integration_connections SET status = $3, status_reason = $4, updated_at = now()
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

/// Store a probe result with the status it led to.
pub async fn set_probe(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    status: &str,
    reason: Option<&str>,
    checks: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE integration_connections
         SET status = $3, status_reason = $4, last_probe = $5, last_probe_at = now(),
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        status,
        reason,
        checks
    )
    .execute(db)
    .await?;
    Ok(())
}
