//! SQL for warehouse_sections and the auto-layout. Every statement takes `workspace_id` and the
//! parent warehouse, and only sees live rows under a live warehouse.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::warehouse_sections::dto::Fields;

/// Live section with its warehouse name (SectionResource source).
pub struct SectionRow {
    pub id: Uuid,
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    pub section_name: String,
    pub section_code: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub rows: Option<i32>,
    pub shelves_per_row: Option<i32>,
    pub bins_per_shelf: Option<i32>,
    pub total_bins: Option<i32>,
    pub zone: Option<String>,
    pub temperature_control: String,
    pub restricted_access: bool,
    pub assigned_manager: Option<Uuid>,
    pub priority_level: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List filter (Laravel ListSection::paginated). `search` = escaped ILIKE pattern.
pub struct ListFilter<'a> {
    pub category: Option<&'a str>,
    pub zone: Option<&'a str>,
    pub priority_level: Option<&'a str>,
    pub restricted_access: Option<bool>,
    pub search: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

/// Generated layout of one section as insert columns, in creation order. Children name their
/// parent by code, and ids are matched by code, never by RETURNING order.
#[derive(Default)]
pub struct Layout {
    pub shelves_per_row: i32,
    pub bins_per_shelf: i32,
    pub rows: Columns,
    /// `parents` = row codes.
    pub shelves: Columns,
    /// `parents` = shelf codes.
    pub bins: Columns,
}

/// One level of a generated layout; `parents` and `levels` stay empty where unused.
#[derive(Default)]
pub struct Columns {
    pub parents: Vec<String>,
    pub codes: Vec<String>,
    pub names: Vec<String>,
    pub levels: Vec<i32>,
}

impl Columns {
    /// Append one generated child.
    pub fn push(&mut self, parent: Option<&str>, code: String, name: String, level: Option<i32>) {
        self.parents.extend(parent.map(str::to_owned));
        self.codes.push(code);
        self.names.push(name);
        self.levels.extend(level);
    }
}

/// New section; returns its id. Create always sends `Some` for every field.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    name: &str,
    code: &str,
    f: &Fields,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO warehouse_sections (workspace_id, warehouse_id, section_name, section_code,
             category, description, rows, shelves_per_row, bins_per_shelf, total_bins, zone,
             temperature_control, restricted_access, assigned_manager, priority_level)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, COALESCE($12, 'ambient'),
                 COALESCE($13, false), $14, COALESCE($15, 'standard'))
         RETURNING id",
        workspace_id,
        warehouse_id,
        name,
        code,
        f.category.clone().flatten(),
        f.description.clone().flatten(),
        f.rows.flatten(),
        f.shelves_per_row.flatten(),
        f.bins_per_shelf.flatten(),
        f.total_bins.flatten(),
        f.zone.clone().flatten(),
        f.temperature_control.as_deref(),
        f.restricted_access,
        f.assigned_manager.flatten(),
        f.priority_level.as_deref(),
    )
    .fetch_one(db)
    .await
}

/// Insert a generated layout in three statements (rows, shelves, bins), whatever its size.
pub async fn insert_layout(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    section_id: Uuid,
    temperature: &str,
    l: &Layout,
) -> Result<(), sqlx::Error> {
    // Laravel SectionCreated defaults: 10 x 1.2 x 6 m rows.
    let rows: HashMap<String, Uuid> = sqlx::query!(
        "INSERT INTO warehouse_section_rows (workspace_id, warehouse_id, section_id, row_code,
             row_name, length_meters, width_meters, height_meters, number_of_shelves,
             temperature_zone)
         SELECT $1, $2, $3, c, n, 10.0, 1.2, 6.0, $6, $7
         FROM unnest($4::text[], $5::text[]) WITH ORDINALITY AS t(c, n, i)
         ORDER BY i
         RETURNING id, row_code",
        workspace_id,
        warehouse_id,
        section_id,
        &l.rows.codes,
        &l.rows.names,
        l.shelves_per_row,
        temperature,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| (r.row_code, r.id))
    .collect();
    let shelf_rows = ids_of(&l.shelves.parents, &rows)?;
    let shelves: HashMap<String, (Uuid, Uuid)> = sqlx::query!(
        "INSERT INTO warehouse_section_row_shelves (workspace_id, warehouse_id, section_id, row_id,
             shelf_code, shelf_name, shelf_level, bins_per_shelf)
         SELECT $1, $2, $3, r, c, n, lvl, $8
         FROM unnest($4::uuid[], $5::text[], $6::text[], $7::int[]) WITH ORDINALITY
              AS t(r, c, n, lvl, i)
         ORDER BY i
         RETURNING id, row_id, shelf_code",
        workspace_id,
        warehouse_id,
        section_id,
        &shelf_rows,
        &l.shelves.codes,
        &l.shelves.names,
        &l.shelves.levels,
        l.bins_per_shelf,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| (r.shelf_code, (r.row_id, r.id)))
    .collect();
    let parents = ids_of(&l.bins.parents, &shelves)?;
    let (bin_rows, bin_shelves): (Vec<Uuid>, Vec<Uuid>) = parents.into_iter().unzip();
    sqlx::query!(
        "INSERT INTO warehouse_section_row_bins (workspace_id, warehouse_id, section_id, row_id,
             shelf_id, bin_code, bin_name)
         SELECT $1, $2, $3, r, s, c, n
         FROM unnest($4::uuid[], $5::uuid[], $6::text[], $7::text[]) WITH ORDINALITY
              AS t(r, s, c, n, i)
         ORDER BY i",
        workspace_id,
        warehouse_id,
        section_id,
        &bin_rows,
        &bin_shelves,
        &l.bins.codes,
        &l.bins.names,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Parent ids by code. A miss would misalign the unnest columns, so it fails the insert.
fn ids_of<T: Copy>(codes: &[String], ids: &HashMap<String, T>) -> Result<Vec<T>, sqlx::Error> {
    codes
        .iter()
        .map(|c| ids.get(c).copied().ok_or(sqlx::Error::RowNotFound))
        .collect()
}

/// Live section under a live warehouse of this workspace. None = 404.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: Uuid,
) -> Result<Option<SectionRow>, sqlx::Error> {
    sqlx::query_as!(
        SectionRow,
        "SELECT s.id, s.warehouse_id, w.name AS warehouse_name, s.section_name, s.section_code,
                s.category, s.description, s.rows, s.shelves_per_row, s.bins_per_shelf,
                s.total_bins, s.zone, s.temperature_control, s.restricted_access,
                s.assigned_manager, s.priority_level, s.created_at, s.updated_at
         FROM warehouse_sections s
         JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
         WHERE s.id = $1 AND s.workspace_id = $2 AND s.warehouse_id = $3
           AND s.deleted_at IS NULL",
        id,
        workspace_id,
        warehouse_id,
    )
    .fetch_optional(db)
    .await
}

/// One page sorted by name (Laravel orderBy section_name), plus the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<SectionRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT s.id, s.warehouse_id, w.name AS warehouse_name, s.section_name, s.section_code,
                  s.category, s.description, s.rows, s.shelves_per_row, s.bins_per_shelf,
                  s.total_bins, s.zone, s.temperature_control, s.restricted_access,
                  s.assigned_manager, s.priority_level, s.created_at, s.updated_at,
                  count(*) OVER () AS "total!"
           FROM warehouse_sections s
           JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
           WHERE s.workspace_id = $1 AND s.warehouse_id = $2 AND s.deleted_at IS NULL
             AND ($3::text IS NULL OR s.category = $3)
             AND ($4::text IS NULL OR s.zone = $4)
             AND ($5::text IS NULL OR s.priority_level = $5)
             AND ($6::bool IS NULL OR s.restricted_access = $6)
             AND ($7::text IS NULL OR s.section_name ILIKE $7 OR s.section_code ILIKE $7
                  OR s.description ILIKE $7)
           ORDER BY s.section_name, s.id
           LIMIT $8 OFFSET $9"#,
        workspace_id,
        warehouse_id,
        f.category,
        f.zone,
        f.priority_level,
        f.restricted_access,
        f.search,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| SectionRow {
            id: r.id,
            warehouse_id: r.warehouse_id,
            warehouse_name: r.warehouse_name,
            section_name: r.section_name,
            section_code: r.section_code,
            category: r.category,
            description: r.description,
            rows: r.rows,
            shelves_per_row: r.shelves_per_row,
            bins_per_shelf: r.bins_per_shelf,
            total_bins: r.total_bins,
            zone: r.zone,
            temperature_control: r.temperature_control,
            restricted_access: r.restricted_access,
            assigned_manager: r.assigned_manager,
            priority_level: r.priority_level,
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
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM warehouse_sections s
           WHERE s.workspace_id = $1 AND s.warehouse_id = $2 AND s.deleted_at IS NULL
             AND ($3::text IS NULL OR s.category = $3)
             AND ($4::text IS NULL OR s.zone = $4)
             AND ($5::text IS NULL OR s.priority_level = $5)
             AND ($6::bool IS NULL OR s.restricted_access = $6)
             AND ($7::text IS NULL OR s.section_name ILIKE $7 OR s.section_code ILIKE $7
                  OR s.description ILIKE $7)"#,
        workspace_id,
        warehouse_id,
        f.category,
        f.zone,
        f.priority_level,
        f.restricted_access,
        f.search,
    )
    .fetch_one(db)
    .await
}

/// Patch in one statement. False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: Uuid,
    name: &str,
    code: &str,
    f: &Fields,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_sections SET
             section_name = $4,
             section_code = $5,
             category = CASE WHEN $6 THEN $7 ELSE category END,
             description = CASE WHEN $8 THEN $9 ELSE description END,
             rows = CASE WHEN $10 THEN $11 ELSE rows END,
             shelves_per_row = CASE WHEN $12 THEN $13 ELSE shelves_per_row END,
             bins_per_shelf = CASE WHEN $14 THEN $15 ELSE bins_per_shelf END,
             total_bins = CASE WHEN $16 THEN $17 ELSE total_bins END,
             zone = CASE WHEN $18 THEN $19 ELSE zone END,
             temperature_control = COALESCE($20, temperature_control),
             restricted_access = COALESCE($21, restricted_access),
             assigned_manager = CASE WHEN $22 THEN $23 ELSE assigned_manager END,
             priority_level = COALESCE($24, priority_level),
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND warehouse_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        warehouse_id,
        name,
        code,
        f.category.is_some(),
        f.category.clone().flatten(),
        f.description.is_some(),
        f.description.clone().flatten(),
        f.rows.is_some(),
        f.rows.flatten(),
        f.shelves_per_row.is_some(),
        f.shelves_per_row.flatten(),
        f.bins_per_shelf.is_some(),
        f.bins_per_shelf.flatten(),
        f.total_bins.is_some(),
        f.total_bins.flatten(),
        f.zone.is_some(),
        f.zone.clone().flatten(),
        f.temperature_control.as_deref(),
        f.restricted_access,
        f.assigned_manager.is_some(),
        f.assigned_manager.flatten(),
        f.priority_level.as_deref(),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete; rows below stay (Laravel parity) but become unreachable. False = no such row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouse_sections SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND warehouse_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        warehouse_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
