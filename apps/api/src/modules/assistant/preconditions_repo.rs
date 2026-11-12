//! Bounded proposal snapshots: requested stock slots, delete targets and impact counts; no writes.

use rust_decimal::Decimal;
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
          'bin_name',coalesce(nullif(btrim(b.bin_name),''),b.bin_code),
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

/// A lot's level at one live warehouse of the workspace (0 before its first movement).
pub(super) async fn lot_level(
    conn: &mut PgConnection,
    workspace: Uuid,
    lot: Uuid,
    warehouse: Uuid,
) -> Result<Option<(String, String, Decimal)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT w.name AS "warehouse!", l.code AS "lot!",
                  coalesce((SELECT s.stock_lvl FROM stocks s WHERE s.workspace_id=$1 AND s.lot_id=l.id
                     AND s.warehouse_id=w.id AND s.bin_id IS NULL AND s.deleted_at IS NULL),0) AS "level!"
           FROM lots l JOIN warehouses w ON w.workspace_id=l.workspace_id AND w.id=$3 AND w.deleted_at IS NULL
           WHERE l.workspace_id=$1 AND l.id=$2"#,
        workspace,
        lot,
        warehouse,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.warehouse, r.lot, r.level)))
}

/// A live payment's amount in minor units, only on that invoice of the workspace.
pub(crate) async fn payment_amount(
    conn: &mut PgConnection,
    workspace: Uuid,
    invoice: Uuid,
    payment: Uuid,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT amount FROM invoice_payments WHERE workspace_id=$1 AND invoice_id=$2 AND id=$3 AND voided_at IS NULL",
        workspace,
        invoice,
        payment,
    )
    .fetch_optional(conn)
    .await
}

/// Live relation's display name (company, alias, else first contact) and what its delete hides.
pub(super) async fn relation_impact(
    conn: &mut PgConnection,
    workspace: Uuid,
    id: Uuid,
) -> Result<Option<(Value, Value)>, sqlx::Error> {
    let row = sqlx::query!(r#"SELECT jsonb_build_object('id',r.id,'label',coalesce(
            nullif(btrim(r.company_name),''),nullif(btrim(r.alias),''),
            (SELECT nullif(btrim(concat_ws(' ',coalesce(c.first_name,c.salutation),c.middle_name,c.last_name)),'')
             FROM contacts c WHERE c.workspace_id=$1 AND c.relation_id=r.id AND c.deleted_at IS NULL ORDER BY c.id LIMIT 1))) AS "locked!",
          jsonb_build_object('effect','archive',
            'orders',(SELECT count(*) FROM orders o WHERE o.workspace_id=$1 AND o.relation_id=r.id),
            'contacts',(SELECT count(*) FROM contacts c WHERE c.workspace_id=$1 AND c.relation_id=r.id AND c.deleted_at IS NULL)) AS "impact!"
        FROM relations r WHERE r.workspace_id=$1 AND r.id=$2 AND r.deleted_at IS NULL"#,
        workspace, id).fetch_optional(conn).await?;
    Ok(row.map(|r| (r.locked, r.impact)))
}

/// Live product's name and its sizes and non-zero stock rows hidden by delete.
pub(super) async fn product_impact(
    conn: &mut PgConnection,
    workspace: Uuid,
    id: Uuid,
) -> Result<Option<(Value, Value)>, sqlx::Error> {
    let row = sqlx::query!(r#"SELECT jsonb_build_object('id',p.id,'label',p.name) AS "locked!",
          jsonb_build_object('effect','archive',
            'variants',(SELECT count(*) FROM product_variants v WHERE v.workspace_id=$1 AND v.product_id=p.id AND v.deleted_at IS NULL),
            'stock_rows',(SELECT count(*) FROM stocks s WHERE s.workspace_id=$1 AND s.product_id=p.id AND s.deleted_at IS NULL AND s.stock_lvl<>0)) AS "impact!"
        FROM products p WHERE p.workspace_id=$1 AND p.id=$2 AND p.deleted_at IS NULL"#,
        workspace, id).fetch_optional(conn).await?;
    Ok(row.map(|r| (r.locked, r.impact)))
}
