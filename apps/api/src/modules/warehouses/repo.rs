//! SQL for warehouses. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::warehouses::dto::Fields;

/// Live warehouse, all columns the resource prints.
pub struct WarehouseRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub is_store: Option<bool>,
    pub default_warehouse: bool,
    pub stock_count_enabled: bool,
    pub pickup_enabled: bool,
    pub auto_generate_transfer_list: bool,
    pub total_sum_enabled: bool,
    pub user_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    pub manager_assistant_id: Option<Uuid>,
    pub location: Option<Value>,
    pub status: Option<String>,
    pub kind: Option<String>,
    pub dimensions: Option<Value>,
    pub loading_docks: Option<i32>,
    pub sections: Option<i32>,
    pub rows_per_sections: Option<i32>,
    pub shelves_per_row: Option<i32>,
    pub bins_per_shelf: Option<i32>,
    pub management_contact: Option<Value>,
    pub operating_hours: Option<Value>,
    pub additional_settings: Option<Value>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WarehouseRow {
    /// User ids the resource embeds.
    pub fn people(&self) -> impl Iterator<Item = Uuid> {
        [self.user_id, self.manager_id, self.manager_assistant_id]
            .into_iter()
            .flatten()
    }
}

/// Live child of a listed warehouse (WarehouseCompactedResource source).
pub struct ChildRow {
    pub parent_id: Uuid,
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub alias: Option<String>,
    pub is_store: Option<bool>,
    pub pickup_enabled: bool,
    pub status: Option<String>,
    pub kind: Option<String>,
}

/// One live warehouse as WarehouseCompactedResource prints it (embedded in other resources).
pub struct CompactRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub alias: Option<String>,
    pub is_store: Option<bool>,
    pub pickup_enabled: bool,
    pub parent_name: Option<String>,
    pub status: Option<String>,
    pub kind: Option<String>,
}

/// A warehouse as a product's fallback stock row embeds it.
pub struct StockRef {
    pub id: Uuid,
    pub name: String,
    pub alias: Option<String>,
    pub is_store: Option<bool>,
    pub pickup_enabled: bool,
    pub location: Option<Value>,
}

/// List filter (Laravel ListWarehouse).
pub struct ListFilter {
    pub parent_id: Option<Uuid>,
    /// Only when `parent_id` is None.
    pub roots_only: bool,
    pub pickup_only: bool,
    pub limit: i64,
    pub offset: i64,
}

/// Per-workspace lock for the "one default" rule: concurrent creates/updates take turns,
/// so two first warehouses can't both read "no default yet".
pub async fn lock_default(conn: &mut PgConnection, workspace_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext('warehouses.default'), hashtext($1::text))",
        workspace_id.to_string(),
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// True when a live default exists.
pub async fn has_default(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM warehouses
               WHERE workspace_id = $1 AND default_warehouse AND deleted_at IS NULL
           ) AS "e!""#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Unset the live default, except `keep`.
pub async fn clear_default(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    keep: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouses SET default_warehouse = false, updated_at = now()
         WHERE workspace_id = $1 AND default_warehouse AND deleted_at IS NULL
           AND ($2::uuid IS NULL OR id <> $2)",
        workspace_id,
        keep,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Make `id` the default. False = no such live row.
pub async fn set_default(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouses SET default_warehouse = true, updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// `(is_store, default_warehouse)` of a live warehouse, row locked until commit (`NO KEY`:
/// stock inserts referencing the warehouse keep going). None = no such row.
pub async fn flags_locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<(bool, bool)>, sqlx::Error> {
    Ok(sqlx::query!(
        r#"SELECT coalesce(is_store, false) AS "is_store!", default_warehouse FROM warehouses
           WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
           FOR NO KEY UPDATE"#,
        id,
        workspace_id,
    )
    .fetch_optional(conn)
    .await?
    .map(|r| (r.is_store, r.default_warehouse)))
}

/// `default_warehouse` of a live warehouse. None = no such row.
pub async fn default_flag(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT default_warehouse FROM warehouses
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// True when `slug` is used by a live warehouse of the workspace.
pub async fn slug_taken(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM warehouses
               WHERE workspace_id = $1 AND slug = $2 AND deleted_at IS NULL
           ) AS "e!""#,
        workspace_id,
        slug,
    )
    .fetch_one(db)
    .await
}

/// True when `id` is a live warehouse of the workspace.
pub async fn exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM warehouses
               WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
           ) AS "e!""#,
        id,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// New row; returns its id. Create always sends every field (`Some`), see `dto::fields`.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    name: &str,
    slug: &str,
    parent_id: Option<Uuid>,
    default_warehouse: bool,
    f: &Fields,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO warehouses (
             workspace_id, name, slug, parent_id, default_warehouse, alias, description,
             stock_count_enabled, is_store, pickup_enabled, auto_generate_transfer_list,
             total_sum_enabled, user_id, manager_id, manager_assistant_id, location, status,
             type, dimensions, loading_docks, sections, rows_per_sections, shelves_per_row,
             bins_per_shelf, management_contact, operating_hours, additional_settings, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, COALESCE($8, false), $9, COALESCE($10, false),
                 COALESCE($11, false), COALESCE($12, false), $13, $14, $15, $16, $17, $18, $19,
                 $20, $21, $22, $23, $24, $25, $26, $27, $28)
         RETURNING id",
        workspace_id,
        name,
        slug,
        parent_id,
        default_warehouse,
        f.alias.clone().flatten(),
        f.description.clone().flatten(),
        f.stock_count_enabled,
        f.is_store.flatten(),
        f.pickup_enabled,
        f.auto_generate_transfer_list,
        f.total_sum_enabled,
        f.user_id.flatten(),
        f.manager_id.flatten(),
        f.manager_assistant_id.flatten(),
        f.location.clone().flatten(),
        f.status.clone().flatten(),
        f.kind.clone().flatten(),
        f.dimensions.clone().flatten(),
        f.loading_docks.flatten(),
        f.sections.flatten(),
        f.rows_per_sections.flatten(),
        f.shelves_per_row.flatten(),
        f.bins_per_shelf.flatten(),
        f.management_contact.clone().flatten(),
        f.operating_hours.clone().flatten(),
        f.additional_settings.clone().flatten(),
        f.meta.clone().flatten(),
    )
    .fetch_one(db)
    .await
}

/// Patch in one statement: `None` keeps the column, `Some(None)` clears it.
/// False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    name: &str,
    default_warehouse: Option<bool>,
    f: &Fields,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouses SET
             name = $3,
             default_warehouse = COALESCE($4, default_warehouse),
             alias = CASE WHEN $5 THEN $6 ELSE alias END,
             description = CASE WHEN $7 THEN $8 ELSE description END,
             stock_count_enabled = COALESCE($9, stock_count_enabled),
             is_store = CASE WHEN $10 THEN $11 ELSE is_store END,
             pickup_enabled = COALESCE($12, pickup_enabled),
             auto_generate_transfer_list = COALESCE($13, auto_generate_transfer_list),
             total_sum_enabled = COALESCE($14, total_sum_enabled),
             user_id = CASE WHEN $15 THEN $16 ELSE user_id END,
             manager_id = CASE WHEN $17 THEN $18 ELSE manager_id END,
             manager_assistant_id = CASE WHEN $19 THEN $20 ELSE manager_assistant_id END,
             location = CASE WHEN $21 THEN $22 ELSE location END,
             status = CASE WHEN $23 THEN $24 ELSE status END,
             type = CASE WHEN $25 THEN $26 ELSE type END,
             dimensions = CASE WHEN $27 THEN $28 ELSE dimensions END,
             loading_docks = CASE WHEN $29 THEN $30 ELSE loading_docks END,
             sections = CASE WHEN $31 THEN $32 ELSE sections END,
             rows_per_sections = CASE WHEN $33 THEN $34 ELSE rows_per_sections END,
             shelves_per_row = CASE WHEN $35 THEN $36 ELSE shelves_per_row END,
             bins_per_shelf = CASE WHEN $37 THEN $38 ELSE bins_per_shelf END,
             management_contact = CASE WHEN $39 THEN $40 ELSE management_contact END,
             operating_hours = CASE WHEN $41 THEN $42 ELSE operating_hours END,
             additional_settings = CASE WHEN $43 THEN $44 ELSE additional_settings END,
             meta = CASE WHEN $45 THEN $46 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        name,
        default_warehouse,
        f.alias.is_some(),
        f.alias.clone().flatten(),
        f.description.is_some(),
        f.description.clone().flatten(),
        f.stock_count_enabled,
        f.is_store.is_some(),
        f.is_store.flatten(),
        f.pickup_enabled,
        f.auto_generate_transfer_list,
        f.total_sum_enabled,
        f.user_id.is_some(),
        f.user_id.flatten(),
        f.manager_id.is_some(),
        f.manager_id.flatten(),
        f.manager_assistant_id.is_some(),
        f.manager_assistant_id.flatten(),
        f.location.is_some(),
        f.location.clone().flatten(),
        f.status.is_some(),
        f.status.clone().flatten(),
        f.kind.is_some(),
        f.kind.clone().flatten(),
        f.dimensions.is_some(),
        f.dimensions.clone().flatten(),
        f.loading_docks.is_some(),
        f.loading_docks.flatten(),
        f.sections.is_some(),
        f.sections.flatten(),
        f.rows_per_sections.is_some(),
        f.rows_per_sections.flatten(),
        f.shelves_per_row.is_some(),
        f.shelves_per_row.flatten(),
        f.bins_per_shelf.is_some(),
        f.bins_per_shelf.flatten(),
        f.management_contact.is_some(),
        f.management_contact.clone().flatten(),
        f.operating_hours.is_some(),
        f.operating_hours.clone().flatten(),
        f.additional_settings.is_some(),
        f.additional_settings.clone().flatten(),
        f.meta.is_some(),
        f.meta.clone().flatten(),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Live warehouse of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<WarehouseRow>, sqlx::Error> {
    sqlx::query_as!(
        WarehouseRow,
        r#"SELECT id, name, slug, alias, description, is_store, default_warehouse,
                  stock_count_enabled, pickup_enabled, auto_generate_transfer_list,
                  total_sum_enabled, user_id, manager_id, manager_assistant_id, location,
                  status, type AS kind, dimensions, loading_docks, sections, rows_per_sections,
                  shelves_per_row, bins_per_shelf, management_contact, operating_hours,
                  additional_settings, meta, created_at, updated_at
           FROM warehouses
           WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL"#,
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Live warehouses of the workspace among `ids`.
pub async fn find_many(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<WarehouseRow>, sqlx::Error> {
    sqlx::query_as!(
        WarehouseRow,
        r#"SELECT id, name, slug, alias, description, is_store, default_warehouse,
                  stock_count_enabled, pickup_enabled, auto_generate_transfer_list,
                  total_sum_enabled, user_id, manager_id, manager_assistant_id, location,
                  status, type AS kind, dimensions, loading_docks, sections, rows_per_sections,
                  shelves_per_row, bins_per_shelf, management_contact, operating_hours,
                  additional_settings, meta, created_at, updated_at
           FROM warehouses
           WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL
           ORDER BY id"#,
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
    f: &ListFilter,
) -> Result<(Vec<WarehouseRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, slug, alias, description, is_store, default_warehouse,
                  stock_count_enabled, pickup_enabled, auto_generate_transfer_list,
                  total_sum_enabled, user_id, manager_id, manager_assistant_id, location,
                  status, type AS kind, dimensions, loading_docks, sections, rows_per_sections,
                  shelves_per_row, bins_per_shelf, management_contact, operating_hours,
                  additional_settings, meta, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM warehouses
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::uuid IS NULL OR parent_id = $2)
             AND ($2::uuid IS NOT NULL OR NOT $3 OR parent_id IS NULL)
             AND (NOT $4 OR pickup_enabled)
           ORDER BY id
           LIMIT $5 OFFSET $6"#,
        workspace_id,
        f.parent_id,
        f.roots_only,
        f.pickup_only,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| WarehouseRow {
            id: r.id,
            name: r.name,
            slug: r.slug,
            alias: r.alias,
            description: r.description,
            is_store: r.is_store,
            default_warehouse: r.default_warehouse,
            stock_count_enabled: r.stock_count_enabled,
            pickup_enabled: r.pickup_enabled,
            auto_generate_transfer_list: r.auto_generate_transfer_list,
            total_sum_enabled: r.total_sum_enabled,
            user_id: r.user_id,
            manager_id: r.manager_id,
            manager_assistant_id: r.manager_assistant_id,
            location: r.location,
            status: r.status,
            kind: r.kind,
            dimensions: r.dimensions,
            loading_docks: r.loading_docks,
            sections: r.sections,
            rows_per_sections: r.rows_per_sections,
            shelves_per_row: r.shelves_per_row,
            bins_per_shelf: r.bins_per_shelf,
            management_contact: r.management_contact,
            operating_hours: r.operating_hours,
            additional_settings: r.additional_settings,
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
    f: &ListFilter,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM warehouses
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::uuid IS NULL OR parent_id = $2)
             AND ($2::uuid IS NOT NULL OR NOT $3 OR parent_id IS NULL)
             AND (NOT $4 OR pickup_enabled)"#,
        workspace_id,
        f.parent_id,
        f.roots_only,
        f.pickup_only,
    )
    .fetch_one(db)
    .await
}

/// Live warehouse of `workspace_id` in compact form; parent name only while the parent is live.
pub async fn compact(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<CompactRow>, sqlx::Error> {
    sqlx::query_as!(
        CompactRow,
        r#"SELECT w.id, w.name, w.slug, w.alias, w.is_store, w.pickup_enabled,
                  p.name AS "parent_name?", w.status, w.type AS kind
           FROM warehouses w
           LEFT JOIN warehouses p ON p.id = w.parent_id AND p.workspace_id = w.workspace_id
                                 AND p.deleted_at IS NULL
           WHERE w.id = $1 AND w.workspace_id = $2 AND w.deleted_at IS NULL"#,
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Compact form of the live warehouses among `ids`.
pub async fn compact_many(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<CompactRow>, sqlx::Error> {
    sqlx::query_as!(
        CompactRow,
        r#"SELECT w.id, w.name, w.slug, w.alias, w.is_store, w.pickup_enabled,
                  p.name AS "parent_name?", w.status, w.type AS kind
           FROM warehouses w
           LEFT JOIN warehouses p ON p.id = w.parent_id AND p.workspace_id = w.workspace_id
                                 AND p.deleted_at IS NULL
           WHERE w.id = ANY($1) AND w.workspace_id = $2 AND w.deleted_at IS NULL"#,
        ids,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// Live warehouses among `ids`, plus the workspace's first live store (Laravel's fallback
/// stock warehouses for products). One query.
pub async fn stock_refs(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<(Vec<StockRef>, Option<Uuid>), sqlx::Error> {
    let rows = sqlx::query!(
        r#"WITH store AS (
               SELECT id FROM warehouses
               WHERE workspace_id = $1 AND is_store AND deleted_at IS NULL
               ORDER BY id LIMIT 1
           )
           SELECT w.id, w.name, w.alias, w.is_store, w.pickup_enabled, w.location,
                  COALESCE(w.id = (SELECT id FROM store), false) AS "is_first_store!"
           FROM warehouses w
           WHERE w.workspace_id = $1 AND w.deleted_at IS NULL
             AND (w.id = ANY($2) OR w.id = (SELECT id FROM store))"#,
        workspace_id,
        ids,
    )
    .fetch_all(db)
    .await?;
    let store = rows.iter().find(|r| r.is_first_store).map(|r| r.id);
    let refs = rows
        .into_iter()
        .map(|r| StockRef {
            id: r.id,
            name: r.name,
            alias: r.alias,
            is_store: r.is_store,
            pickup_enabled: r.pickup_enabled,
            location: r.location,
        })
        .collect();
    Ok((refs, store))
}

/// Live children of any of `parent_ids`, oldest first. One query for a whole page.
pub async fn children_of(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    parent_ids: &[Uuid],
) -> Result<Vec<ChildRow>, sqlx::Error> {
    sqlx::query_as!(
        ChildRow,
        r#"SELECT parent_id AS "parent_id!", id, name, slug, alias, is_store, pickup_enabled,
                  status, type AS kind
           FROM warehouses
           WHERE workspace_id = $1 AND parent_id = ANY($2) AND deleted_at IS NULL
           ORDER BY id"#,
        workspace_id,
        parent_ids,
    )
    .fetch_all(db)
    .await
}

/// The live warehouse under `FOR UPDATE`, so stock writers (`FOR SHARE` in
/// `stock::repo::locked`) finish first and new ones wait; false = no such live row.
pub async fn lock_for_delete(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM warehouses WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
         FOR UPDATE",
        id,
        workspace_id,
    )
    .fetch_optional(conn)
    .await
    .map(|r| r.is_some())
}

/// Soft delete; children stay, as in Laravel. False = no such live row, or a live stock row here
/// still has a level or a hold (deleting would drop it).
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE warehouses SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM stocks s
                           WHERE s.workspace_id = $2 AND s.warehouse_id = $1
                             AND s.deleted_at IS NULL
                             AND (s.stock_lvl <> 0 OR s.reserved <> 0))",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
