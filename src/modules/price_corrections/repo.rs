//! SQL for price corrections, every query scoped to one workspace and live rows, plus the
//! band read the product prices use.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::{price_corrections::dto::CorrectionInput, products::pricing::Band};

/// One live price correction.
pub struct CorrectionRow {
    pub id: Uuid,
    pub value_from: Decimal,
    pub value_to: Decimal,
    pub round_on: Option<Decimal>,
    pub remove_amount: Option<Decimal>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Window count on list reads.
    pub total: Option<i64>,
}

/// New band (create input: both ends set); returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    c: &CorrectionInput,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO price_corrections (workspace_id, value_from, value_to, round_on, remove_amount, meta)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        workspace_id,
        c.value_from,
        c.value_to,
        c.round_on.flatten(),
        c.remove_amount.flatten(),
        c.meta.clone().flatten(),
    )
    .fetch_one(db)
    .await
}

/// Live band of the workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<CorrectionRow>, sqlx::Error> {
    sqlx::query_as!(
        CorrectionRow,
        r#"SELECT id, value_from, value_to, round_on, remove_amount, meta, created_at, updated_at,
                  NULL::bigint AS total
           FROM price_corrections WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL"#,
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// One page (oldest first) with the window count.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<CorrectionRow>, sqlx::Error> {
    sqlx::query_as!(
        CorrectionRow,
        r#"SELECT id, value_from, value_to, round_on, remove_amount, meta, created_at, updated_at,
                  count(*) OVER () AS total
           FROM price_corrections WHERE workspace_id = $1 AND deleted_at IS NULL
           ORDER BY id LIMIT $2 OFFSET $3"#,
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// Live bands of the workspace (the quota and the empty-page total).
pub async fn count(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM price_corrections
           WHERE workspace_id = $1 AND deleted_at IS NULL"#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Patch a live band in one statement. True when a row matched.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    c: &CorrectionInput,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE price_corrections SET
             value_from = COALESCE($3, value_from), value_to = COALESCE($4, value_to),
             round_on = CASE WHEN $5 THEN $6 ELSE round_on END,
             remove_amount = CASE WHEN $7 THEN $8 ELSE remove_amount END,
             meta = CASE WHEN $9 THEN $10 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        c.value_from,
        c.value_to,
        c.round_on.is_some(),
        c.round_on.flatten(),
        c.remove_amount.is_some(),
        c.remove_amount.flatten(),
        c.meta.is_some(),
        c.meta.clone().flatten(),
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// Soft delete a live band. True when it was live.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE price_corrections SET deleted_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// Every live band of the workspace in pick order (lowest start, then oldest). Bounded by the
/// per-workspace quota.
pub async fn bands(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<Vec<Band>, sqlx::Error> {
    sqlx::query_as!(
        Band,
        "SELECT value_from, value_to, round_on, remove_amount FROM price_corrections
         WHERE workspace_id = $1 AND deleted_at IS NULL ORDER BY value_from, id",
        workspace_id,
    )
    .fetch_all(db)
    .await
}
