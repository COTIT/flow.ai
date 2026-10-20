//! SQL for currencies. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::currencies::dto::{CurrencyPatch, NewCurrency};

/// Live currency.
pub struct CurrencyRow {
    pub id: Uuid,
    pub name: String,
    pub asset_type: String,
    pub symbol_prefix: Option<String>,
    pub symbol_suffix: Option<String>,
    pub decimals: i16,
    pub default_currency: bool,
    pub active: bool,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New row; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    c: &NewCurrency,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO currencies (workspace_id, name, asset_type, symbol_prefix, symbol_suffix,
                                 decimals, default_currency, active, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
        workspace_id,
        c.name,
        c.asset_type,
        c.symbol_prefix,
        c.symbol_suffix,
        c.decimals,
        c.default_currency,
        c.active,
        c.meta,
    )
    .fetch_one(db)
    .await
}

/// Live currency of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<CurrencyRow>, sqlx::Error> {
    sqlx::query_as!(
        CurrencyRow,
        "SELECT id, name, asset_type, symbol_prefix, symbol_suffix, decimals, default_currency,
                active, meta, created_at, updated_at
         FROM currencies
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Live currencies of `workspace_id` among `ids`.
pub async fn find_many(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<CurrencyRow>, sqlx::Error> {
    sqlx::query_as!(
        CurrencyRow,
        "SELECT id, name, asset_type, symbol_prefix, symbol_suffix, decimals, default_currency,
                active, meta, created_at, updated_at
         FROM currencies
         WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL",
        ids,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// One page, oldest first, plus the total matching count (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CurrencyRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, asset_type, symbol_prefix, symbol_suffix, decimals, default_currency,
                  active, meta, created_at, updated_at, count(*) OVER () AS "total!"
           FROM currencies
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)
           ORDER BY id
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        pattern,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| CurrencyRow {
            id: r.id,
            name: r.name,
            asset_type: r.asset_type,
            symbol_prefix: r.symbol_prefix,
            symbol_suffix: r.symbol_suffix,
            decimals: r.decimals,
            default_currency: r.default_currency,
            active: r.active,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Matching count, for a page past the end (which has no row to carry it).
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM currencies
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)"#,
        workspace_id,
        pattern,
    )
    .fetch_one(db)
    .await
}

/// One-statement patch, so concurrent edits can't drop each other's fields. False = no such row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &CurrencyPatch,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE currencies
         SET name = COALESCE($3, name),
             asset_type = COALESCE($4, asset_type),
             symbol_prefix = CASE WHEN $5 THEN $6 ELSE symbol_prefix END,
             symbol_suffix = CASE WHEN $7 THEN $8 ELSE symbol_suffix END,
             decimals = COALESCE($9, decimals),
             default_currency = COALESCE($10, default_currency),
             active = COALESCE($11, active),
             meta = CASE WHEN $12 THEN $13 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        p.name,
        p.asset_type,
        p.symbol_prefix.is_some(),
        p.symbol_prefix.as_ref().and_then(Option::as_deref),
        p.symbol_suffix.is_some(),
        p.symbol_suffix.as_ref().and_then(Option::as_deref),
        p.decimals,
        p.default_currency,
        p.active,
        p.meta.is_some(),
        p.meta.as_ref().and_then(Option::as_ref),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Unset the live default of `workspace_id`, except row `keep` (the one becoming default).
pub async fn clear_default(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    keep: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE currencies SET default_currency = false, updated_at = now()
         WHERE workspace_id = $1 AND default_currency AND deleted_at IS NULL
           AND id IS DISTINCT FROM $2",
        workspace_id,
        keep,
    )
    .execute(db)
    .await
    .map(drop)
}

/// Soft delete; documents keep pointing at the row. False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE currencies SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Laravel WorkspaceSetup currencies for a new workspace: Euro (default), UK Sterling, Dollar.
pub async fn seed_defaults(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO currencies (workspace_id, name, asset_type, symbol_prefix, symbol_suffix,
                                 decimals, default_currency, active)
         VALUES ($1, 'Euro', 'eur', '€', 'EUR', 2, true, true),
                ($1, 'UK Sterling', 'gbp', '£', 'GBP', 2, false, true),
                ($1, 'Dollar', 'usd', '$', 'USD', 2, false, true)",
        workspace_id,
    )
    .execute(db)
    .await
    .map(drop)
}
