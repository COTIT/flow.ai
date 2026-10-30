//! SQL for warehouse_section_row_shelves. Callers resolve the live row first; every statement
//! takes `workspace_id` + `row_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::warehouse_shelves::dto::{Fields, NewShelf};

/// Live shelf with its parent chain.
pub struct ShelfRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub warehouse_id: Uuid,
    pub section_id: Uuid,
    pub row_id: Uuid,
    pub shelf_code: String,
    pub shelf_name: Option<String>,
    pub shelf_level: i32,
    pub side: Option<String>,
    pub bins_per_shelf: Option<i32>,
    pub total_storage_bins: Option<i32>,
    pub max_weight_kg: Option<i32>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Parent chain of a new shelf.
pub struct Parent {
    pub workspace_id: Uuid,
    pub warehouse_id: Uuid,
    pub section_id: Uuid,
    pub row_id: Uuid,
}

/// New shelf; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    p: &Parent,
    n: &NewShelf,
) -> Result<Uuid, sqlx::Error> {
    let f = &n.fields;
    sqlx::query_scalar!(
        "INSERT INTO warehouse_section_row_shelves (workspace_id, warehouse_id, section_id,
             row_id, shelf_code, shelf_level, shelf_name, side, bins_per_shelf,
             total_storage_bins, max_weight_kg, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id",
        p.workspace_id,
        p.warehouse_id,
        p.section_id,
        p.row_id,
        n.code,
        n.level,
        f.shelf_name.clone().flatten(),
        f.side.clone().flatten(),
        f.bins_per_shelf.flatten(),
        f.total_storage_bins.flatten(),
        f.max_weight_kg.flatten(),
        f.notes.clone().flatten(),
    )
    .fetch_one(db)
    .await
}

/// Live shelf of this row. None = 404.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    row_id: Uuid,
    id: Uuid,
) -> Result<Option<ShelfRow>, sqlx::Error> {
    sqlx::query_as!(
        ShelfRow,
        "SELECT id, workspace_id, warehouse_id, section_id, row_id, shelf_code, shelf_name,
                shelf_level, side, bins_per_shelf, total_storage_bins, max_weight_kg, notes,
                created_at, updated_at
         FROM warehouse_section_row_shelves
         WHERE id = $1 AND workspace_id = $2 AND row_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        row_id,
    )
    .fetch_optional(db)
    .await
}

/// One page, oldest first, plus the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    row_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ShelfRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, workspace_id, warehouse_id, section_id, row_id, shelf_code, shelf_name,
                  shelf_level, side, bins_per_shelf, total_storage_bins, max_weight_kg, notes,
                  created_at, updated_at, count(*) OVER () AS "total!"
           FROM warehouse_section_row_shelves
           WHERE workspace_id = $1 AND row_id = $2 AND deleted_at IS NULL
           ORDER BY id
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        row_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| ShelfRow {
            id: r.id,
            workspace_id: r.workspace_id,
            warehouse_id: r.warehouse_id,
            section_id: r.section_id,
            row_id: r.row_id,
            shelf_code: r.shelf_code,
            shelf_name: r.shelf_name,
            shelf_level: r.shelf_level,
            side: r.side,
            bins_per_shelf: r.bins_per_shelf,
            total_storage_bins: r.total_storage_bins,
            max_weight_kg: r.max_weight_kg,
            notes: r.notes,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Live shelves of this row, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    row_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM warehouse_section_row_shelves
           WHERE workspace_id = $1 AND row_id = $2 AND deleted_at IS NULL"#,
        workspace_id,
        row_id,
    )
    .fetch_one(db)
    .await
}

/// Patch in one statement. False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    row_id: Uuid,
    id: Uuid,
    f: &Fields,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_section_row_shelves SET
             shelf_code = COALESCE($4, shelf_code),
             shelf_level = COALESCE($5, shelf_level),
             shelf_name = CASE WHEN $6 THEN $7 ELSE shelf_name END,
             side = CASE WHEN $8 THEN $9 ELSE side END,
             bins_per_shelf = CASE WHEN $10 THEN $11 ELSE bins_per_shelf END,
             total_storage_bins = CASE WHEN $12 THEN $13 ELSE total_storage_bins END,
             max_weight_kg = CASE WHEN $14 THEN $15 ELSE max_weight_kg END,
             notes = CASE WHEN $16 THEN $17 ELSE notes END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND row_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        row_id,
        f.shelf_code.as_deref(),
        f.shelf_level,
        f.shelf_name.is_some(),
        f.shelf_name.clone().flatten(),
        f.side.is_some(),
        f.side.clone().flatten(),
        f.bins_per_shelf.is_some(),
        f.bins_per_shelf.flatten(),
        f.total_storage_bins.is_some(),
        f.total_storage_bins.flatten(),
        f.max_weight_kg.is_some(),
        f.max_weight_kg.flatten(),
        f.notes.is_some(),
        f.notes.clone().flatten(),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete; bins below stay but become unreachable. False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    row_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_section_row_shelves SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND row_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        row_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
