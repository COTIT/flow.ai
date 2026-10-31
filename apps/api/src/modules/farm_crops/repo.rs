//! farm_crops SQL. Every statement filters `workspace_id`.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// A stored crop.
pub struct CropRow {
    pub id: Uuid,
    pub code: String,
    pub name_en: String,
    pub name_sw: String,
    pub measured_by: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Changed fields; None = keep.
#[derive(Default)]
pub struct CropPatch {
    pub code: Option<String>,
    pub name_en: Option<String>,
    pub name_sw: Option<String>,
    pub measured_by: Option<String>,
    pub is_active: Option<bool>,
}

/// One page of crops by code, and the count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CropRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        CropRow,
        "SELECT id, code, name_en, name_sw, measured_by, is_active, created_at, updated_at
         FROM farm_crops WHERE workspace_id = $1 ORDER BY code, id LIMIT $2 OFFSET $3",
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_crops WHERE workspace_id = $1"#,
        workspace_id
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Insert a crop.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    c: &CropPatch,
) -> Result<CropRow, sqlx::Error> {
    sqlx::query_as!(
        CropRow,
        "INSERT INTO farm_crops (workspace_id, code, name_en, name_sw, measured_by, is_active)
         VALUES ($1, $2, $3, $4, coalesce($5, 'area'), coalesce($6, true))
         RETURNING id, code, name_en, name_sw, measured_by, is_active, created_at, updated_at",
        workspace_id,
        c.code,
        c.name_en,
        c.name_sw,
        c.measured_by,
        c.is_active,
    )
    .fetch_one(db)
    .await
}

/// Apply a patch; None when the crop isn't in this workspace.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &CropPatch,
) -> Result<Option<CropRow>, sqlx::Error> {
    sqlx::query_as!(
        CropRow,
        "UPDATE farm_crops SET
             code = coalesce($3, code), name_en = coalesce($4, name_en),
             name_sw = coalesce($5, name_sw), measured_by = coalesce($6, measured_by),
             is_active = coalesce($7, is_active), updated_at = now()
         WHERE workspace_id = $1 AND id = $2
         RETURNING id, code, name_en, name_sw, measured_by, is_active, created_at, updated_at",
        workspace_id,
        id,
        p.code,
        p.name_en,
        p.name_sw,
        p.measured_by,
        p.is_active,
    )
    .fetch_optional(db)
    .await
}
