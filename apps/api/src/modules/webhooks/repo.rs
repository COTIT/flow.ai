//! webhook_endpoints SQL. Every query filters `workspace_id`.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// An endpoint, without its sealed secret.
pub struct EndpointRow {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub description: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Fields of a new endpoint; `id` is chosen first so the secret can be sealed to it.
pub struct NewEndpoint<'a> {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub url: &'a str,
    pub events: &'a [String],
    pub description: Option<&'a str>,
    pub secret_sealed: &'a [u8],
    pub secret_key_id: &'a str,
    pub created_by: Uuid,
}

/// Changes to an endpoint; None keeps the field.
pub struct EndpointPatch {
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    pub description: Option<Option<String>>,
    pub active: Option<bool>,
}

pub async fn insert(
    db: impl PgExecutor<'_>,
    e: &NewEndpoint<'_>,
) -> Result<EndpointRow, sqlx::Error> {
    sqlx::query_as!(
        EndpointRow,
        "INSERT INTO webhook_endpoints
             (id, workspace_id, url, events, description, secret_sealed, secret_key_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, url, events, description, active, created_at, updated_at",
        e.id,
        e.workspace_id,
        e.url,
        e.events,
        e.description,
        e.secret_sealed,
        e.secret_key_id,
        e.created_by,
    )
    .fetch_one(db)
    .await
}

pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<EndpointRow>, sqlx::Error> {
    sqlx::query_as!(
        EndpointRow,
        "SELECT id, url, events, description, active, created_at, updated_at
         FROM webhook_endpoints WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// One page, newest first, and the workspace's endpoint count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EndpointRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        EndpointRow,
        "SELECT id, url, events, description, active, created_at, updated_at
         FROM webhook_endpoints WHERE workspace_id = $1
         ORDER BY created_at DESC, id DESC
         LIMIT $2 OFFSET $3",
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM webhook_endpoints WHERE workspace_id = $1"#,
        workspace_id
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Apply `patch`; None when no such endpoint in this workspace.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    patch: &EndpointPatch,
) -> Result<Option<EndpointRow>, sqlx::Error> {
    sqlx::query_as!(
        EndpointRow,
        "UPDATE webhook_endpoints SET
             url = coalesce($3, url),
             events = coalesce($4, events),
             description = CASE WHEN $5 THEN $6 ELSE description END,
             active = coalesce($7, active),
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2
         RETURNING id, url, events, description, active, created_at, updated_at",
        workspace_id,
        id,
        patch.url,
        patch.events.as_deref(),
        patch.description.is_some(),
        patch.description.clone().flatten(),
        patch.active,
    )
    .fetch_optional(db)
    .await
}

/// Delete (cascades to its deliveries). False: no such endpoint in this workspace.
pub async fn delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "DELETE FROM webhook_endpoints WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Rows one prune statement deletes.
const PRUNE_BATCH: i64 = 1000;

/// Delete delivered or failed deliveries untouched for 30 days; how many.
pub async fn prune_deliveries(db: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let n = sqlx::query!(
            "DELETE FROM webhook_deliveries WHERE id = ANY(ARRAY(
                 SELECT id FROM webhook_deliveries
                 WHERE status <> 'pending' AND updated_at < now() - interval '30 days'
                 LIMIT $1))",
            PRUNE_BATCH,
        )
        .execute(db)
        .await?
        .rows_affected();
        total += n;
        if n < PRUNE_BATCH.unsigned_abs() {
            return Ok(total);
        }
    }
}

/// Delete events dispatched over 30 days ago that no delivery points at any more; how many.
pub async fn prune_events(db: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let n = sqlx::query!(
            "DELETE FROM webhook_events WHERE id = ANY(ARRAY(
                 SELECT e.id FROM webhook_events e
                 WHERE e.dispatched_at < now() - interval '30 days'
                   AND NOT EXISTS (SELECT 1 FROM webhook_deliveries d WHERE d.event_id = e.id)
                 LIMIT $1))",
            PRUNE_BATCH,
        )
        .execute(db)
        .await?
        .rows_affected();
        total += n;
        if n < PRUNE_BATCH.unsigned_abs() {
            return Ok(total);
        }
    }
}
