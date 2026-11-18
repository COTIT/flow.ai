//! SQL for sales returns: the locked sale (recognition) a return credits, capacity per
//! recognition line, draft rows, and reads. Money math stays in the service's SQL.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::error::AppError;

/// The sale a return credits, read under its row lock.
#[derive(FromRow, Clone)]
pub struct Sale {
    pub id: Uuid,
    pub source_id: Uuid,
    pub order_id: Option<Uuid>,
    pub invoice_id: Option<Uuid>,
    pub relation_id: Option<Uuid>,
    pub currency_id: Uuid,
    pub decimals: i16,
    pub gross: i64,
    /// Billed on an issued invoice (or itself an invoice): credits are invoice credits.
    pub billed: bool,
}

/// The order or invoice a recognition hangs on (immutable, read before any lock).
pub async fn anchor(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(Option<Uuid>, Option<Uuid>)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT order_id, invoice_id FROM sales_recognitions WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

/// Recognition `id` FOR UPDATE: every route crediting it queues here.
pub async fn lock_sale(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<Sale, AppError> {
    sqlx::query_as(
        "SELECT r.id, r.source_kind, r.source_id, r.order_id, r.invoice_id, r.relation_id,
                r.currency_id, c.decimals, r.gross,
                r.source_kind = 'invoice' OR EXISTS (
                    SELECT 1 FROM invoice_recognition_links l
                    WHERE l.workspace_id = r.workspace_id AND l.recognition_id = r.id
                      AND l.state = 'issued') AS billed
         FROM sales_recognitions r
         JOIN currencies c ON c.workspace_id = r.workspace_id AND c.id = r.currency_id
         WHERE r.workspace_id = $1 AND r.id = $2
         FOR UPDATE OF r",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Per input line (recognition line, quantity): what is left to credit, None when the line
/// isn't the sale's. Left = recognised − Σ draft and posted credits of other returns.
pub async fn left(
    conn: &mut PgConnection,
    ws: Uuid,
    (sale, except): (Uuid, Option<Uuid>),
    ids: &[Uuid],
) -> Result<Vec<(Uuid, Decimal)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT l.id, l.quantity - coalesce((
                    SELECT sum(x.quantity) FROM sales_return_lines x
                    JOIN sales_returns r ON r.id = x.return_id
                    WHERE x.workspace_id = $1 AND x.recognition_line_id = l.id
                      AND r.state IN ('draft', 'posted')
                      AND r.id IS DISTINCT FROM $3), 0)
         FROM sales_recognition_lines l
         WHERE l.workspace_id = $1 AND l.recognition_id = $2 AND l.id = ANY($4)",
    )
    .bind(ws)
    .bind(sale)
    .bind(except)
    .bind(ids)
    .fetch_all(conn)
    .await?)
}

/// A new draft row.
pub struct NewReturn<'a> {
    pub client_ref: Uuid,
    pub kind: &'a str,
    pub sale: &'a Sale,
    pub credit_invoice_id: Option<Uuid>,
    pub reason: &'a str,
    pub date: NaiveDate,
    pub by: Uuid,
}

/// Insert draft `r`; its id.
pub async fn insert(
    conn: &mut PgConnection,
    ws: Uuid,
    r: &NewReturn<'_>,
) -> Result<Uuid, AppError> {
    Ok(sqlx::query_scalar(
        "INSERT INTO sales_returns (workspace_id, client_ref, kind, recognition_id, order_id,
             relation_id, credit_invoice_id, reason, accounting_date, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(ws)
    .bind(r.client_ref)
    .bind(r.kind)
    .bind(r.sale.id)
    .bind(r.sale.order_id)
    .bind(r.sale.relation_id)
    .bind(r.credit_invoice_id)
    .bind(r.reason)
    .bind(r.date)
    .bind(r.by)
    .fetch_one(conn)
    .await?)
}

/// Replace a draft's lines.
pub async fn replace_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    (id, sale): (Uuid, Uuid),
    lines: &[(Uuid, Decimal)],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM sales_return_lines WHERE workspace_id = $1 AND return_id = $2")
        .bind(ws)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let ids: Vec<Uuid> = lines.iter().map(|l| l.0).collect();
    let qty: Vec<Decimal> = lines.iter().map(|l| l.1).collect();
    sqlx::query(
        "INSERT INTO sales_return_lines (workspace_id, return_id, recognition_id,
             recognition_line_id, quantity)
         SELECT $1, $2, $3, u.l, u.q FROM UNNEST($4::uuid[], $5::numeric[]) AS u(l, q)",
    )
    .bind(ws)
    .bind(id)
    .bind(sale)
    .bind(&ids)
    .bind(&qty)
    .execute(conn)
    .await?;
    Ok(())
}

/// Lock return `id` (404 outside the workspace); (recognition, state, version).
pub async fn lock(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<(Uuid, String, i64), AppError> {
    sqlx::query_as(
        "SELECT recognition_id, state, version FROM sales_returns
         WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// The recognition of return `id` (unlocked; it never changes).
pub async fn recognition_of(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<Uuid, AppError> {
    sqlx::query_scalar(
        "SELECT recognition_id FROM sales_returns WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Draft date/reason change.
pub async fn update_draft(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    (date, reason): (NaiveDate, &str),
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE sales_returns SET accounting_date = $3, reason = $4, version = version + 1,
                updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(date)
    .bind(reason)
    .execute(conn)
    .await?;
    Ok(())
}

/// Cancel a draft (frees its capacity).
pub async fn cancel(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    (reason, by): (&str, Uuid),
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE sales_returns SET state = 'cancelled', cancelled_by = $4, cancelled_at = now(),
                cancel_reason = $3, version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(reason)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

/// A return row for its resource.
#[derive(FromRow)]
pub struct ReturnRow {
    pub id: Uuid,
    pub kind: String,
    pub recognition_id: Uuid,
    pub order_id: Option<Uuid>,
    pub relation_id: Option<Uuid>,
    pub credit_invoice_id: Option<Uuid>,
    pub state: String,
    pub version: i64,
    pub reason: String,
    pub accounting_date: NaiveDate,
    pub net: Option<i64>,
    pub tax: Option<i64>,
    pub rounding: Option<i64>,
    pub credit_item_id: Option<Uuid>,
    pub decimals: i16,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub posted_at: Option<DateTime<Utc>>,
    pub reversed_at: Option<DateTime<Utc>>,
    pub reversal_reason: Option<String>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
}

/// Returns of the workspace, newest first (`id` = one); (page rows, total).
pub async fn returns(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Option<Uuid>,
    (limit, offset): (i64, i64),
) -> Result<(Vec<ReturnRow>, i64), AppError> {
    let rows = sqlx::query_as(
        "SELECT r.id, r.kind, r.recognition_id, r.order_id, r.relation_id, r.credit_invoice_id,
                r.state, r.version, r.reason, r.accounting_date, r.net, r.tax, r.rounding,
                r.credit_item_id, c.decimals, r.created_by, r.created_at, r.updated_at,
                r.posted_at, r.reversed_at, r.reversal_reason, r.cancelled_at, r.cancel_reason
         FROM sales_returns r
         JOIN sales_recognitions g ON g.workspace_id = r.workspace_id AND g.id = r.recognition_id
         JOIN currencies c ON c.workspace_id = g.workspace_id AND c.id = g.currency_id
         WHERE r.workspace_id = $1 AND ($2::uuid IS NULL OR r.id = $2)
         ORDER BY r.created_at DESC, r.id DESC LIMIT $3 OFFSET $4",
    )
    .bind(ws)
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sales_returns WHERE workspace_id = $1 AND ($2::uuid IS NULL OR id = $2)",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(conn)
    .await?;
    Ok((rows, total))
}

/// A return line with its product and restocked quantity.
#[derive(FromRow)]
pub struct LineRow {
    pub id: Uuid,
    pub return_id: Uuid,
    pub recognition_line_id: Uuid,
    pub product_id: Option<Uuid>,
    pub quantity: Decimal,
    pub net: Option<i64>,
    pub tax: Option<i64>,
    pub restocked: Decimal,
}

/// Lines of `ids`, recognition line order.
pub async fn lines(
    conn: &mut PgConnection,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<LineRow>, AppError> {
    Ok(sqlx::query_as(
        "SELECT x.id, x.return_id, x.recognition_line_id, l.product_id, x.quantity, x.net, x.tax,
                coalesce((SELECT sum(k.quantity) FROM sales_return_restock_lines k
                          JOIN sales_return_restocks s ON s.id = k.restock_id
                          WHERE k.workspace_id = x.workspace_id AND k.return_line_id = x.id
                            AND s.state = 'posted'), 0) AS restocked
         FROM sales_return_lines x
         JOIN sales_recognition_lines l ON l.workspace_id = x.workspace_id
                                        AND l.id = x.recognition_line_id
         WHERE x.workspace_id = $1 AND x.return_id = ANY($2)
         ORDER BY x.return_id, l.id",
    )
    .bind(ws)
    .bind(ids)
    .fetch_all(conn)
    .await?)
}

/// A restock with its lines (one row per line).
#[derive(FromRow)]
pub struct RestockRow {
    pub id: Uuid,
    pub return_id: Uuid,
    pub warehouse_id: Uuid,
    pub state: String,
    pub version: i64,
    pub accounting_date: NaiveDate,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub reversed_at: Option<DateTime<Utc>>,
    pub reversal_reason: Option<String>,
    pub return_line_id: Uuid,
    pub quantity: Decimal,
    pub lot_id: Option<Uuid>,
    pub value: i64,
    pub legacy_return_cost_id: Option<Uuid>,
}

/// Restock lines of returns `ids`, oldest restock first.
pub async fn restocks(
    conn: &mut PgConnection,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<RestockRow>, AppError> {
    Ok(sqlx::query_as(
        "SELECT s.id, s.return_id, s.warehouse_id, s.state, s.version, s.accounting_date,
                s.created_by, s.created_at, s.reversed_at, s.reversal_reason, k.return_line_id,
                k.quantity, k.lot_id, k.value, k.legacy_return_cost_id
         FROM sales_return_restocks s
         JOIN sales_return_restock_lines k ON k.restock_id = s.id
         WHERE s.workspace_id = $1 AND s.return_id = ANY($2)
         ORDER BY s.created_at, s.id, k.id",
    )
    .bind(ws)
    .bind(ids)
    .fetch_all(conn)
    .await?)
}

/// The legacy credit note aliasing return `id`, if any.
pub async fn credit_invoice(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT credit_invoice_id FROM sales_returns WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(conn)
    .await?)
}

/// Posted money and links of return `id`.
pub struct Posted {
    pub net: i64,
    pub tax: i64,
    pub rounding: i64,
    pub credit_item_id: Uuid,
    pub journal_entry_id: Uuid,
    pub effect_id: Uuid,
}

/// Write each line's money then stamp the header posted (lines first: they freeze after).
pub async fn mark_posted(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    lines: &[(Uuid, i64, i64)],
    p: &Posted,
    by: Uuid,
) -> Result<(), AppError> {
    let ids: Vec<Uuid> = lines.iter().map(|l| l.0).collect();
    let net: Vec<i64> = lines.iter().map(|l| l.1).collect();
    let tax: Vec<i64> = lines.iter().map(|l| l.2).collect();
    sqlx::query(
        "UPDATE sales_return_lines x SET net = u.n, tax = u.t
         FROM UNNEST($2::uuid[], $3::int8[], $4::int8[]) AS u(id, n, t)
         WHERE x.workspace_id = $1 AND x.id = u.id",
    )
    .bind(ws)
    .bind(&ids)
    .bind(&net)
    .bind(&tax)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE sales_returns SET state = 'posted', net = $3, tax = $4, rounding = $5,
                credit_item_id = $6, journal_entry_id = $7, effect_id = $8, posted_by = $9,
                posted_at = now(), version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(p.net)
    .bind(p.tax)
    .bind(p.rounding)
    .bind(p.credit_item_id)
    .bind(p.journal_entry_id)
    .bind(p.effect_id)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

/// A return line as a restock sees it, under the sale lock.
#[derive(FromRow)]
pub struct Physical {
    pub id: Uuid,
    pub product_id: Option<Uuid>,
    pub order_line_id: Option<Uuid>,
    /// Warehouse the sale took the stock from; None when it moved none.
    pub origin: Option<Uuid>,
    /// Units still to put back: min(this line − its restocks, posted credits of the sale line
    /// − every restock of that sale line, reversed credits' restocks included).
    pub left: Decimal,
}

/// Lines `ids` of return `id` (others are absent).
pub async fn physical(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Physical>, AppError> {
    Ok(sqlx::query_as(
        "WITH k AS (
             SELECT k.return_line_id, sum(k.quantity) AS q FROM sales_return_restock_lines k
             JOIN sales_return_restocks s ON s.id = k.restock_id
             WHERE k.workspace_id = $1 AND s.state = 'posted' GROUP BY k.return_line_id)
         SELECT x.id, x.quantity, l.product_id, l.order_line_id,
                CASE WHEN ol.id IS NOT NULL THEN CASE WHEN ol.tracked THEN o.warehouse_id END
                     ELSE (SELECT i.mutate_from_warehouse_id FROM items i
                           WHERE i.workspace_id = l.workspace_id AND i.id = l.invoice_item_id
                             AND i.mutated_from_stock <> 0) END AS origin,
                least(x.quantity - coalesce((SELECT q FROM k WHERE k.return_line_id = x.id), 0),
                      coalesce((SELECT sum(y.quantity) FROM sales_return_lines y
                                JOIN sales_returns r ON r.id = y.return_id
                                WHERE y.workspace_id = $1
                                  AND y.recognition_line_id = x.recognition_line_id
                                  AND r.state = 'posted'), 0)
                      - coalesce((SELECT sum(k.q) FROM k JOIN sales_return_lines y
                                         ON y.id = k.return_line_id
                                  WHERE y.recognition_line_id = x.recognition_line_id), 0))
                    AS left
         FROM sales_return_lines x
         JOIN sales_recognition_lines l ON l.workspace_id = x.workspace_id
                                        AND l.id = x.recognition_line_id
         LEFT JOIN order_lines ol ON ol.workspace_id = l.workspace_id AND ol.id = l.order_line_id
         LEFT JOIN orders o ON o.workspace_id = ol.workspace_id AND o.id = ol.order_id
         WHERE x.workspace_id = $1 AND x.return_id = $2 AND x.id = ANY($3)",
    )
    .bind(ws)
    .bind(id)
    .bind(ids)
    .fetch_all(conn)
    .await?)
}

/// Units of `product` already put back (posted restocks) on returns of order `order`.
pub async fn restocked_of_order(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    product: Uuid,
) -> Result<Decimal, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(sum(k.quantity), 0) FROM sales_return_restock_lines k
         JOIN sales_return_restocks s ON s.id = k.restock_id AND s.state = 'posted'
         JOIN sales_returns r ON r.id = s.return_id
         JOIN sales_return_lines x ON x.id = k.return_line_id
         JOIN sales_recognition_lines l ON l.id = x.recognition_line_id
         WHERE k.workspace_id = $1 AND r.order_id = $2 AND l.product_id = $3",
    )
    .bind(ws)
    .bind(order)
    .bind(product)
    .fetch_one(conn)
    .await?)
}

/// Legacy return cost `id` for order line `line`, if it exists in the workspace.
pub async fn legacy_fits(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    line: Option<Uuid>,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM legacy_return_costs
                        WHERE workspace_id = $1 AND id = $2 AND order_line_id = $3)",
    )
    .bind(ws)
    .bind(id)
    .bind(line)
    .fetch_one(conn)
    .await?)
}

/// Insert a restock header; its id.
pub async fn insert_restock(
    conn: &mut PgConnection,
    ws: Uuid,
    (client_ref, id, warehouse): (Uuid, Uuid, Uuid),
    (date, by): (NaiveDate, Uuid),
) -> Result<Uuid, AppError> {
    Ok(sqlx::query_scalar(
        "INSERT INTO sales_return_restocks (workspace_id, client_ref, return_id, warehouse_id,
             accounting_date, created_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(ws)
    .bind(client_ref)
    .bind(id)
    .bind(warehouse)
    .bind(date)
    .bind(by)
    .fetch_one(conn)
    .await?)
}

/// Book value the restock `id`'s movements moved so far (Σ value entries).
pub async fn restock_value(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(sum(value_delta), 0)::int8 FROM inventory_value_entries
         WHERE workspace_id = $1 AND source_kind = 'sales_return_restock' AND source_id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(conn)
    .await?)
}

/// One restock line.
pub async fn insert_restock_line(
    conn: &mut PgConnection,
    ws: Uuid,
    (restock, line): (Uuid, Uuid),
    (quantity, lot, value, legacy): (Decimal, Option<Uuid>, i64, Option<Uuid>),
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO sales_return_restock_lines (workspace_id, restock_id, return_line_id,
             quantity, lot_id, value, legacy_return_cost_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(ws)
    .bind(restock)
    .bind(line)
    .bind(quantity)
    .bind(lot)
    .bind(value)
    .bind(legacy)
    .execute(conn)
    .await?;
    Ok(())
}

/// Allocations touching credit item `item` that aren't reversed, other than the post's own
/// application (effect `own`): refunds or applications that depend on the credit.
pub async fn dependents(
    conn: &mut PgConnection,
    ws: Uuid,
    item: Uuid,
    own: Uuid,
) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM accounting_allocations a
         WHERE a.workspace_id = $1 AND (a.source_item_id = $2 OR a.target_item_id = $2)
           AND a.reversal_of IS NULL AND a.effect_id <> $3
           AND NOT EXISTS (SELECT 1 FROM accounting_allocations b
                           WHERE b.workspace_id = a.workspace_id AND b.reversal_of = a.id)",
    )
    .bind(ws)
    .bind(item)
    .bind(own)
    .fetch_one(conn)
    .await?)
}

/// The post's own application of credit item `item` (effect `own`), if any: (allocation,
/// amount, role of the receivable it relieved).
pub async fn own_application(
    conn: &mut PgConnection,
    ws: Uuid,
    item: Uuid,
    own: Uuid,
) -> Result<Option<(Uuid, i64, String)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT a.id, a.amount, t.account_role FROM accounting_allocations a
         JOIN accounting_open_items t ON t.workspace_id = a.workspace_id AND t.id = a.target_item_id
         WHERE a.workspace_id = $1 AND a.source_item_id = $2 AND a.effect_id = $3
           AND a.reversal_of IS NULL",
    )
    .bind(ws)
    .bind(item)
    .bind(own)
    .fetch_optional(conn)
    .await?)
}

/// Posted facts of return `id` a reversal needs.
#[derive(FromRow)]
pub struct Reversible {
    pub net: i64,
    pub tax: i64,
    pub rounding: i64,
    pub credit_item_id: Uuid,
    pub effect_id: Uuid,
}

pub async fn reversible(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Reversible, AppError> {
    sqlx::query_as(
        "SELECT net, tax, rounding, credit_item_id, effect_id FROM sales_returns
         WHERE workspace_id = $1 AND id = $2 AND state = 'posted'",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| AppError::Unprocessable("Only a posted credit can be reversed.".into()))
}

/// Stamp return `id` reversed.
pub async fn mark_reversed(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    (effect, reason, by): (Uuid, &str, Uuid),
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE sales_returns SET state = 'reversed', reversal_effect_id = $3, reversed_by = $5,
                reversed_at = now(), reversal_reason = $4, version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(effect)
    .bind(reason)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

/// A restock line to undo, with its product.
#[derive(FromRow)]
pub struct Undo {
    pub product_id: Uuid,
    pub lot_id: Option<Uuid>,
    pub quantity: Decimal,
    pub value: i64,
    pub legacy_return_cost_id: Option<Uuid>,
}

/// Restock `restock` of return `id` FOR UPDATE: (state, warehouse) and its lines.
pub async fn lock_restock(
    conn: &mut PgConnection,
    ws: Uuid,
    (id, restock): (Uuid, Uuid),
) -> Result<(String, Uuid, Vec<Undo>), AppError> {
    let (state, warehouse): (String, Uuid) = sqlx::query_as(
        "SELECT state, warehouse_id FROM sales_return_restocks
         WHERE workspace_id = $1 AND id = $2 AND return_id = $3 FOR UPDATE",
    )
    .bind(ws)
    .bind(restock)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let lines = sqlx::query_as(
        "SELECT l.product_id, k.lot_id, k.quantity, k.value, k.legacy_return_cost_id
         FROM sales_return_restock_lines k
         JOIN sales_return_lines x ON x.id = k.return_line_id
         JOIN sales_recognition_lines l ON l.id = x.recognition_line_id
         WHERE k.workspace_id = $1 AND k.restock_id = $2 ORDER BY k.id",
    )
    .bind(ws)
    .bind(restock)
    .fetch_all(conn)
    .await?;
    Ok((state, warehouse, lines))
}

/// Whether a valued movement came after restock `restock` in any pool it touched.
pub async fn valued_since(
    conn: &mut PgConnection,
    ws: Uuid,
    restock: Uuid,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM inventory_value_entries e
             JOIN inventory_cost_pools p ON p.id = e.pool_id
             WHERE e.workspace_id = $1 AND e.source_kind = 'sales_return_restock'
               AND e.source_id = $2
             GROUP BY p.id, p.valuation_sequence
             HAVING max(e.sequence) <> p.valuation_sequence)",
    )
    .bind(ws)
    .bind(restock)
    .fetch_one(conn)
    .await?)
}

/// Stamp restock `restock` reversed.
pub async fn mark_restock_reversed(
    conn: &mut PgConnection,
    ws: Uuid,
    restock: Uuid,
    (reason, by): (&str, Uuid),
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE sales_return_restocks SET state = 'reversed', reversed_by = $4, reversed_at = now(),
                reversal_reason = $3, version = version + 1
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(restock)
    .bind(reason)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

/// Order line `line` of order `order` in the workspace: (product, deducted).
pub async fn order_line(
    conn: &mut PgConnection,
    ws: Uuid,
    (order, line): (Uuid, Uuid),
) -> Result<Option<(Uuid, Decimal)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT product_id, deducted FROM order_lines
         WHERE workspace_id = $1 AND order_id = $2 AND id = $3",
    )
    .bind(ws)
    .bind(order)
    .bind(line)
    .fetch_optional(conn)
    .await?)
}

/// A reviewed legacy cost row.
#[derive(FromRow)]
pub struct LegacyRow {
    pub id: Uuid,
    pub order_id: Uuid,
    pub order_line_id: Uuid,
    pub quantity: Decimal,
    pub restoration_amount: i64,
    pub target_product_id: Option<Uuid>,
    pub evidence_ref: String,
    pub reason: String,
    pub consumed_quantity: Decimal,
    pub reviewed_by: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Insert a reviewed legacy cost; the row.
pub async fn insert_legacy(
    conn: &mut PgConnection,
    ws: Uuid,
    client_ref: Uuid,
    c: &crate::modules::sales_returns::dto::LegacyCostInput,
    (amount, by): (i64, Uuid),
) -> Result<LegacyRow, AppError> {
    Ok(sqlx::query_as(
        "INSERT INTO legacy_return_costs (workspace_id, client_ref, order_id, order_line_id,
             quantity, restoration_amount, target_product_id, evidence_ref, reason, reviewed_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING id, order_id, order_line_id, quantity, restoration_amount, target_product_id,
                   evidence_ref, reason, consumed_quantity, reviewed_by, created_at",
    )
    .bind(ws)
    .bind(client_ref)
    .bind(c.order_id)
    .bind(c.line_id)
    .bind(c.quantity)
    .bind(amount)
    .bind(c.target_product_id)
    .bind(&c.evidence_ref)
    .bind(&c.reason)
    .bind(by)
    .fetch_one(conn)
    .await?)
}
