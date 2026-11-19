//! SQL for `integration_mappings`. Every query filters by `workspace_id`.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// One mapping row.
pub struct MappingRow {
    pub id: Uuid,
    pub pair_id: Uuid,
    pub entity: String,
    pub source_id: String,
    pub target_id: String,
    pub source_label: Option<String>,
    pub target_label: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// One page of a pair's mappings (optionally one entity), plus the total.
pub async fn list(
    db: impl PgExecutor<'_> + Copy,
    ws: Uuid,
    pair: Uuid,
    entity: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MappingRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        MappingRow,
        "SELECT id, pair_id, entity, source_id, target_id, source_label, target_label, updated_at
         FROM integration_mappings
         WHERE workspace_id = $1 AND pair_id = $2 AND ($3::text IS NULL OR entity = $3)
         ORDER BY entity, source_id LIMIT $4 OFFSET $5",
        ws,
        pair,
        entity,
        limit,
        offset
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM integration_mappings
           WHERE workspace_id = $1 AND pair_id = $2 AND ($3::text IS NULL OR entity = $3)"#,
        ws,
        pair,
        entity
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Insert or replace by `(pair, entity, source_id)`; returns the row id.
#[allow(clippy::too_many_arguments)]
pub async fn upsert(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    pair: Uuid,
    entity: &str,
    source_id: &str,
    target_id: &str,
    source_label: Option<&str>,
    target_label: Option<&str>,
    user: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO integration_mappings
           (workspace_id, pair_id, entity, source_id, target_id, source_label, target_label, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (pair_id, entity, source_id) DO UPDATE
         SET target_id = EXCLUDED.target_id, source_label = EXCLUDED.source_label,
             target_label = EXCLUDED.target_label, updated_by = $8, updated_at = now()
         RETURNING id",
        ws,
        pair,
        entity,
        source_id,
        target_id,
        source_label,
        target_label,
        user
    )
    .fetch_one(db)
    .await
}

/// Delete one; true when it existed.
pub async fn delete(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    pair: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM integration_mappings WHERE workspace_id = $1 AND pair_id = $2 AND id = $3",
        ws,
        pair,
        id
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// `(source_id, target_id)` for the given source ids.
pub async fn resolve(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    pair: Uuid,
    entity: &str,
    source_ids: &[String],
) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT source_id, target_id FROM integration_mappings
         WHERE workspace_id = $1 AND pair_id = $2 AND entity = $3 AND source_id = ANY($4)",
        ws,
        pair,
        entity,
        source_ids
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.source_id, r.target_id))
        .collect())
}

/// True when `user` is an active member of the workspace.
pub async fn active_member(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    user: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM workspace_members
                          WHERE workspace_id = $1 AND user_id = $2 AND active) AS "ok!""#,
        ws,
        user
    )
    .fetch_one(db)
    .await
}

/// True when the pair is the workspace's.
pub async fn pair_exists(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    pair: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM integration_pairs WHERE workspace_id = $1 AND id = $2) AS "ok!""#,
        ws,
        pair
    )
    .fetch_one(db)
    .await
}
