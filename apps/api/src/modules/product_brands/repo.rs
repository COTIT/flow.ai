//! SQL for product_brands. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::product_brands::dto::BrandInput;

/// Live brand.
pub struct BrandRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub logo: Option<String>,
    pub website: Option<String>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New row; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    b: &BrandInput,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO product_brands (workspace_id, name, description, logo, website, meta)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        workspace_id,
        b.name.as_deref().unwrap_or_default(),
        b.description.clone().flatten(),
        b.logo.clone().flatten(),
        b.website.clone().flatten(),
        b.meta.clone().flatten(),
    )
    .fetch_one(db)
    .await
}

/// Live brand of `workspace_id`. None = 404.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<BrandRow>, sqlx::Error> {
    sqlx::query_as!(
        BrandRow,
        "SELECT id, name, description, logo, website, meta, created_at, updated_at
         FROM product_brands WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// One page, oldest first, plus the total (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BrandRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, description, logo, website, meta, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM product_brands
           WHERE workspace_id = $1 AND deleted_at IS NULL AND ($2::text IS NULL OR name ILIKE $2)
           ORDER BY id LIMIT $3 OFFSET $4"#,
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
        .map(|r| BrandRow {
            id: r.id,
            name: r.name,
            description: r.description,
            logo: r.logo,
            website: r.website,
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
    pattern: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM product_brands
           WHERE workspace_id = $1 AND deleted_at IS NULL AND ($2::text IS NULL OR name ILIKE $2)"#,
        workspace_id,
        pattern,
    )
    .fetch_one(db)
    .await
}

/// Patch in one statement. False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    b: &BrandInput,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE product_brands SET
             name = COALESCE($3, name),
             description = CASE WHEN $4 THEN $5 ELSE description END,
             logo = CASE WHEN $6 THEN $7 ELSE logo END,
             website = CASE WHEN $8 THEN $9 ELSE website END,
             meta = CASE WHEN $10 THEN $11 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        b.name.as_deref(),
        b.description.is_some(),
        b.description.clone().flatten(),
        b.logo.is_some(),
        b.logo.clone().flatten(),
        b.website.is_some(),
        b.website.clone().flatten(),
        b.meta.is_some(),
        b.meta.clone().flatten(),
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
        "UPDATE product_brands SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
