//! SQL for storage_locations. Every statement takes `workspace_id`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A storage location with its warehouse location and that location's warehouse (compact).
pub struct PlaceRow {
    pub id: Uuid,
    pub description: Option<String>,
    pub meta: Option<Value>,
    pub l_id: Option<Uuid>,
    pub l_location: Option<String>,
    pub l_meta: Option<Value>,
    pub l_created: Option<DateTime<Utc>>,
    pub l_updated: Option<DateTime<Utc>>,
    pub w_id: Option<Uuid>,
    pub w_name: Option<String>,
    pub w_slug: Option<String>,
    pub w_alias: Option<String>,
    pub w_is_store: Option<bool>,
    pub w_pickup: Option<bool>,
    pub w_parent_name: Option<String>,
    pub w_status: Option<String>,
    pub w_type: Option<String>,
}

/// Live storage locations of a product, oldest first.
pub async fn of_product(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<Vec<PlaceRow>, sqlx::Error> {
    sqlx::query_as!(
        PlaceRow,
        r#"SELECT sl.id, sl.description, sl.meta,
                  l.id AS "l_id?", l.location AS "l_location?", l.meta AS "l_meta?",
                  l.created_at AS "l_created?", l.updated_at AS "l_updated?",
                  w.id AS "w_id?", w.name AS "w_name?", w.slug AS "w_slug?",
                  w.alias AS "w_alias?", w.is_store AS "w_is_store?",
                  w.pickup_enabled AS "w_pickup?", pw.name AS "w_parent_name?",
                  w.status AS "w_status?", w.type AS "w_type?"
           FROM storage_locations sl
           LEFT JOIN warehouse_locations l ON l.id = sl.warehouse_location_id
                                          AND l.deleted_at IS NULL
           LEFT JOIN warehouses w ON w.id = l.warehouse_id AND w.deleted_at IS NULL
           LEFT JOIN warehouses pw ON pw.id = w.parent_id AND pw.deleted_at IS NULL
           WHERE sl.workspace_id = $1 AND sl.parent_domain = 'product' AND sl.parent_id = $2
             AND sl.deleted_at IS NULL
           ORDER BY sl.id"#,
        workspace_id,
        product_id,
    )
    .fetch_all(db)
    .await
}

/// True when `id` is a live warehouse location of the workspace.
pub async fn location_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM warehouse_locations
                          WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
        id,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// A live bin's warehouse and its `section.row.shelf.bin` code path (Laravel
/// resolveWarehouseLocationFromBin; empty and "0" codes are dropped as PHP array_filter did).
pub async fn bin_path(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    bin_id: Uuid,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT b.warehouse_id,
                  array_to_string(array_remove(array_remove(
                      ARRAY[s.section_code, r.row_code, sh.shelf_code, b.bin_code], ''), '0'), '.')
                  AS "path!"
           FROM warehouse_section_row_bins b
           JOIN warehouse_section_row_shelves sh ON sh.id = b.shelf_id AND sh.deleted_at IS NULL
           JOIN warehouse_section_rows r ON r.id = b.row_id AND r.deleted_at IS NULL
           JOIN warehouse_sections s ON s.id = b.section_id AND s.deleted_at IS NULL
           WHERE b.id = $1 AND b.workspace_id = $2 AND b.deleted_at IS NULL"#,
        bin_id,
        workspace_id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.warehouse_id, r.path)))
}

/// Laravel WarehouseLocation::firstOrCreate by (warehouse, location text), under a lock so two
/// parallel calls make one row. Returns its id.
pub async fn location_for(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    location: &str,
    meta: &Value,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext('warehouse_locations'), hashtext($1::text || $2))",
        warehouse_id.to_string(),
        location,
    )
    .execute(&mut *conn)
    .await?;
    let found = sqlx::query_scalar!(
        "SELECT id FROM warehouse_locations
         WHERE workspace_id = $1 AND warehouse_id = $2 AND location = $3 AND deleted_at IS NULL
         ORDER BY id LIMIT 1",
        workspace_id,
        warehouse_id,
        location,
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(id) = found {
        return Ok(id);
    }
    sqlx::query_scalar!(
        "INSERT INTO warehouse_locations (workspace_id, warehouse_id, location, meta)
         VALUES ($1, $2, $3, $4) RETURNING id",
        workspace_id,
        warehouse_id,
        location,
        meta,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Soft-delete every live storage location of the product (`replace`).
pub async fn clear_product(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE storage_locations SET deleted_at = now(), updated_at = now()
         WHERE workspace_id = $1 AND parent_domain = 'product' AND parent_id = $2
           AND deleted_at IS NULL",
        workspace_id,
        product_id,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Insert; returns the new id.
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    location_id: Uuid,
    description: Option<&str>,
    meta: Option<&Value>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO storage_locations (workspace_id, parent_domain, parent_id,
                                        warehouse_location_id, description, meta)
         VALUES ($1, 'product', $2, $3, $4, $5) RETURNING id",
        workspace_id,
        product_id,
        location_id,
        description,
        meta,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Soft delete; false = unknown.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE storage_locations SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
