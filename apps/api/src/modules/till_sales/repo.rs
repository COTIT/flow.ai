//! Till sale SQL: the per-ref lock, `till_sales` rows with their order number, the order's
//! sale times and the drawer's `sale` ledger row. Every query filters `workspace_id`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A till sale with its order number.
pub struct SaleRow {
    pub order_id: Uuid,
    pub number: String,
    pub client_ref: String,
    pub register_id: Uuid,
    pub till_user_id: Uuid,
    pub request_hash: Vec<u8>,
    pub method: String,
    pub total: i64,
    pub server_total: i64,
    pub price_mismatch: i64,
    pub mismatched_lines: i32,
    pub due: i64,
    pub rounding: i64,
    pub tendered: Option<i64>,
    pub change: Option<i64>,
    pub card_reference: Option<String>,
    pub sold_at: DateTime<Utc>,
}

/// Fields of a new `till_sales` row.
pub struct NewSaleRow<'a> {
    pub order_id: Uuid,
    pub register_id: Uuid,
    pub till_user_id: Uuid,
    pub shift_id: Option<Uuid>,
    pub client_ref: &'a str,
    pub request_hash: &'a [u8; 32],
    pub method: &'a str,
    pub total: i64,
    pub server_total: i64,
    pub mismatched_lines: i32,
    pub server_prices: &'a Value,
    pub due: i64,
    pub tendered: Option<i64>,
    pub card_reference: Option<&'a str>,
    pub sold_at: DateTime<Utc>,
}

/// Hold this register's `client_ref` until the transaction ends: copies of one sale queue up
/// here, so the second finds the first's row instead of writing again.
pub async fn lock_ref(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    register_id: Uuid,
    client_ref: &str,
) -> Result<(), sqlx::Error> {
    let key = format!("till_sale:{workspace_id}:{register_id}:{client_ref}");
    sqlx::query!("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", key)
        .execute(conn)
        .await
        .map(|_| ())
}

/// The register's sale under `client_ref`.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    register_id: Uuid,
    client_ref: &str,
) -> Result<Option<SaleRow>, sqlx::Error> {
    sqlx::query_as!(
        SaleRow,
        "SELECT s.order_id, o.number, s.client_ref, s.register_id, s.till_user_id,
                s.request_hash, s.method, s.total, s.server_total, s.price_mismatch,
                s.mismatched_lines, s.due,
                s.rounding, s.tendered, s.change, s.card_reference, s.sold_at
         FROM till_sales s
         JOIN orders o ON o.workspace_id = s.workspace_id AND o.id = s.order_id
         WHERE s.workspace_id = $1 AND s.register_id = $2 AND s.client_ref = $3",
        workspace_id,
        register_id,
        client_ref,
    )
    .fetch_optional(db)
    .await
}

/// Store the sale.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    s: &NewSaleRow<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO till_sales (order_id, workspace_id, register_id, till_user_id, shift_id,
                                 client_ref, request_hash, method, total, server_total,
                                 price_mismatch, mismatched_lines, server_prices, due, rounding,
                                 tendered, change, card_reference, sold_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $9::bigint - $10::bigint, $16, $11,
                 $12, $12::bigint - $9::bigint, $13, $13::bigint - $12::bigint, $14, $15)",
        s.order_id,
        workspace_id,
        s.register_id,
        s.till_user_id,
        s.shift_id,
        s.client_ref,
        &s.request_hash[..],
        s.method,
        s.total,
        s.server_total,
        s.server_prices,
        s.due,
        s.tendered,
        s.card_reference,
        s.sold_at,
        s.mismatched_lines,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Stamp a till order paid and completed when it was sold.
pub async fn stamp_order(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    order_id: Uuid,
    sold_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE orders SET paid_at = $3, completed_at = $3, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        order_id,
        sold_at,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// The cash a sale put in the drawer: one `sale` row of the shift.
pub async fn book_cash(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    register_id: Uuid,
    shift_id: Uuid,
    order_id: Uuid,
    amount: i64,
    till_user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO cash_ledger (workspace_id, register_id, shift_id, kind, order_id, amount,
                                  till_user_id)
         VALUES ($1, $2, $3, 'sale', $4, $5, $6)",
        workspace_id,
        register_id,
        shift_id,
        order_id,
        amount,
        till_user_id,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Which sales the office list returns; `None` = any.
#[derive(Default)]
pub struct ListFilter {
    /// Only sales with a line charged at another price than the catalog's.
    pub mismatch: bool,
    pub register_id: Option<Uuid>,
    /// `sold_at` at or after.
    pub from: Option<DateTime<Utc>>,
    /// `sold_at` before.
    pub to: Option<DateTime<Utc>>,
}

/// One page of the workspace's till sales matching `f`, newest first, and the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SaleRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT s.order_id, o.number, s.client_ref, s.register_id, s.till_user_id,
                  s.request_hash, s.method, s.total, s.server_total, s.price_mismatch,
                s.mismatched_lines, s.due,
                  s.rounding, s.tendered, s.change, s.card_reference, s.sold_at,
                  count(*) OVER () AS "total_rows!"
           FROM till_sales s
           JOIN orders o ON o.workspace_id = s.workspace_id AND o.id = s.order_id
           WHERE s.workspace_id = $1 AND (NOT $2 OR s.mismatched_lines > 0)
             AND ($5::uuid IS NULL OR s.register_id = $5)
             AND ($6::timestamptz IS NULL OR s.sold_at >= $6)
             AND ($7::timestamptz IS NULL OR s.sold_at < $7)
           ORDER BY s.created_at DESC, s.order_id DESC
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        f.mismatch,
        limit,
        offset,
        f.register_id,
        f.from,
        f.to,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total_rows);
    let rows = rows
        .into_iter()
        .map(|s| SaleRow {
            order_id: s.order_id,
            number: s.number,
            client_ref: s.client_ref,
            register_id: s.register_id,
            till_user_id: s.till_user_id,
            request_hash: s.request_hash,
            method: s.method,
            total: s.total,
            server_total: s.server_total,
            price_mismatch: s.price_mismatch,
            mismatched_lines: s.mismatched_lines,
            due: s.due,
            rounding: s.rounding,
            tendered: s.tendered,
            change: s.change,
            card_reference: s.card_reference,
            sold_at: s.sold_at,
        })
        .collect();
    Ok((rows, total))
}
