//! SQL for orders and their lines. Writes run in the service's transaction; lines are written
//! in one statement (UNNEST) whatever the count.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A catalog product as an order line prices it.
pub struct CatalogProduct {
    pub id: Uuid,
    pub name: String,
    pub price: Decimal,
    pub currency_id: Uuid,
    pub has_stock: bool,
    /// Sellable on the web shop: active, not archived.
    pub sellable: bool,
    pub vat: Decimal,
}

/// A live variant.
pub struct CatalogVariant {
    pub id: Uuid,
    pub product_id: Uuid,
    pub sku: Option<String>,
    pub price: Decimal,
}

/// Live products of `ids` with their VAT rate (0 without a VAT group).
pub async fn products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<CatalogProduct>, sqlx::Error> {
    sqlx::query_as!(
        CatalogProduct,
        r#"SELECT p.id, p.name, p.price, p.currency_id, p.has_stock,
                  (p.status = 'active' AND NOT p.archived) AS "sellable!",
                  coalesce(v.vat_amount, 0) AS "vat!"
           FROM products p
           LEFT JOIN vat_groups v ON v.workspace_id = p.workspace_id AND v.id = p.vat_group_id
                AND v.deleted_at IS NULL
           WHERE p.workspace_id = $1 AND p.id = ANY($2) AND p.deleted_at IS NULL"#,
        workspace_id,
        ids,
    )
    .fetch_all(db)
    .await
}

/// Live variants of `ids`.
pub async fn variants(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<CatalogVariant>, sqlx::Error> {
    sqlx::query_as!(
        CatalogVariant,
        "SELECT id, product_id, sku, price FROM product_variants
         WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        workspace_id,
        ids,
    )
    .fetch_all(db)
    .await
}

/// The main (default) live warehouse.
pub async fn default_warehouse(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM warehouses
         WHERE workspace_id = $1 AND default_warehouse AND deleted_at IS NULL
         ORDER BY id LIMIT 1",
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// A live warehouse of the workspace.
pub async fn warehouse_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM warehouses
                          WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL) AS "e!""#,
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}

/// A header to insert.
pub struct NewOrder<'a> {
    pub number: &'a str,
    pub status: &'a str,
    pub source: &'a str,
    pub relation_id: Option<Uuid>,
    pub currency_id: Uuid,
    pub warehouse_id: Uuid,
    pub customer: Option<&'a Value>,
    pub reference: Option<&'a str>,
    pub note: Option<&'a str>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub app_key_id: Option<Uuid>,
}

/// Insert a header; its id.
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    o: &NewOrder<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO orders (workspace_id, number, status, source, relation_id, currency_id,
                             warehouse_id, customer, reference, note, expires_at, created_by,
                             app_key_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         RETURNING id",
        workspace_id,
        o.number,
        o.status,
        o.source,
        o.relation_id,
        o.currency_id,
        o.warehouse_id,
        o.customer,
        o.reference,
        o.note,
        o.expires_at,
        o.created_by,
        o.app_key_id,
    )
    .fetch_one(conn)
    .await
}

/// A priced line to write.
pub struct NewLine {
    pub product_id: Uuid,
    pub product_variant_id: Option<Uuid>,
    pub title: String,
    pub quantity: Decimal,
    pub unit_price: i64,
    pub vat_rate: Decimal,
    pub discount: Decimal,
    pub line_excl: i64,
    pub line_vat: i64,
    pub tracked: bool,
}

/// Replace the order's lines with `lines` (positions 1..) and store its totals.
pub async fn write_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    order_id: Uuid,
    lines: &[NewLine],
    totals: (i64, i64, i64),
    summary: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM order_lines WHERE workspace_id = $1 AND order_id = $2",
        workspace_id,
        order_id,
    )
    .execute(&mut *conn)
    .await?;
    let col = |f: fn(&NewLine) -> Uuid| lines.iter().map(f).collect::<Vec<_>>();
    let positions: Vec<i32> = (1..=i32::try_from(lines.len()).unwrap_or(i32::MAX)).collect();
    sqlx::query!(
        "INSERT INTO order_lines (workspace_id, order_id, position, product_id,
                                  product_variant_id, title, quantity, unit_price, vat_rate,
                                  discount_percentage, line_excl, line_vat, tracked)
         SELECT $1, $2, * FROM UNNEST($3::int[], $4::uuid[], $5::uuid[], $6::text[],
                                      $7::numeric[], $8::bigint[], $9::numeric[], $10::numeric[],
                                      $11::bigint[], $12::bigint[], $13::bool[])",
        workspace_id,
        order_id,
        &positions,
        &col(|l| l.product_id),
        &lines
            .iter()
            .map(|l| l.product_variant_id)
            .collect::<Vec<_>>() as &[Option<Uuid>],
        &lines.iter().map(|l| l.title.clone()).collect::<Vec<_>>(),
        &lines.iter().map(|l| l.quantity).collect::<Vec<_>>(),
        &lines.iter().map(|l| l.unit_price).collect::<Vec<_>>(),
        &lines.iter().map(|l| l.vat_rate).collect::<Vec<_>>(),
        &lines.iter().map(|l| l.discount).collect::<Vec<_>>(),
        &lines.iter().map(|l| l.line_excl).collect::<Vec<_>>(),
        &lines.iter().map(|l| l.line_vat).collect::<Vec<_>>(),
        &lines.iter().map(|l| l.tracked).collect::<Vec<_>>(),
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE orders SET subtotal = $3, vat = $4, total = $5, summary = $6, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        order_id,
        totals.0,
        totals.1,
        totals.2,
        summary,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// An order header.
pub struct OrderRow {
    pub id: Uuid,
    pub number: String,
    pub status: String,
    pub source: String,
    pub relation_id: Option<Uuid>,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub warehouse_id: Uuid,
    pub customer: Option<Value>,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub subtotal: i64,
    pub vat: i64,
    pub total: i64,
    pub summary: Value,
    pub expires_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub invoice_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The header under a row lock (first in every write's lock order).
pub async fn lock(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<OrderRow>, sqlx::Error> {
    sqlx::query_as!(
        OrderRow,
        r#"SELECT o.id, o.number, o.status, o.source, o.relation_id, o.currency_id,
                  upper(c.asset_type) AS "currency!", c.decimals, o.warehouse_id, o.customer,
                  o.reference, o.note, o.subtotal, o.vat, o.total, o.summary, o.expires_at,
                  o.paid_at, o.completed_at, o.cancelled_at, o.invoice_id, o.created_at,
                  o.updated_at
           FROM orders o JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
           WHERE o.workspace_id = $1 AND o.id = $2
           FOR UPDATE OF o"#,
        workspace_id,
        id,
    )
    .fetch_optional(conn)
    .await
}

/// One header.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<OrderRow>, sqlx::Error> {
    sqlx::query_as!(
        OrderRow,
        r#"SELECT o.id, o.number, o.status, o.source, o.relation_id, o.currency_id,
                  upper(c.asset_type) AS "currency!", c.decimals, o.warehouse_id, o.customer,
                  o.reference, o.note, o.subtotal, o.vat, o.total, o.summary, o.expires_at,
                  o.paid_at, o.completed_at, o.cancelled_at, o.invoice_id, o.created_at,
                  o.updated_at
           FROM orders o JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
           WHERE o.workspace_id = $1 AND o.id = $2"#,
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// A page of headers, newest first; `total` counts every match.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<OrderRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT o.id, o.number, o.status, o.source, o.relation_id, o.currency_id,
                  upper(c.asset_type) AS "currency!", c.decimals, o.warehouse_id, o.customer,
                  o.reference, o.note, o.subtotal, o.vat, o.total, o.summary, o.expires_at,
                  o.paid_at, o.completed_at, o.cancelled_at, o.invoice_id, o.created_at,
                  o.updated_at, count(*) OVER () AS "total_rows!"
           FROM orders o JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
           WHERE o.workspace_id = $1 AND ($2::text IS NULL OR o.status = $2)
           ORDER BY o.created_at DESC, o.id DESC
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        status,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total_rows);
    let rows = rows
        .into_iter()
        .map(|r| OrderRow {
            id: r.id,
            number: r.number,
            status: r.status,
            source: r.source,
            relation_id: r.relation_id,
            currency_id: r.currency_id,
            currency: r.currency,
            decimals: r.decimals,
            warehouse_id: r.warehouse_id,
            customer: r.customer,
            reference: r.reference,
            note: r.note,
            subtotal: r.subtotal,
            vat: r.vat,
            total: r.total,
            summary: r.summary,
            expires_at: r.expires_at,
            paid_at: r.paid_at,
            completed_at: r.completed_at,
            cancelled_at: r.cancelled_at,
            invoice_id: r.invoice_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Count of orders matching (page past the end).
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    status: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM orders
           WHERE workspace_id = $1 AND ($2::text IS NULL OR status = $2)"#,
        workspace_id,
        status,
    )
    .fetch_one(db)
    .await
}

/// An order line.
pub struct LineRow {
    pub id: Uuid,
    pub order_id: Uuid,
    pub product_id: Uuid,
    pub product_variant_id: Option<Uuid>,
    pub title: String,
    pub quantity: Decimal,
    pub unit_price: i64,
    pub vat_rate: Decimal,
    pub discount_percentage: Decimal,
    pub line_excl: i64,
    pub line_vat: i64,
    pub tracked: bool,
    pub picked: Decimal,
    pub deducted: Decimal,
    pub returned: Decimal,
}

/// Lines of `order_ids` in display order.
pub async fn lines(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    order_ids: &[Uuid],
) -> Result<Vec<LineRow>, sqlx::Error> {
    sqlx::query_as!(
        LineRow,
        "SELECT id, order_id, product_id, product_variant_id, title, quantity, unit_price,
                vat_rate, discount_percentage, line_excl, line_vat, tracked, picked, deducted,
                returned
         FROM order_lines WHERE workspace_id = $1 AND order_id = ANY($2)
         ORDER BY order_id, position, id",
        workspace_id,
        order_ids,
    )
    .fetch_all(db)
    .await
}

/// Move an order to `status` and stamp the matching time.
pub async fn set_status(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE orders SET status = $3, updated_at = now(),
             expires_at = CASE WHEN $3 = 'awaiting_payment' THEN expires_at END,
             completed_at = CASE WHEN $3 = 'completed' THEN now() ELSE completed_at END,
             cancelled_at = CASE WHEN $3 = 'cancelled' THEN now()
                                 WHEN $3 = 'open' THEN NULL ELSE cancelled_at END
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        status,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// Set lines' picked/deducted/returned to the given absolute values.
pub async fn set_line_quantities(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    rows: &[(Uuid, Decimal, Decimal, Decimal)],
) -> Result<(), sqlx::Error> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.0).collect();
    let picked: Vec<Decimal> = rows.iter().map(|r| r.1).collect();
    let deducted: Vec<Decimal> = rows.iter().map(|r| r.2).collect();
    let returned: Vec<Decimal> = rows.iter().map(|r| r.3).collect();
    sqlx::query!(
        "UPDATE order_lines l SET picked = u.p, deducted = u.d, returned = u.r, updated_at = now()
         FROM UNNEST($2::uuid[], $3::numeric[], $4::numeric[], $5::numeric[]) AS u(id, p, d, r)
         WHERE l.workspace_id = $1 AND l.id = u.id",
        workspace_id,
        &ids,
        &picked,
        &deducted,
        &returned,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// Header fields any status may change.
pub async fn update_header(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    relation_id: Option<Uuid>,
    warehouse_id: Uuid,
    reference: Option<&str>,
    note: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE orders SET relation_id = $3, warehouse_id = $4, reference = $5, note = $6,
                           updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        relation_id,
        warehouse_id,
        reference,
        note,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// Delete a draft (its lines cascade).
pub async fn delete(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM orders WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// Link the order to the invoice made from it.
pub async fn set_invoice(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    invoice_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE orders SET invoice_id = $3, updated_at = now() WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        invoice_id,
    )
    .execute(conn)
    .await
    .map(|_| ())
}
