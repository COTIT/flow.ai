//! SQL for warehouse_locations. Every statement takes `workspace_id` + `warehouse_id` and
//! skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Live location.
pub struct LocationRow {
    pub id: Uuid,
    pub location: String,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New row; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    location: &str,
    meta: Option<&Value>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO warehouse_locations (workspace_id, warehouse_id, location, meta)
         VALUES ($1, $2, $3, $4) RETURNING id",
        workspace_id,
        warehouse_id,
        location,
        meta,
    )
    .fetch_one(db)
    .await
}

/// Live location of this warehouse. None = unknown, deleted or elsewhere.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: Uuid,
) -> Result<Option<LocationRow>, sqlx::Error> {
    sqlx::query_as!(
        LocationRow,
        "SELECT id, location, meta, created_at, updated_at FROM warehouse_locations
         WHERE id = $1 AND workspace_id = $2 AND warehouse_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        warehouse_id,
    )
    .fetch_optional(db)
    .await
}

/// One page, oldest first, plus the total matching count (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LocationRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, location, meta, created_at, updated_at, count(*) OVER () AS "total!"
           FROM warehouse_locations
           WHERE workspace_id = $1 AND warehouse_id = $2 AND deleted_at IS NULL
             AND ($3::text IS NULL OR location ILIKE $3)
           ORDER BY id
           LIMIT $4 OFFSET $5"#,
        workspace_id,
        warehouse_id,
        pattern,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| LocationRow {
            id: r.id,
            location: r.location,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Matching count, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    pattern: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM warehouse_locations
           WHERE workspace_id = $1 AND warehouse_id = $2 AND deleted_at IS NULL
             AND ($3::text IS NULL OR location ILIKE $3)"#,
        workspace_id,
        warehouse_id,
        pattern,
    )
    .fetch_one(db)
    .await
}

/// `location` always set (Laravel update rules require it); `meta` patches.
/// False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: Uuid,
    location: &str,
    meta: Option<Option<&Value>>,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_locations
         SET location = $4, meta = CASE WHEN $5 THEN $6 ELSE meta END, updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND warehouse_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        warehouse_id,
        location,
        meta.is_some(),
        meta.flatten(),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete the live rows among `ids` of this warehouse; returns how many.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_locations SET deleted_at = now(), updated_at = now()
         WHERE id = ANY($3) AND workspace_id = $1 AND warehouse_id = $2 AND deleted_at IS NULL",
        workspace_id,
        warehouse_id,
        ids,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected())
}
