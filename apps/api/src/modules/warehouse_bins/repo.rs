//! SQL for warehouse_section_row_bins. Callers resolve the live shelf first; every statement
//! takes `workspace_id` + `shelf_id` (or the warehouse for the flat list) and skips deleted rows.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::{
    warehouse_bins::dto::{Fields, NewBin},
    warehouse_shelves::repo::ShelfRow,
};

/// Bins `GET /warehouses/{id}/bins` returns at most. Laravel had no cap. One maximal section
/// layout already fills it, so a bigger warehouse gets `meta.truncated`.
// shortcut: cap + flag instead of paging; page it once bin pickers can page.
pub const WAREHOUSE_BINS_MAX: i64 = 10_000;

/// Live bins one workspace may hold. Laravel had no limit; this bounds what self-signup
/// accounts can write (a section layout alone may add 10000).
pub const WORKSPACE_BINS_MAX: i64 = 200_000;

/// True when adding `adding` bins stays within `WORKSPACE_BINS_MAX`.
pub fn fits_quota(existing: i64, adding: i64) -> bool {
    existing.saturating_add(adding) <= WORKSPACE_BINS_MAX
}

/// Live bins of the workspace (index-only on `warehouse_section_row_bins_warehouse_idx`).
pub async fn count_in_workspace(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM warehouse_section_row_bins
           WHERE workspace_id = $1 AND deleted_at IS NULL"#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Live bin with its parent chain.
pub struct BinRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub warehouse_id: Uuid,
    pub section_id: Uuid,
    pub row_id: Uuid,
    pub shelf_id: Uuid,
    pub bin_code: String,
    pub bin_name: Option<String>,
    pub side: Option<String>,
    pub position: Option<String>,
    pub bin_type: String,
    pub length_cm: Option<i32>,
    pub width_cm: Option<i32>,
    pub height_cm: Option<i32>,
    pub volume_m3: Option<f64>,
    pub max_weight_kg: Option<i32>,
    pub load_rating: Option<String>,
    pub access_type: String,
    pub priority_level: String,
    pub temperature_monitoring: bool,
    pub barcode_scanner: bool,
    pub rfid_enabled: bool,
    pub security_lock: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A bin with the codes and names of its whole path.
pub struct HierarchyRow {
    pub id: Uuid,
    pub bin_code: String,
    pub bin_name: Option<String>,
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    pub section_id: Uuid,
    pub section_code: String,
    pub section_name: String,
    pub row_id: Uuid,
    pub row_code: String,
    pub row_name: String,
    pub shelf_id: Uuid,
    pub shelf_code: String,
    pub shelf_name: Option<String>,
}

/// New bin on `shelf`; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    shelf: &ShelfRow,
    n: &NewBin,
) -> Result<Uuid, sqlx::Error> {
    let f = &n.fields;
    sqlx::query_scalar!(
        "INSERT INTO warehouse_section_row_bins (workspace_id, warehouse_id, section_id, row_id,
             shelf_id, bin_code, bin_name, side, position, bin_type, length_cm, width_cm,
             height_cm, volume_m3, max_weight_kg, load_rating, access_type, priority_level,
             temperature_monitoring, barcode_scanner, rfid_enabled, security_lock, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                 $18, COALESCE($19, false), COALESCE($20, false), COALESCE($21, false),
                 COALESCE($22, false), $23)
         RETURNING id",
        shelf.workspace_id,
        shelf.warehouse_id,
        shelf.section_id,
        shelf.row_id,
        shelf.id,
        n.code,
        f.bin_name.clone().flatten(),
        f.side.clone().flatten(),
        f.position.clone().flatten(),
        f.bin_type.as_deref().unwrap_or("standard"),
        f.length_cm.flatten(),
        f.width_cm.flatten(),
        f.height_cm.flatten(),
        f.volume_m3.flatten(),
        f.max_weight_kg.flatten(),
        f.load_rating.clone().flatten(),
        f.access_type.as_deref().unwrap_or("manual_access"),
        f.priority_level.as_deref().unwrap_or("standard"),
        f.temperature_monitoring,
        f.barcode_scanner,
        f.rfid_enabled,
        f.security_lock,
        f.notes.clone().flatten(),
    )
    .fetch_one(db)
    .await
}

/// Live bin of this shelf. None = 404.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    shelf_id: Uuid,
    id: Uuid,
) -> Result<Option<BinRow>, sqlx::Error> {
    sqlx::query_as!(
        BinRow,
        "SELECT id, workspace_id, warehouse_id, section_id, row_id, shelf_id, bin_code, bin_name,
                side, position, bin_type, length_cm, width_cm, height_cm, volume_m3,
                max_weight_kg, load_rating, access_type, priority_level, temperature_monitoring,
                barcode_scanner, rfid_enabled, security_lock, notes, created_at, updated_at
         FROM warehouse_section_row_bins
         WHERE id = $1 AND workspace_id = $2 AND shelf_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        shelf_id,
    )
    .fetch_optional(db)
    .await
}

/// One page, oldest first, plus the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    shelf_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BinRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, workspace_id, warehouse_id, section_id, row_id, shelf_id, bin_code,
                  bin_name, side, position, bin_type, length_cm, width_cm, height_cm,
                  volume_m3, max_weight_kg, load_rating, access_type, priority_level,
                  temperature_monitoring, barcode_scanner, rfid_enabled, security_lock, notes,
                  created_at, updated_at, count(*) OVER () AS "total!"
           FROM warehouse_section_row_bins
           WHERE workspace_id = $1 AND shelf_id = $2 AND deleted_at IS NULL
           ORDER BY id
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        shelf_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| BinRow {
            id: r.id,
            workspace_id: r.workspace_id,
            warehouse_id: r.warehouse_id,
            section_id: r.section_id,
            row_id: r.row_id,
            shelf_id: r.shelf_id,
            bin_code: r.bin_code,
            bin_name: r.bin_name,
            side: r.side,
            position: r.position,
            bin_type: r.bin_type,
            length_cm: r.length_cm,
            width_cm: r.width_cm,
            height_cm: r.height_cm,
            volume_m3: r.volume_m3,
            max_weight_kg: r.max_weight_kg,
            load_rating: r.load_rating,
            access_type: r.access_type,
            priority_level: r.priority_level,
            temperature_monitoring: r.temperature_monitoring,
            barcode_scanner: r.barcode_scanner,
            rfid_enabled: r.rfid_enabled,
            security_lock: r.security_lock,
            notes: r.notes,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Live bins of this shelf, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    shelf_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM warehouse_section_row_bins
           WHERE workspace_id = $1 AND shelf_id = $2 AND deleted_at IS NULL"#,
        workspace_id,
        shelf_id,
    )
    .fetch_one(db)
    .await
}

/// Bins of a live warehouse whose whole path is live, in Laravel's order (section, row,
/// shelf, code), at most `limit`.
pub async fn of_warehouse(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    limit: i64,
) -> Result<Vec<HierarchyRow>, sqlx::Error> {
    sqlx::query_as!(
        HierarchyRow,
        "SELECT b.id, b.bin_code, b.bin_name, w.id AS warehouse_id, w.name AS warehouse_name,
                s.id AS section_id, s.section_code, s.section_name, r.id AS row_id, r.row_code,
                r.row_name, sh.id AS shelf_id, sh.shelf_code, sh.shelf_name
         FROM warehouse_section_row_bins b
         JOIN warehouses w ON w.id = b.warehouse_id AND w.deleted_at IS NULL
         JOIN warehouse_sections s ON s.id = b.section_id AND s.deleted_at IS NULL
         JOIN warehouse_section_rows r ON r.id = b.row_id AND r.deleted_at IS NULL
         JOIN warehouse_section_row_shelves sh ON sh.id = b.shelf_id AND sh.deleted_at IS NULL
         WHERE b.workspace_id = $1 AND b.warehouse_id = $2 AND b.deleted_at IS NULL
         ORDER BY s.id, r.id, sh.id, b.bin_code
         LIMIT $3",
        workspace_id,
        warehouse_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Patch in one statement. False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    shelf_id: Uuid,
    id: Uuid,
    f: &Fields,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_section_row_bins SET
             bin_code = COALESCE($4, bin_code),
             bin_name = CASE WHEN $5 THEN $6 ELSE bin_name END,
             side = CASE WHEN $7 THEN $8 ELSE side END,
             position = CASE WHEN $9 THEN $10 ELSE position END,
             bin_type = COALESCE($11, bin_type),
             length_cm = CASE WHEN $12 THEN $13 ELSE length_cm END,
             width_cm = CASE WHEN $14 THEN $15 ELSE width_cm END,
             height_cm = CASE WHEN $16 THEN $17 ELSE height_cm END,
             volume_m3 = CASE WHEN $18 THEN $19 ELSE volume_m3 END,
             max_weight_kg = CASE WHEN $20 THEN $21 ELSE max_weight_kg END,
             load_rating = CASE WHEN $22 THEN $23 ELSE load_rating END,
             access_type = COALESCE($24, access_type),
             priority_level = COALESCE($25, priority_level),
             temperature_monitoring = COALESCE($26, temperature_monitoring),
             barcode_scanner = COALESCE($27, barcode_scanner),
             rfid_enabled = COALESCE($28, rfid_enabled),
             security_lock = COALESCE($29, security_lock),
             notes = CASE WHEN $30 THEN $31 ELSE notes END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND shelf_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        shelf_id,
        f.bin_code.as_deref(),
        f.bin_name.is_some(),
        f.bin_name.clone().flatten(),
        f.side.is_some(),
        f.side.clone().flatten(),
        f.position.is_some(),
        f.position.clone().flatten(),
        f.bin_type.as_deref(),
        f.length_cm.is_some(),
        f.length_cm.flatten(),
        f.width_cm.is_some(),
        f.width_cm.flatten(),
        f.height_cm.is_some(),
        f.height_cm.flatten(),
        f.volume_m3.is_some(),
        f.volume_m3.flatten(),
        f.max_weight_kg.is_some(),
        f.max_weight_kg.flatten(),
        f.load_rating.is_some(),
        f.load_rating.clone().flatten(),
        f.access_type.as_deref(),
        f.priority_level.as_deref(),
        f.temperature_monitoring,
        f.barcode_scanner,
        f.rfid_enabled,
        f.security_lock,
        f.notes.is_some(),
        f.notes.clone().flatten(),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete. False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    shelf_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_section_row_bins SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND shelf_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        shelf_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_boundary() {
        assert!(fits_quota(0, WORKSPACE_BINS_MAX));
        assert!(fits_quota(WORKSPACE_BINS_MAX - 1, 1));
        assert!(!fits_quota(WORKSPACE_BINS_MAX, 1));
        assert!(!fits_quota(1, i64::MAX));
    }
}
