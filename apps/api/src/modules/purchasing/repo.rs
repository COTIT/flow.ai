//! SQL for purchase orders, receipts, landed costs and the pending-receipt guard. Every
//! query filters by workspace; writers run in the caller's transaction.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A rate snapshot: (rate, inverse, source, rate date).
pub type Rate = (Decimal, bool, &'static str, Option<NaiveDate>);

/// Whether an active book holds accepted-unbooked receipts in `warehouse` (for `product`, or
/// any product when None). Stock-layer guard, sale availability (SQL function of the same
/// name) and stocktake start (M6) share it.
pub async fn pending(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    warehouse: Uuid,
    product: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT purchase_receipt_pending($1, $2, $3) AS "pending!""#,
        ws,
        warehouse,
        product
    )
    .fetch_one(db)
    .await
}

/// Whether an unbooked, non-late line of `line`'s pool comes before it in eligible booking
/// order (effective date, record time, id).
pub async fn pending_before(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    line: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM purchase_receipt_lines o, purchase_receipt_lines t
               WHERE t.workspace_id = $1 AND t.id = $2
                 AND o.workspace_id = t.workspace_id AND o.warehouse_id = t.warehouse_id
                 AND o.product_id = t.product_id AND o.booked_at IS NULL AND NOT o.late
                 AND (o.effective_date, o.recorded_at, o.id)
                     < (t.effective_date, t.recorded_at, t.id)) AS "before!""#,
        ws,
        line
    )
    .fetch_one(db)
    .await
}

/// Like [`pending_before`], ignoring lines in `selected` (booked earlier in the same request).
pub async fn pending_before_outside(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    line: Uuid,
    selected: &[Uuid],
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM purchase_receipt_lines o, purchase_receipt_lines t
               WHERE t.workspace_id = $1 AND t.id = $2
                 AND o.workspace_id = t.workspace_id AND o.warehouse_id = t.warehouse_id
                 AND o.product_id = t.product_id AND o.booked_at IS NULL AND NOT o.late
                 AND o.id <> ALL($3)
                 AND (o.effective_date, o.recorded_at, o.id)
                     < (t.effective_date, t.recorded_at, t.id)) AS "before!""#,
        ws,
        line,
        selected
    )
    .fetch_one(db)
    .await
}

/// The book's local business date: accounting timezone; before one is set, the workspace's
/// `general.timezone` (when Postgres knows it); else UTC.
pub async fn today(db: impl PgExecutor<'_>, ws: Uuid) -> Result<NaiveDate, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT (now() AT TIME ZONE coalesce(
                      a.timezone,
                      (SELECT z.name FROM pg_timezone_names z
                       WHERE z.name = (SELECT s.data -> 'general' ->> 'timezone'
                                       FROM settings s WHERE s.workspace_id = a.workspace_id)),
                      'UTC'))::date AS "d!"
           FROM accounting_settings a WHERE a.workspace_id = $1"#,
        ws
    )
    .fetch_one(db)
    .await
}

// --- orders --------------------------------------------------------------------------------

/// A purchase order row with its supplier name.
pub struct OrderRow {
    pub id: Uuid,
    pub number: i64,
    pub state: String,
    pub supplier_id: Uuid,
    pub supplier_name: Option<String>,
    pub supplier_snapshot: Option<Value>,
    pub currency_id: Uuid,
    pub currency_code: String,
    pub currency_decimals: i16,
    pub purchase_date: NaiveDate,
    pub expected_date: Option<NaiveDate>,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub default_warehouse_id: Option<Uuid>,
    pub version: i64,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub confirmed_by: Option<Uuid>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    /// Σ extension, and the extension share of received and open quantities (SQL rounding).
    pub ordered_total: i64,
    pub received_total: i64,
    pub open_total: i64,
}

/// Orders by id (`ids` None = a filtered page, newest first) plus the filtered total.
pub async fn orders(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    state: Option<&str>,
    supplier: Option<Uuid>,
    (limit, offset): (i64, i64),
) -> Result<(Vec<OrderRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT o.id, o.number, o.state, o.supplier_id,
                  coalesce(r.company_name, r.alias) AS supplier_name,
                  o.supplier_snapshot, o.currency_id, upper(c.asset_type) AS "currency_code!",
                  c.decimals AS currency_decimals, o.purchase_date, o.expected_date,
                  o.reference, o.note, o.default_warehouse_id, o.version, o.created_by,
                  o.created_at, o.updated_at, o.confirmed_by, o.confirmed_at, o.cancelled_at,
                  o.cancel_reason, count(*) OVER () AS "total!",
                  t.ordered AS "ordered!", t.received AS "received!", t.open AS "open!"
           FROM purchase_orders o
           JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
           JOIN relations r ON r.workspace_id = o.workspace_id AND r.id = o.supplier_id
           CROSS JOIN LATERAL (
               SELECT coalesce(sum(l.extension), 0)::bigint AS ordered,
                      coalesce(sum(money_round_div(l.extension * l.received_quantity,
                                                   l.base_quantity)), 0)::bigint AS received,
                      coalesce(sum(money_round_div(l.extension * (l.base_quantity
                                       - l.received_quantity - l.closed_quantity),
                                   l.base_quantity)), 0)::bigint AS open
               FROM purchase_order_lines l WHERE l.purchase_order_id = o.id) t
           WHERE o.workspace_id = $1 AND ($2::uuid IS NULL OR o.id = $2)
             AND ($3::text IS NULL OR o.state = $3)
             AND ($4::uuid IS NULL OR o.supplier_id = $4)
           ORDER BY o.number DESC
           LIMIT $5 OFFSET $6"#,
        ws,
        id,
        state,
        supplier,
        limit,
        offset
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| OrderRow {
            id: r.id,
            number: r.number,
            state: r.state,
            supplier_id: r.supplier_id,
            supplier_name: r.supplier_name,
            supplier_snapshot: r.supplier_snapshot,
            currency_id: r.currency_id,
            currency_code: r.currency_code,
            currency_decimals: r.currency_decimals,
            purchase_date: r.purchase_date,
            expected_date: r.expected_date,
            reference: r.reference,
            note: r.note,
            default_warehouse_id: r.default_warehouse_id,
            version: r.version,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            confirmed_by: r.confirmed_by,
            confirmed_at: r.confirmed_at,
            cancelled_at: r.cancelled_at,
            cancel_reason: r.cancel_reason,
            ordered_total: r.ordered,
            received_total: r.received,
            open_total: r.open,
        })
        .collect();
    Ok((rows, total))
}

/// An order line with product and unit names.
pub struct LineRow {
    pub id: Uuid,
    pub purchase_order_id: Uuid,
    pub line_no: i32,
    pub product_id: Uuid,
    pub product_name: String,
    pub product_unit: Option<String>,
    pub purchase_unit_id: Option<Uuid>,
    pub unit_code: Option<String>,
    pub input_quantity: Decimal,
    pub ratio: Decimal,
    pub base_quantity: Decimal,
    pub unit_price: Decimal,
    pub extension: i64,
    pub warehouse_id: Option<Uuid>,
    pub received_quantity: Decimal,
    pub booked_quantity: Decimal,
    pub closed_quantity: Decimal,
}

/// Lines of `orders`, by order then line number. `lock` takes them FOR UPDATE (id order).
pub async fn lines(
    conn: &mut PgConnection,
    ws: Uuid,
    orders: &[Uuid],
    lock: bool,
) -> Result<Vec<LineRow>, sqlx::Error> {
    if lock {
        sqlx::query!(
            "SELECT id FROM purchase_order_lines
             WHERE workspace_id = $1 AND purchase_order_id = ANY($2) ORDER BY id FOR UPDATE",
            ws,
            orders
        )
        .fetch_all(&mut *conn)
        .await?;
    }
    sqlx::query_as!(
        LineRow,
        r#"SELECT l.id, l.purchase_order_id, l.line_no, l.product_id, p.name AS product_name,
                  p.unit AS product_unit, l.purchase_unit_id, u.code AS "unit_code?",
                  l.input_quantity, l.ratio, l.base_quantity, l.unit_price, l.extension,
                  l.warehouse_id, l.received_quantity, l.booked_quantity, l.closed_quantity
           FROM purchase_order_lines l
           JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
           LEFT JOIN purchase_units u ON u.workspace_id = l.workspace_id
                                     AND u.id = l.purchase_unit_id
           WHERE l.workspace_id = $1 AND l.purchase_order_id = ANY($2)
           ORDER BY l.purchase_order_id, l.line_no"#,
        ws,
        orders
    )
    .fetch_all(conn)
    .await
}

/// The order FOR UPDATE: (state, version, currency).
pub async fn lock_order(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(String, i64, Uuid)>, sqlx::Error> {
    sqlx::query!(
        "SELECT state, version, currency_id FROM purchase_orders
         WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        ws,
        id
    )
    .fetch_optional(conn)
    .await
    .map(|r| r.map(|r| (r.state, r.version, r.currency_id)))
}

/// Order header fields as stored.
pub struct Header<'a> {
    pub supplier: Uuid,
    pub currency: Uuid,
    pub purchase_date: NaiveDate,
    pub expected_date: Option<NaiveDate>,
    pub reference: Option<&'a str>,
    pub note: Option<&'a str>,
    pub default_warehouse: Option<Uuid>,
}

/// New draft order; its id.
pub async fn insert_order(
    conn: &mut PgConnection,
    ws: Uuid,
    number: i64,
    h: &Header<'_>,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO purchase_orders (workspace_id, number, supplier_id, currency_id,
             purchase_date, expected_date, reference, note, default_warehouse_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
        ws,
        number,
        h.supplier,
        h.currency,
        h.purchase_date,
        h.expected_date,
        h.reference,
        h.note,
        h.default_warehouse,
        by
    )
    .fetch_one(conn)
    .await
}

/// Replace a draft's header; bumps the version.
pub async fn update_order(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    h: &Header<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE purchase_orders SET supplier_id = $3, currency_id = $4, purchase_date = $5,
             expected_date = $6, reference = $7, note = $8, default_warehouse_id = $9,
             version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        h.supplier,
        h.currency,
        h.purchase_date,
        h.expected_date,
        h.reference,
        h.note,
        h.default_warehouse
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A line to store.
pub struct NewLine {
    pub product: Uuid,
    pub unit: Option<Uuid>,
    pub input_quantity: Decimal,
    pub ratio: Decimal,
    pub base_quantity: Decimal,
    pub unit_price: Decimal,
    pub warehouse: Option<Uuid>,
}

/// Replace every line of a draft (numbered from 1 in input order). The extension
/// (quantity × price at the order currency's decimals) is computed here, in SQL; a total past
/// bigint fails with `numeric_value_out_of_range`.
pub async fn replace_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    lines: &[NewLine],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM purchase_order_lines WHERE workspace_id = $1 AND purchase_order_id = $2",
        ws,
        order
    )
    .execute(&mut *conn)
    .await?;
    let col = |f: fn(&NewLine) -> Decimal| lines.iter().map(f).collect::<Vec<_>>();
    sqlx::query!(
        "INSERT INTO purchase_order_lines (workspace_id, purchase_order_id, line_no, product_id,
             purchase_unit_id, input_quantity, ratio, base_quantity, unit_price, extension,
             warehouse_id)
         SELECT $1, $2, n::int, x.product, x.unit, x.q, x.ratio, x.base, x.price,
                money_round_div(x.q * x.price * 10::numeric ^ c.decimals, 1)::bigint, x.wh
         FROM unnest($3::uuid[], $4::uuid[], $5::numeric[], $6::numeric[], $7::numeric[],
                     $8::numeric[], $9::uuid[])
              WITH ORDINALITY AS x(product, unit, q, ratio, base, price, wh, n)
         JOIN purchase_orders o ON o.workspace_id = $1 AND o.id = $2
         JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id",
        ws,
        order,
        &lines.iter().map(|l| l.product).collect::<Vec<_>>(),
        &lines.iter().map(|l| l.unit).collect::<Vec<_>>() as &[Option<Uuid>],
        &col(|l| l.input_quantity),
        &col(|l| l.ratio),
        &col(|l| l.base_quantity),
        &col(|l| l.unit_price),
        &lines.iter().map(|l| l.warehouse).collect::<Vec<_>>() as &[Option<Uuid>],
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Product facts a line needs: (id, has_stock, type, live).
pub async fn products(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, bool, String)>, sqlx::Error> {
    sqlx::query!(
        "SELECT id, has_stock, type FROM products
         WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        ws,
        ids
    )
    .fetch_all(db)
    .await
    .map(|rs| {
        rs.into_iter()
            .map(|r| (r.id, r.has_stock, r.r#type))
            .collect()
    })
}

/// Purchase units among `ids`: (id, product, ratio, active).
pub async fn units(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, Uuid, Decimal, bool)>, sqlx::Error> {
    sqlx::query!(
        "SELECT id, product_id, base_units_per_input_unit, active FROM purchase_units
         WHERE workspace_id = $1 AND id = ANY($2)",
        ws,
        ids
    )
    .fetch_all(db)
    .await
    .map(|rs| {
        rs.into_iter()
            .map(|r| (r.id, r.product_id, r.base_units_per_input_unit, r.active))
            .collect()
    })
}

/// Whether `id` is a live relation of the workspace.
pub async fn supplier_exists(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM relations
                          WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL) AS "e!""#,
        ws,
        id
    )
    .fetch_one(db)
    .await
}

/// The supplier as confirm freezes it: (id, company_name, alias, addresses, supplier_data).
pub async fn supplier_facts(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(Option<String>, Option<String>, Value, Option<Value>)>, sqlx::Error> {
    sqlx::query!(
        "SELECT company_name, alias, addresses, supplier_data FROM relations
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id
    )
    .fetch_optional(db)
    .await
    .map(|r| r.map(|r| (r.company_name, r.alias, r.addresses, r.supplier_data)))
}

/// Confirm: freeze snapshot, bump version.
pub async fn confirm(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    snapshot: &Value,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE purchase_orders SET state = 'confirmed', supplier_snapshot = $3,
             confirmed_by = $4, confirmed_at = now(), version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        snapshot,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Close `quantity` of a line's open remainder with its history row.
pub async fn close_line(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    line: Uuid,
    quantity: Decimal,
    reason: &str,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE purchase_order_lines SET closed_quantity = closed_quantity + $3
         WHERE workspace_id = $1 AND id = $2",
        ws,
        line,
        quantity
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "INSERT INTO purchase_order_closures (workspace_id, purchase_order_id, line_id, quantity,
             reason, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)",
        ws,
        order,
        line,
        quantity,
        reason,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Bump the order version (a child change: receipt, booking, closure).
pub async fn touch(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE purchase_orders SET version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Cancel the order.
pub async fn cancel(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    reason: &str,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE purchase_orders SET state = 'cancelled', cancelled_by = $3, cancelled_at = now(),
             cancel_reason = $4, version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        by,
        reason
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// One history event.
pub struct EventRow {
    pub at: DateTime<Utc>,
    pub kind: String,
    pub actor: Uuid,
    pub ref_id: Uuid,
    pub detail: Option<String>,
}

/// An order's history from its rows, newest first (at most 500).
pub async fn events(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    order: Uuid,
) -> Result<Vec<EventRow>, sqlx::Error> {
    sqlx::query_as!(
        EventRow,
        r#"SELECT at AS "at!", kind AS "kind!", actor AS "actor!", ref_id AS "ref_id!", detail
           FROM (
             SELECT o.created_at AS at, 'created' AS kind, o.created_by AS actor, o.id AS ref_id,
                    NULL::text AS detail
             FROM purchase_orders o WHERE o.workspace_id = $1 AND o.id = $2
             UNION ALL
             SELECT o.confirmed_at, 'confirmed', o.confirmed_by, o.id, NULL
             FROM purchase_orders o
             WHERE o.workspace_id = $1 AND o.id = $2 AND o.confirmed_at IS NOT NULL
             UNION ALL
             SELECT o.cancelled_at, 'cancelled', o.cancelled_by, o.id, o.cancel_reason
             FROM purchase_orders o
             WHERE o.workspace_id = $1 AND o.id = $2 AND o.cancelled_at IS NOT NULL
             UNION ALL
             SELECT r.recorded_at, 'received', r.created_by, r.id, r.number::text
             FROM purchase_receipts r WHERE r.workspace_id = $1 AND r.purchase_order_id = $2
             UNION ALL
             SELECT max(l.booked_at), 'booked', (array_agg(l.booked_by))[1], r.id, r.number::text
             FROM purchase_receipts r
             JOIN purchase_receipt_lines l ON l.receipt_id = r.id AND l.booked_at IS NOT NULL
             WHERE r.workspace_id = $1 AND r.purchase_order_id = $2
             GROUP BY r.id, r.number, l.booked_at
             UNION ALL
             SELECT c.created_at, 'closed', c.created_by, c.line_id,
                    trim_scale(c.quantity)::text || ': ' || c.reason
             FROM purchase_order_closures c WHERE c.workspace_id = $1 AND c.purchase_order_id = $2
           ) e
           ORDER BY at DESC, kind LIMIT 500"#,
        ws,
        order
    )
    .fetch_all(db)
    .await
}

// --- receipts ------------------------------------------------------------------------------

/// A receipt row.
pub struct ReceiptRow {
    pub id: Uuid,
    pub purchase_order_id: Uuid,
    pub number: i64,
    pub effective_receipt_date: NaiveDate,
    pub note: Option<String>,
    pub version: i64,
    pub created_by: Uuid,
    pub recorded_at: DateTime<Utc>,
    /// The book's functional currency (id, code, decimals); None unless the book is active.
    pub functional: Option<(Uuid, String, i16)>,
}

/// Receipts of an order (newest first), or one when `id` is set; plus the total.
pub async fn receipts(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    order: Uuid,
    id: Option<Uuid>,
    (limit, offset): (i64, i64),
) -> Result<(Vec<ReceiptRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT r.id, r.purchase_order_id, r.number, r.effective_receipt_date, r.note,
                  r.version, r.created_by, r.recorded_at, count(*) OVER () AS "total!",
                  c.id AS "fc_id?", upper(c.asset_type) AS "fc_code?",
                  c.decimals AS "fc_decimals?"
           FROM purchase_receipts r
           JOIN accounting_settings a ON a.workspace_id = r.workspace_id
           LEFT JOIN currencies c ON a.state = 'active' AND c.workspace_id = a.workspace_id
                                 AND c.id = a.functional_currency_id
           WHERE r.workspace_id = $1 AND r.purchase_order_id = $2
             AND ($3::uuid IS NULL OR r.id = $3)
           ORDER BY r.number DESC LIMIT $4 OFFSET $5"#,
        ws,
        order,
        id,
        limit,
        offset
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    Ok((
        rows.into_iter()
            .map(|r| ReceiptRow {
                id: r.id,
                purchase_order_id: r.purchase_order_id,
                number: r.number,
                effective_receipt_date: r.effective_receipt_date,
                note: r.note,
                version: r.version,
                created_by: r.created_by,
                recorded_at: r.recorded_at,
                functional: match (r.fc_id, r.fc_code, r.fc_decimals) {
                    (Some(id), Some(code), Some(dp)) => Some((id, code, dp)),
                    _ => None,
                },
            })
            .collect(),
        total,
    ))
}

/// The receipt of `order` FOR UPDATE: (version, effective date).
pub async fn lock_receipt(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    id: Uuid,
) -> Result<Option<(i64, NaiveDate)>, sqlx::Error> {
    sqlx::query!(
        "SELECT version, effective_receipt_date FROM purchase_receipts
         WHERE workspace_id = $1 AND purchase_order_id = $2 AND id = $3 FOR UPDATE",
        ws,
        order,
        id
    )
    .fetch_optional(conn)
    .await
    .map(|r| r.map(|r| (r.version, r.effective_receipt_date)))
}

/// A receipt line.
pub struct ReceiptLineRow {
    pub id: Uuid,
    pub receipt_id: Uuid,
    pub po_line_id: Uuid,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub bin_id: Option<Uuid>,
    pub shelf_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    pub base_quantity: Decimal,
    pub late: bool,
    pub booked_at: Option<DateTime<Utc>>,
    pub accounting_date: Option<NaiveDate>,
    pub net_value: Option<i64>,
    pub landed_value: Option<i64>,
    pub rate: Option<Decimal>,
    pub rate_inverse: Option<bool>,
    pub rate_source: Option<String>,
    pub rate_date: Option<NaiveDate>,
    pub journal_entry_id: Option<Uuid>,
    /// net + landed, functional minor units.
    pub value: Option<i64>,
}

/// Lines of `receipts` (id order); `only` narrows to those ids and takes them FOR UPDATE.
pub async fn receipt_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    receipts: &[Uuid],
    only: Option<&[Uuid]>,
) -> Result<Vec<ReceiptLineRow>, sqlx::Error> {
    if let Some(ids) = only {
        sqlx::query!(
            "SELECT id FROM purchase_receipt_lines
             WHERE workspace_id = $1 AND receipt_id = ANY($2) AND id = ANY($3)
             ORDER BY id FOR UPDATE",
            ws,
            receipts,
            ids
        )
        .fetch_all(&mut *conn)
        .await?;
    }
    sqlx::query_as!(
        ReceiptLineRow,
        r#"SELECT l.id, l.receipt_id, l.po_line_id, l.product_id, l.warehouse_id, l.bin_id,
                  b.shelf_id AS "shelf_id?", l.lot_id, l.base_quantity, l.late, l.booked_at,
                  l.accounting_date, l.net_value, l.landed_value, l.rate, l.rate_inverse,
                  l.rate_source, l.rate_date, l.journal_entry_id,
                  l.net_value + l.landed_value AS value
           FROM purchase_receipt_lines l
           LEFT JOIN warehouse_section_row_bins b ON b.workspace_id = l.workspace_id
                                                 AND b.id = l.bin_id
           WHERE l.workspace_id = $1 AND l.receipt_id = ANY($2)
             AND ($3::uuid[] IS NULL OR l.id = ANY($3))
           ORDER BY l.id"#,
        ws,
        receipts,
        only as Option<&[Uuid]>
    )
    .fetch_all(conn)
    .await
}

/// Whether `lot` is a lot of `product` in the workspace.
pub async fn lot_of(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    lot: Uuid,
    product: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM lots
                          WHERE workspace_id = $1 AND id = $2 AND product_id = $3) AS "e!""#,
        ws,
        lot,
        product
    )
    .fetch_one(db)
    .await
}

/// Warehouse FOR SHARE (the stock layer's place lock), or None when gone.
pub async fn share_warehouse(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM warehouses WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL
         FOR SHARE",
        ws,
        id
    )
    .fetch_optional(conn)
    .await
}

/// New receipt; (id, recorded_at).
pub async fn insert_receipt(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    number: i64,
    date: NaiveDate,
    note: Option<&str>,
    by: Uuid,
) -> Result<(Uuid, DateTime<Utc>), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO purchase_receipts (workspace_id, purchase_order_id, number,
             effective_receipt_date, note, created_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id, recorded_at",
        ws,
        order,
        number,
        date,
        note,
        by
    )
    .fetch_one(conn)
    .await
    .map(|r| (r.id, r.recorded_at))
}

/// A received line to store.
pub struct NewReceiptLine {
    pub po_line: Uuid,
    pub product: Uuid,
    pub warehouse: Uuid,
    pub bin: Option<Uuid>,
    pub lot: Option<Uuid>,
    pub quantity: Decimal,
    pub late: bool,
}

/// Store a receipt's lines and add them to their order lines' received quantity.
pub async fn insert_receipt_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    receipt: Uuid,
    (date, recorded_at): (NaiveDate, DateTime<Utc>),
    lines: &[NewReceiptLine],
) -> Result<(), sqlx::Error> {
    let uuids = |f: fn(&NewReceiptLine) -> Uuid| lines.iter().map(f).collect::<Vec<_>>();
    let opt = |f: fn(&NewReceiptLine) -> Option<Uuid>| lines.iter().map(f).collect::<Vec<_>>();
    let qty: Vec<Decimal> = lines.iter().map(|l| l.quantity).collect();
    sqlx::query!(
        "INSERT INTO purchase_receipt_lines (workspace_id, receipt_id, po_line_id, product_id,
             warehouse_id, bin_id, lot_id, base_quantity, late, effective_date, recorded_at)
         SELECT $1, $2, x.po, x.product, x.wh, x.bin, x.lot, x.q, x.late, $10, $11
         FROM unnest($3::uuid[], $4::uuid[], $5::uuid[], $6::uuid[], $7::uuid[], $8::numeric[],
                     $9::bool[]) AS x(po, product, wh, bin, lot, q, late)",
        ws,
        receipt,
        &uuids(|l| l.po_line),
        &uuids(|l| l.product),
        &uuids(|l| l.warehouse),
        &opt(|l| l.bin) as &[Option<Uuid>],
        &opt(|l| l.lot) as &[Option<Uuid>],
        &qty,
        &lines.iter().map(|l| l.late).collect::<Vec<_>>(),
        date,
        recorded_at
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE purchase_order_lines l SET received_quantity = l.received_quantity + x.q
         FROM unnest($2::uuid[], $3::numeric[]) AS x(id, q)
         WHERE l.workspace_id = $1 AND l.id = x.id",
        ws,
        &uuids(|l| l.po_line),
        &qty
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A booked line's facts.
pub struct Booked {
    pub line: Uuid,
    pub po_line: Uuid,
    pub quantity: Decimal,
    pub date: NaiveDate,
    /// (net, landed, PO-currency share, rate); None in setup.
    pub value: Option<(i64, i64, i64, Rate)>,
}

/// Mark a receipt line booked from its stock movement (and its value entry's journal), and
/// add it to its order line.
pub async fn mark_booked(
    conn: &mut PgConnection,
    ws: Uuid,
    b: &Booked,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    let v = b.value;
    sqlx::query!(
        "UPDATE purchase_receipt_lines l
         SET booked_at = now(), booked_by = $3, accounting_date = $4,
             movement_id = m.id, journal_entry_id = e.journal_entry_id,
             net_value = $5, landed_value = $6, rate = $7, rate_inverse = $8,
             rate_source = $9, rate_date = $10
         FROM stock_movements m
         LEFT JOIN inventory_value_entries e ON e.movement_id = m.id
         WHERE l.workspace_id = $1 AND l.id = $2
           AND m.workspace_id = $1 AND m.related_domain = 'purchase_receipt_line'
           AND m.related_id = $2",
        ws,
        b.line,
        by,
        b.date,
        v.map(|v| v.0),
        v.map(|v| v.1),
        v.map(|v| v.3.0),
        v.map(|v| v.3.1),
        v.map(|v| v.3.2),
        v.and_then(|v| v.3.3)
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE purchase_order_lines SET booked_quantity = booked_quantity + $3,
             booked_value = booked_value + $4
         WHERE workspace_id = $1 AND id = $2",
        ws,
        b.po_line,
        b.quantity,
        v.map_or(0, |v| v.2)
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Bump a receipt's version.
pub async fn touch_receipt(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE purchase_receipts SET version = version + 1 WHERE workspace_id = $1 AND id = $2",
        ws,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Component refs of `order` already capitalised, among `refs`.
pub async fn used_components(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    order: Uuid,
    refs: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT client_component_ref FROM purchase_landed_costs
         WHERE workspace_id = $1 AND purchase_order_id = $2 AND client_component_ref = ANY($3)",
        ws,
        order,
        refs
    )
    .fetch_all(db)
    .await
}

/// A component to store.
pub struct NewLanded<'a> {
    pub component: Uuid,
    pub amount: i64,
    pub currency: Uuid,
    pub functional: i64,
    pub rate: Rate,
    pub evidence_ref: &'a str,
    /// (receipt line, functional share).
    pub allocations: &'a [(Uuid, i64)],
}

/// Store a capitalised component and its allocations.
pub async fn insert_landed(
    conn: &mut PgConnection,
    ws: Uuid,
    (order, receipt): (Uuid, Uuid),
    c: &NewLanded<'_>,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    let id = sqlx::query_scalar!(
        "INSERT INTO purchase_landed_costs (workspace_id, purchase_order_id, receipt_id,
             client_component_ref, amount, currency_id, functional_amount, rate, rate_inverse,
             rate_source, rate_date, evidence_ref, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
        ws,
        order,
        receipt,
        c.component,
        c.amount,
        c.currency,
        c.functional,
        c.rate.0,
        c.rate.1,
        c.rate.2,
        c.rate.3,
        c.evidence_ref,
        by
    )
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query!(
        "INSERT INTO purchase_landed_cost_allocations (workspace_id, landed_cost_id,
             receipt_line_id, functional_amount)
         SELECT $1, $2, x.line, x.amount FROM unnest($3::uuid[], $4::bigint[]) AS x(line, amount)",
        ws,
        id,
        &c.allocations.iter().map(|a| a.0).collect::<Vec<_>>(),
        &c.allocations.iter().map(|a| a.1).collect::<Vec<_>>()
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A stored component with its allocations as JSON `[{receipt_line_id, functional_amount}]`.
pub struct LandedRow {
    pub id: Uuid,
    pub receipt_id: Uuid,
    pub client_component_ref: Uuid,
    pub amount: i64,
    pub currency_id: Uuid,
    pub currency_code: String,
    pub currency_decimals: i16,
    pub functional_amount: i64,
    pub rate: Decimal,
    pub rate_inverse: bool,
    pub rate_source: String,
    pub rate_date: Option<NaiveDate>,
    pub evidence_ref: String,
    pub allocations: Value,
}

/// Components of `receipts`, oldest first.
pub async fn landed(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    receipts: &[Uuid],
) -> Result<Vec<LandedRow>, sqlx::Error> {
    sqlx::query_as!(
        LandedRow,
        r#"SELECT c.id, c.receipt_id, c.client_component_ref, c.amount, c.currency_id,
                  upper(cu.asset_type) AS "currency_code!", cu.decimals AS currency_decimals,
                  c.functional_amount, c.rate, c.rate_inverse, c.rate_source, c.rate_date,
                  c.evidence_ref,
                  (SELECT coalesce(json_agg(json_build_object('receipt_line_id', a.receipt_line_id,
                                                              'functional_amount', a.functional_amount)
                                            ORDER BY a.receipt_line_id), '[]')
                   FROM purchase_landed_cost_allocations a
                   WHERE a.landed_cost_id = c.id) AS "allocations!"
           FROM purchase_landed_costs c
           JOIN currencies cu ON cu.workspace_id = c.workspace_id AND cu.id = c.currency_id
           WHERE c.workspace_id = $1 AND c.receipt_id = ANY($2)
           ORDER BY c.id"#,
        ws,
        receipts
    )
    .fetch_all(db)
    .await
}

/// (quantity, value, last date) of a product/warehouse pool, if any.
pub async fn pool_state(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    product: Uuid,
    warehouse: Uuid,
) -> Result<Option<(Decimal, i64, Option<NaiveDate>)>, sqlx::Error> {
    sqlx::query!(
        "SELECT quantity, value, last_accounting_date FROM inventory_cost_pools
         WHERE workspace_id = $1 AND product_id = $2 AND warehouse_id = $3",
        ws,
        product,
        warehouse
    )
    .fetch_optional(db)
    .await
    .map(|r| r.map(|r| (r.quantity, r.value, r.last_accounting_date)))
}

/// Open confirmed orders per supplier among `suppliers`: (supplier, count, open value in
/// major units). The value counts only orders in the workspace default currency: amounts in
/// other currencies don't add up without a rate (they still count as orders).
pub async fn supplier_open(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    suppliers: &[Uuid],
) -> Result<Vec<(Uuid, i64, Decimal)>, sqlx::Error> {
    sqlx::query!(
        r#"SELECT o.supplier_id, count(*) AS "orders!",
                  coalesce(sum(o.open_value / 10::numeric ^ c.decimals)
                           FILTER (WHERE c.default_currency), 0) AS "value!"
           FROM (SELECT o.supplier_id, o.currency_id,
                        sum(money_round_div(l.extension * (l.base_quantity
                                                - l.received_quantity - l.closed_quantity),
                                            l.base_quantity)) AS open_value
                 FROM purchase_orders o
                 JOIN purchase_order_lines l ON l.purchase_order_id = o.id
                 WHERE o.workspace_id = $1 AND o.supplier_id = ANY($2) AND o.state = 'confirmed'
                 GROUP BY o.id
                 HAVING sum(l.base_quantity - l.received_quantity - l.closed_quantity) > 0) o
           JOIN currencies c ON c.workspace_id = $1 AND c.id = o.currency_id
           GROUP BY o.supplier_id"#,
        ws,
        suppliers
    )
    .fetch_all(db)
    .await
    .map(|rs| {
        rs.into_iter()
            .map(|r| (r.supplier_id, r.orders, r.value))
            .collect()
    })
}

/// Cumulative order-line share (PO-currency minor units, the last unit of the ordered quantity
/// takes the exact rest) and its functional conversion, per receipt line: (line, doc, net).
pub async fn nets(
    conn: &mut PgConnection,
    ws: Uuid,
    lines: &[Uuid],
    rate: &Rate,
    (from_dp, to_dp): (i32, i32),
) -> Result<Vec<(Uuid, i64, i64)>, sqlx::Error> {
    sqlx::query!(
        r#"WITH d AS (
               SELECT r.id,
                      CASE WHEN o.booked_quantity + r.base_quantity = o.base_quantity
                           THEN o.extension - o.booked_value
                           ELSE money_round_div(o.extension * (o.booked_quantity + r.base_quantity),
                                                o.base_quantity)::bigint - o.booked_value
                      END AS doc
               FROM purchase_receipt_lines r
               JOIN purchase_order_lines o ON o.workspace_id = r.workspace_id AND o.id = r.po_line_id
               WHERE r.workspace_id = $1 AND r.id = ANY($2))
           SELECT id AS "id!", doc AS "doc!", money_convert(doc, $3, $4, $5, $6) AS "net!"
           FROM d ORDER BY id"#,
        ws,
        lines,
        rate.0,
        from_dp,
        to_dp,
        rate.1
    )
    .fetch_all(conn)
    .await
    .map(|rs| rs.into_iter().map(|r| (r.id, r.doc, r.net)).collect())
}

/// `minor` converted with `rate` (SQL `money_convert`).
pub async fn convert(
    conn: &mut PgConnection,
    minor: i64,
    rate: &Rate,
    (from_dp, to_dp): (i32, i32),
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT money_convert($1, $2, $3, $4, $5) AS "v!""#,
        minor,
        rate.0,
        from_dp,
        to_dp,
        rate.1
    )
    .fetch_one(conn)
    .await
}

/// `total` split over `ids` by `bases`, last id taking the rest (SQL `money_allocate`); empty
/// when every basis is 0.
pub async fn allocate(
    conn: &mut PgConnection,
    total: i64,
    ids: &[Uuid],
    bases: &[i64],
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    sqlx::query!(
        r#"SELECT id AS "id!", amount AS "amount!" FROM money_allocate($1, $2, $3)"#,
        total,
        ids,
        bases
    )
    .fetch_all(conn)
    .await
    .map(|rs| rs.into_iter().map(|r| (r.id, r.amount)).collect())
}

/// Explicit component-currency shares per line converted to functional, the last line (id
/// order) taking the exact functional rest. None when the parts don't add up to
/// `amount` or a share would be negative.
pub async fn explicit_shares(
    conn: &mut PgConnection,
    parts: &[(Uuid, i64)],
    (amount, functional): (i64, i64),
    rate: &Rate,
    (from_dp, to_dp): (i32, i32),
) -> Result<Option<Vec<(Uuid, i64)>>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"WITH p AS (
               SELECT u.id, sum(u.a)::bigint AS part
               FROM unnest($1::uuid[], $2::bigint[]) AS u(id, a) GROUP BY u.id),
           c AS (
               SELECT p.id, p.part, sum(p.part) OVER () AS whole,
                      money_convert(p.part, $4, $5, $6, $7) AS conv,
                      row_number() OVER (ORDER BY p.id DESC) = 1 AS last
               FROM p)
           SELECT c.id AS "id!", c.whole::bigint AS "whole!",
                  (CASE WHEN c.last
                        THEN $3::bigint - coalesce(sum(c.conv) FILTER (WHERE NOT c.last) OVER (), 0)
                        ELSE c.conv END)::bigint AS "share!"
           FROM c ORDER BY c.id"#,
        &parts.iter().map(|p| p.0).collect::<Vec<_>>(),
        &parts.iter().map(|p| p.1).collect::<Vec<_>>(),
        functional,
        rate.0,
        from_dp,
        to_dp,
        rate.1
    )
    .fetch_all(conn)
    .await?;
    let ok = rows.first().is_some_and(|r| r.whole == amount) && rows.iter().all(|r| r.share >= 0);
    Ok(ok.then(|| rows.into_iter().map(|r| (r.id, r.share)).collect()))
}

/// Per line (line, landed sum, net + landed) and the booking total, from net values and
/// component shares.
pub async fn booking_totals(
    conn: &mut PgConnection,
    lines: &[Uuid],
    nets: &[i64],
    shares: &[(Uuid, i64)],
) -> Result<(Vec<(Uuid, i64, i64)>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"WITH n AS (SELECT * FROM unnest($1::uuid[], $2::bigint[]) AS n(id, net)),
                a AS (SELECT u.id, sum(u.s) AS landed
                      FROM unnest($3::uuid[], $4::bigint[]) AS u(id, s) GROUP BY u.id)
           SELECT n.id AS "id!", coalesce(a.landed, 0)::bigint AS "landed!",
                  (n.net + coalesce(a.landed, 0))::bigint AS "value!",
                  sum(n.net + coalesce(a.landed, 0)) OVER ()::bigint AS "total!"
           FROM n LEFT JOIN a ON a.id = n.id ORDER BY n.id"#,
        lines,
        nets,
        &shares.iter().map(|s| s.0).collect::<Vec<_>>(),
        &shares.iter().map(|s| s.1).collect::<Vec<_>>()
    )
    .fetch_all(conn)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    Ok((
        rows.into_iter()
            .map(|r| (r.id, r.landed, r.value))
            .collect(),
        total,
    ))
}

/// A pool before and after a booking.
pub struct PoolPreviewRow {
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub quantity_before: Decimal,
    pub value_before: i64,
    pub quantity_after: Decimal,
    pub value_after: i64,
}

/// Pools the receipt `lines` (with booking `values`) would change, before and after.
pub async fn pool_preview(
    conn: &mut PgConnection,
    ws: Uuid,
    lines: &[Uuid],
    values: &[i64],
) -> Result<Vec<PoolPreviewRow>, sqlx::Error> {
    sqlx::query_as!(
        PoolPreviewRow,
        r#"WITH v AS (SELECT * FROM unnest($2::uuid[], $3::bigint[]) AS v(id, value))
           SELECT l.product_id, l.warehouse_id,
                  coalesce(p.quantity, 0) AS "quantity_before!",
                  coalesce(p.value, 0) AS "value_before!",
                  coalesce(p.quantity, 0) + sum(l.base_quantity) AS "quantity_after!",
                  (coalesce(p.value, 0) + sum(v.value))::bigint AS "value_after!"
           FROM v
           JOIN purchase_receipt_lines l ON l.workspace_id = $1 AND l.id = v.id
           LEFT JOIN inventory_cost_pools p ON p.workspace_id = $1
                AND p.product_id = l.product_id AND p.warehouse_id = l.warehouse_id
           GROUP BY l.product_id, l.warehouse_id, p.quantity, p.value
           ORDER BY l.product_id, l.warehouse_id"#,
        ws,
        lines,
        values
    )
    .fetch_all(conn)
    .await
}
