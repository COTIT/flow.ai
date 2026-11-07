//! Bounded snapshots of explicitly requested stock slots; no locks or writes at proposal.

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Requested slots in request order, with zero values for rows not yet created.
pub(super) async fn stock_slots(
    conn: &mut PgConnection,
    workspace: Uuid,
    product: Uuid,
    warehouses: &[Uuid],
    bins: &[Option<Uuid>],
) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT coalesce(jsonb_agg(jsonb_build_object(
          'warehouse_id',wanted.warehouse_id,'warehouse_name',w.name,'bin_id',wanted.bin_id,
          'valid',w.id IS NOT NULL AND (wanted.bin_id IS NULL OR b.id IS NOT NULL),
          'before',trim_scale(coalesce(s.stock_lvl,0))::text,
          'reserved',trim_scale(coalesce(s.reserved,0))::text,
          'minimal',coalesce(s.minimal_stock_level,0),'maximum',coalesce(s.maximum_stock_level,0)
        ) ORDER BY wanted.position),'[]'::jsonb) AS "slots!"
        FROM unnest($3::uuid[],$4::uuid[]) WITH ORDINALITY AS wanted(warehouse_id,bin_id,position)
        LEFT JOIN warehouses w ON w.workspace_id=$1 AND w.id=wanted.warehouse_id AND w.deleted_at IS NULL
        LEFT JOIN warehouse_section_row_bins b ON b.workspace_id=$1 AND b.warehouse_id=wanted.warehouse_id AND b.id=wanted.bin_id AND b.deleted_at IS NULL
        LEFT JOIN stocks s ON s.workspace_id=$1 AND s.product_id=$2 AND s.warehouse_id=wanted.warehouse_id
          AND s.bin_id IS NOT DISTINCT FROM wanted.bin_id AND s.lot_id IS NULL AND s.deleted_at IS NULL"#,
        workspace,product,warehouses,bins as &[Option<Uuid>]).fetch_one(conn).await
}
