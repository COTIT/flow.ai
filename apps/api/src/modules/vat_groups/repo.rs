//! SQL for vat_groups. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::vat_groups::dto::{NewVatGroup, VatGroupPatch};

/// Live VAT group.
#[derive(Clone)]
pub struct VatGroupRow {
    pub id: Uuid,
    pub name: String,
    pub vat_amount: Decimal,
    pub default_group: bool,
    pub country: Option<String>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List filter. `country` uppercase ISO-2; `pattern` from `list::search`.
pub struct ListFilter<'a> {
    pub country: Option<&'a str>,
    pub pattern: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

/// New row; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    g: &NewVatGroup,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO vat_groups (workspace_id, name, vat_amount, default_group, country, meta)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
        workspace_id,
        g.name,
        g.vat_amount,
        g.default_group,
        g.country,
        g.meta,
    )
    .fetch_one(db)
    .await
}

/// Live group of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<VatGroupRow>, sqlx::Error> {
    sqlx::query_as!(
        VatGroupRow,
        "SELECT id, name, vat_amount, default_group, country, meta, created_at, updated_at
         FROM vat_groups
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// One page, oldest first, plus the total matching count (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<VatGroupRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, vat_amount, default_group, country, meta, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM vat_groups
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR country = $2)
             AND ($3::text IS NULL OR name ILIKE $3)
           ORDER BY id
           LIMIT $4 OFFSET $5"#,
        workspace_id,
        f.country,
        f.pattern,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| VatGroupRow {
            id: r.id,
            name: r.name,
            vat_amount: r.vat_amount,
            default_group: r.default_group,
            country: r.country,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Every live group of the workspace, oldest first. Reference data: a few dozen rows at most,
/// so relation views filter it in memory instead of one query per address country.
pub async fn all(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<VatGroupRow>, sqlx::Error> {
    sqlx::query_as!(
        VatGroupRow,
        "SELECT id, name, vat_amount, default_group, country, meta, created_at, updated_at
         FROM vat_groups
         WHERE workspace_id = $1 AND deleted_at IS NULL
         ORDER BY id",
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// Matching count, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM vat_groups
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR country = $2)
             AND ($3::text IS NULL OR name ILIKE $3)"#,
        workspace_id,
        f.country,
        f.pattern,
    )
    .fetch_one(db)
    .await
}

/// Name and rate always set (Laravel update rules require them); the rest is a patch.
/// False = no such row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &VatGroupPatch,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE vat_groups
         SET name = $3,
             vat_amount = $4,
             default_group = COALESCE($5, default_group),
             country = CASE WHEN $6 THEN $7 ELSE country END,
             meta = CASE WHEN $8 THEN $9 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        p.name,
        p.vat_amount,
        p.default_group,
        p.country.is_some(),
        p.country.as_ref().and_then(Option::as_deref),
        p.meta.is_some(),
        p.meta.as_ref().and_then(Option::as_ref),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete. False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE vat_groups SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
