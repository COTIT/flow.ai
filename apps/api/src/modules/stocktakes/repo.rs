//! SQL for stocktakes and their frozen lines. Every query filters by workspace.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A stocktake header.
pub struct Head {
    pub id: Uuid,
    pub warehouse_id: Uuid,
    pub state: String,
    pub version: i64,
    pub reason: String,
    pub cancel_reason: Option<String>,
    pub accounting_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub posted_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

/// A frozen line plus its stock row's live hold.
pub struct Line {
    pub stock_id: Uuid,
    pub product_id: Uuid,
    pub shelf_id: Option<Uuid>,
    pub bin_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    pub expected: Decimal,
    pub counted: Option<Decimal>,
    pub value: Option<i64>,
    pub reserved: Decimal,
}

/// New draft; its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    warehouse: Uuid,
    reason: &str,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO stocktakes (workspace_id, warehouse_id, reason, created_by)
         VALUES ($1, $2, $3, $4) RETURNING id",
        ws,
        warehouse,
        reason,
        by
    )
    .fetch_one(db)
    .await
}

/// The header, `FOR UPDATE` when `lock`.
pub async fn head(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    lock: bool,
) -> Result<Option<Head>, sqlx::Error> {
    if lock {
        sqlx::query_scalar!(
            "SELECT id FROM stocktakes WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
            ws,
            id
        )
        .fetch_optional(&mut *conn)
        .await?;
    }
    sqlx::query_as!(
        Head,
        "SELECT id, warehouse_id, state, version, reason, cancel_reason, accounting_date,
                created_at, started_at, posted_at, cancelled_at
         FROM stocktakes WHERE workspace_id = $1 AND id = $2",
        ws,
        id
    )
    .fetch_optional(conn)
    .await
}

/// One page of headers, newest first, and the total.
pub async fn list(
    conn: &mut PgConnection,
    ws: Uuid,
    (limit, offset): (i64, i64),
) -> Result<(Vec<Head>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        Head,
        "SELECT id, warehouse_id, state, version, reason, cancel_reason, accounting_date,
                created_at, started_at, posted_at, cancelled_at
         FROM stocktakes WHERE workspace_id = $1
         ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
        ws,
        limit,
        offset
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM stocktakes WHERE workspace_id = $1"#,
        ws
    )
    .fetch_one(conn)
    .await?;
    Ok((rows, total))
}

/// Lines in stock-row order.
pub async fn lines(db: impl PgExecutor<'_>, ws: Uuid, id: Uuid) -> Result<Vec<Line>, sqlx::Error> {
    sqlx::query_as!(
        Line,
        r#"SELECT l.stock_id, l.product_id, l.shelf_id, l.bin_id, l.lot_id, l.expected, l.counted,
                  l.positive_adjustment_value AS value, s.reserved
           FROM stocktake_lines l JOIN stocks s ON s.id = l.stock_id
           WHERE l.workspace_id = $1 AND l.stocktake_id = $2
           ORDER BY l.product_id, l.bin_id NULLS FIRST, l.lot_id NULLS FIRST, l.stock_id"#,
        ws,
        id
    )
    .fetch_all(db)
    .await
}

/// Freeze every live stock row of the warehouse as a line.
pub async fn snapshot(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    warehouse: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO stocktake_lines (workspace_id, stocktake_id, stock_id, product_id, shelf_id,
             bin_id, lot_id, expected)
         SELECT workspace_id, $2, id, product_id, shelf_id, bin_id, lot_id, stock_lvl
         FROM stocks WHERE workspace_id = $1 AND warehouse_id = $3 AND deleted_at IS NULL",
        ws,
        id,
        warehouse
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Per sent line (in order): whether it is a line of this stocktake, and its value in minor
/// units at `dp` decimals (None: no value sent, or more decimals than the currency has).
pub async fn check_counts(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    stock_ids: &[Uuid],
    values: &[Option<Decimal>],
    dp: i32,
) -> Result<Vec<(bool, Option<i64>)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT l.stock_id IS NOT NULL AS "known!",
                  CASE WHEN u.v * 10 ^ $5::int = trunc(u.v * 10 ^ $5::int)
                       THEN (u.v * 10 ^ $5::int)::bigint END AS minor
           FROM unnest($3::uuid[], $4::numeric[]) WITH ORDINALITY AS u(stock_id, v, i)
           LEFT JOIN stocktake_lines l
             ON l.workspace_id = $1 AND l.stocktake_id = $2 AND l.stock_id = u.stock_id
           ORDER BY u.i"#,
        ws,
        id,
        stock_ids,
        values as &[Option<Decimal>],
        dp
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.known, r.minor)).collect())
}

/// Store counts (and values, minor units) of the sent lines.
pub async fn set_counts(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    stock_ids: &[Uuid],
    counted: &[Decimal],
    values: &[Option<i64>],
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE stocktake_lines l
         SET counted = u.counted, positive_adjustment_value = u.v, counted_by = $6,
             counted_at = now()
         FROM unnest($3::uuid[], $4::numeric[], $5::bigint[]) AS u(stock_id, counted, v)
         WHERE l.workspace_id = $1 AND l.stocktake_id = $2 AND l.stock_id = u.stock_id",
        ws,
        id,
        stock_ids,
        counted,
        values as &[Option<i64>],
        by
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Move to `state`, stamping who/when for it; version + 1.
pub async fn set_state(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    state: &str,
    by: Uuid,
    (accounting_date, cancel_reason): (Option<NaiveDate>, Option<&str>),
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE stocktakes SET state = $3, version = version + 1, updated_at = now(),
             started_by = CASE WHEN $3 = 'counting' THEN $4 ELSE started_by END,
             started_at = CASE WHEN $3 = 'counting' THEN now() ELSE started_at END,
             posted_by = CASE WHEN $3 = 'posted' THEN $4 ELSE posted_by END,
             posted_at = CASE WHEN $3 = 'posted' THEN now() ELSE posted_at END,
             cancelled_by = CASE WHEN $3 = 'cancelled' THEN $4 ELSE cancelled_by END,
             cancelled_at = CASE WHEN $3 = 'cancelled' THEN now() ELSE cancelled_at END,
             accounting_date = coalesce($5, accounting_date),
             cancel_reason = coalesce($6, cancel_reason)
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        state,
        by,
        accounting_date,
        cancel_reason
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Version + 1 (a count changed).
pub async fn bump(db: impl PgExecutor<'_>, ws: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE stocktakes SET version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Pool quantity per product at the warehouse (no pool = absent).
pub async fn pool_quantities(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    warehouse: Uuid,
    products: &[Uuid],
) -> Result<Vec<(Uuid, Decimal)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT product_id, quantity FROM inventory_cost_pools
         WHERE workspace_id = $1 AND warehouse_id = $2 AND product_id = ANY($3)",
        ws,
        warehouse,
        products
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.product_id, r.quantity))
        .collect())
}

/// Value change per stock row this stocktake's movements booked.
pub async fn values(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT m.stock_id, e.value_delta FROM stock_movements m
         JOIN inventory_value_entries e ON e.workspace_id = m.workspace_id AND e.movement_id = m.id
         WHERE m.workspace_id = $1 AND m.related_domain = 'stocktake' AND m.related_id = $2",
        ws,
        id
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.stock_id, r.value_delta))
        .collect())
}

/// `(lot, product)` with holds at the warehouse that picking would skip: not released or past
/// expiry on the book's business date.
pub async fn invalid_held_lots(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    warehouse: Uuid,
) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT DISTINCT l.id AS "lot!", l.product_id AS "product!" FROM stock_reservations r
           JOIN stocks s ON s.workspace_id = r.workspace_id AND s.id = r.stock_id
           JOIN lots l ON l.workspace_id = s.workspace_id AND l.id = s.lot_id
           WHERE r.workspace_id = $1 AND s.warehouse_id = $2
             AND (l.status <> 'released' OR l.expires_on < current_date)
           ORDER BY 1"#,
        ws,
        warehouse
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.lot, r.product)).collect())
}

/// Functional currency decimals of the book (2 when none is set).
pub async fn functional_dp(db: impl PgExecutor<'_>, ws: Uuid) -> Result<i32, sqlx::Error> {
    let dp = sqlx::query_scalar!(
        "SELECT c.decimals FROM accounting_settings s
         JOIN currencies c ON c.id = s.functional_currency_id
         WHERE s.workspace_id = $1",
        ws
    )
    .fetch_optional(db)
    .await?;
    Ok(dp.map_or(2, i32::from))
}
