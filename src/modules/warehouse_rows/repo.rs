//! SQL for warehouse_section_rows. Callers resolve the live section first; every statement
//! takes `workspace_id` + `section_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::warehouse_rows::dto::Fields;

/// Live row.
pub struct RowRow {
    pub id: Uuid,
    pub row_code: String,
    pub row_name: String,
    pub orientation: Option<String>,
    pub row_type: Option<String>,
    pub length_meters: f64,
    pub width_meters: f64,
    pub height_meters: f64,
    pub calculated_volume_cubic_meters: Option<f64>,
    pub number_of_shelves: Option<i32>,
    pub shelf_spacing_cm: Option<i32>,
    pub position_x: Option<f64>,
    pub position_y: Option<f64>,
    pub access_side: Option<String>,
    pub aisle_width_cm: Option<i32>,
    pub equipment_access: Option<String>,
    pub priority_level: String,
    pub temperature_zone: String,
    pub notes: Option<String>,
    pub enable_automatic_bin_labeling: bool,
    pub generate_qr_codes_for_shelves: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New row under `section_id` of `warehouse_id`; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    section_id: Uuid,
    f: &Fields,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO warehouse_section_rows (workspace_id, warehouse_id, section_id, row_code,
             row_name, length_meters, width_meters, height_meters, orientation, row_type,
             calculated_volume_cubic_meters, number_of_shelves, shelf_spacing_cm, position_x,
             position_y, access_side, aisle_width_cm, equipment_access, priority_level,
             temperature_zone, notes, enable_automatic_bin_labeling,
             generate_qr_codes_for_shelves)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                 $18, $19, $20, $21, COALESCE($22, false), COALESCE($23, false))
         RETURNING id",
        workspace_id,
        warehouse_id,
        section_id,
        f.row_code.as_deref().unwrap_or_default(),
        f.row_name.as_deref().unwrap_or_default(),
        f.length_meters.unwrap_or_default(),
        f.width_meters.unwrap_or_default(),
        f.height_meters.unwrap_or_default(),
        f.orientation.clone().flatten(),
        f.row_type.clone().flatten(),
        f.calculated_volume_cubic_meters.flatten(),
        f.number_of_shelves.flatten(),
        f.shelf_spacing_cm.flatten(),
        f.position_x.flatten(),
        f.position_y.flatten(),
        f.access_side.clone().flatten(),
        f.aisle_width_cm.flatten(),
        f.equipment_access.clone().flatten(),
        f.priority_level.as_deref().unwrap_or("standard"),
        f.temperature_zone.as_deref().unwrap_or("ambient"),
        f.notes.clone().flatten(),
        f.enable_automatic_bin_labeling,
        f.generate_qr_codes_for_shelves,
    )
    .fetch_one(db)
    .await
}

/// True when a live row of this section uses `code` (any case), other than `except`.
pub async fn code_taken(
    db: impl PgExecutor<'_>,
    section_id: Uuid,
    code: &str,
    except: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM warehouse_section_rows
               WHERE section_id = $1 AND lower(row_code) = lower($2) AND deleted_at IS NULL
                 AND ($3::uuid IS NULL OR id <> $3)
           ) AS "e!""#,
        section_id,
        code,
        except,
    )
    .fetch_one(db)
    .await
}

/// Live row of this section. None = 404.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    section_id: Uuid,
    id: Uuid,
) -> Result<Option<RowRow>, sqlx::Error> {
    sqlx::query_as!(
        RowRow,
        "SELECT id, row_code, row_name, orientation, row_type, length_meters, width_meters,
                height_meters, calculated_volume_cubic_meters, number_of_shelves,
                shelf_spacing_cm, position_x, position_y, access_side, aisle_width_cm,
                equipment_access, priority_level, temperature_zone, notes,
                enable_automatic_bin_labeling, generate_qr_codes_for_shelves, created_at,
                updated_at
         FROM warehouse_section_rows
         WHERE id = $1 AND workspace_id = $2 AND section_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        section_id,
    )
    .fetch_optional(db)
    .await
}

/// One page, oldest first, plus the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    section_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<RowRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, row_code, row_name, orientation, row_type, length_meters, width_meters,
                  height_meters, calculated_volume_cubic_meters, number_of_shelves,
                  shelf_spacing_cm, position_x, position_y, access_side, aisle_width_cm,
                  equipment_access, priority_level, temperature_zone, notes,
                  enable_automatic_bin_labeling, generate_qr_codes_for_shelves, created_at,
                  updated_at, count(*) OVER () AS "total!"
           FROM warehouse_section_rows
           WHERE workspace_id = $1 AND section_id = $2 AND deleted_at IS NULL
           ORDER BY id
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        section_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| RowRow {
            id: r.id,
            row_code: r.row_code,
            row_name: r.row_name,
            orientation: r.orientation,
            row_type: r.row_type,
            length_meters: r.length_meters,
            width_meters: r.width_meters,
            height_meters: r.height_meters,
            calculated_volume_cubic_meters: r.calculated_volume_cubic_meters,
            number_of_shelves: r.number_of_shelves,
            shelf_spacing_cm: r.shelf_spacing_cm,
            position_x: r.position_x,
            position_y: r.position_y,
            access_side: r.access_side,
            aisle_width_cm: r.aisle_width_cm,
            equipment_access: r.equipment_access,
            priority_level: r.priority_level,
            temperature_zone: r.temperature_zone,
            notes: r.notes,
            enable_automatic_bin_labeling: r.enable_automatic_bin_labeling,
            generate_qr_codes_for_shelves: r.generate_qr_codes_for_shelves,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Live rows of this section, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    section_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM warehouse_section_rows
           WHERE workspace_id = $1 AND section_id = $2 AND deleted_at IS NULL"#,
        workspace_id,
        section_id,
    )
    .fetch_one(db)
    .await
}

/// Patch in one statement. False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    section_id: Uuid,
    id: Uuid,
    f: &Fields,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_section_rows SET
             row_code = COALESCE($4, row_code),
             row_name = COALESCE($5, row_name),
             length_meters = COALESCE($6, length_meters),
             width_meters = COALESCE($7, width_meters),
             height_meters = COALESCE($8, height_meters),
             orientation = CASE WHEN $9 THEN $10 ELSE orientation END,
             row_type = CASE WHEN $11 THEN $12 ELSE row_type END,
             calculated_volume_cubic_meters =
                 CASE WHEN $13 THEN $14 ELSE calculated_volume_cubic_meters END,
             number_of_shelves = CASE WHEN $15 THEN $16 ELSE number_of_shelves END,
             shelf_spacing_cm = CASE WHEN $17 THEN $18 ELSE shelf_spacing_cm END,
             position_x = CASE WHEN $19 THEN $20 ELSE position_x END,
             position_y = CASE WHEN $21 THEN $22 ELSE position_y END,
             access_side = CASE WHEN $23 THEN $24 ELSE access_side END,
             aisle_width_cm = CASE WHEN $25 THEN $26 ELSE aisle_width_cm END,
             equipment_access = CASE WHEN $27 THEN $28 ELSE equipment_access END,
             priority_level = COALESCE($29, priority_level),
             temperature_zone = COALESCE($30, temperature_zone),
             notes = CASE WHEN $31 THEN $32 ELSE notes END,
             enable_automatic_bin_labeling = COALESCE($33, enable_automatic_bin_labeling),
             generate_qr_codes_for_shelves = COALESCE($34, generate_qr_codes_for_shelves),
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND section_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        section_id,
        f.row_code.as_deref(),
        f.row_name.as_deref(),
        f.length_meters,
        f.width_meters,
        f.height_meters,
        f.orientation.is_some(),
        f.orientation.clone().flatten(),
        f.row_type.is_some(),
        f.row_type.clone().flatten(),
        f.calculated_volume_cubic_meters.is_some(),
        f.calculated_volume_cubic_meters.flatten(),
        f.number_of_shelves.is_some(),
        f.number_of_shelves.flatten(),
        f.shelf_spacing_cm.is_some(),
        f.shelf_spacing_cm.flatten(),
        f.position_x.is_some(),
        f.position_x.flatten(),
        f.position_y.is_some(),
        f.position_y.flatten(),
        f.access_side.is_some(),
        f.access_side.clone().flatten(),
        f.aisle_width_cm.is_some(),
        f.aisle_width_cm.flatten(),
        f.equipment_access.is_some(),
        f.equipment_access.clone().flatten(),
        f.priority_level.as_deref(),
        f.temperature_zone.as_deref(),
        f.notes.is_some(),
        f.notes.clone().flatten(),
        f.enable_automatic_bin_labeling,
        f.generate_qr_codes_for_shelves,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete; shelves below stay but become unreachable. False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    section_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_section_rows SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND section_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        section_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
