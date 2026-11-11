//! Attention SQL: per section one count statement and one items statement, each filtered by
//! `workspace_id`. Status sets and filters copy the shape of the rule they cite.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::attention::dto::{
    AttentionDelivery, AttentionLot, AttentionMoney, AttentionOrder, AttentionShift,
    AttentionStockItem, Mark, NewMark,
};

/// Order counts: draft (`OrderAction::Confirm`), open (`Pick`/`Complete`), awaiting payment.
pub struct OrderCounts {
    pub draft: i64,
    pub open: i64,
    pub awaiting_payment: i64,
}

/// Counts of draft, open and awaiting-payment orders.
pub async fn order_counts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<OrderCounts, sqlx::Error> {
    sqlx::query_as!(
        OrderCounts,
        r#"SELECT count(*) FILTER (WHERE status = 'draft') AS "draft!",
                  count(*) FILTER (WHERE status = 'open') AS "open!",
                  count(*) FILTER (WHERE status = 'awaiting_payment') AS "awaiting_payment!"
           FROM orders
           WHERE workspace_id = $1 AND status IN ('draft', 'open', 'awaiting_payment')"#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Draft and open orders, oldest first.
pub async fn order_items(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    limit: i64,
) -> Result<Vec<AttentionOrder>, sqlx::Error> {
    sqlx::query_as!(
        AttentionOrder,
        r#"SELECT o.id, o.number, o.status, o.created_at, o.total,
                  upper(c.asset_type) AS "currency!", c.decimals
           FROM orders o
           JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
           WHERE o.workspace_id = $1 AND o.status IN ('draft', 'open')
           ORDER BY o.created_at, o.id
           LIMIT $2"#,
        workspace_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Local midnight starting today in `general.timezone`; UTC when unset or unknown to Postgres.
// shortcut: same rule as voice-filters' settings::repo::day_start; use that once both land.
pub async fn today_start(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"WITH tz AS (
               SELECT coalesce((SELECT z.name FROM pg_timezone_names z
                                WHERE z.name = (SELECT s.data -> 'general' ->> 'timezone'
                                                FROM settings s WHERE s.workspace_id = $1)),
                               'UTC') AS name)
           SELECT ((now() AT TIME ZONE tz.name)::date::timestamp AT TIME ZONE tz.name) AS "at!"
           FROM tz"#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Invoice counts: draft (`Send`), unpaid (`Pay`: `payable` and not credit), overdue.
pub struct InvoiceCounts {
    pub draft: i64,
    pub unpaid: i64,
    pub overdue: i64,
}

/// Counts of draft, unpaid and overdue (due before `today`) live invoices.
pub async fn invoice_counts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    payable: &[String],
    today: DateTime<Utc>,
) -> Result<InvoiceCounts, sqlx::Error> {
    sqlx::query_as!(
        InvoiceCounts,
        r#"SELECT count(*) FILTER (WHERE status = 'draft') AS "draft!",
                  count(*) FILTER (WHERE status = ANY($2) AND NOT is_credit) AS "unpaid!",
                  count(*) FILTER (WHERE status = ANY($2) AND NOT is_credit
                                     AND due_date < $3) AS "overdue!"
           FROM invoices
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND (status = 'draft' OR status = ANY($2))"#,
        workspace_id,
        payable,
        today,
    )
    .fetch_one(db)
    .await
}

/// Open amount (total − paid) of unpaid invoices, summed per currency.
pub async fn invoice_open(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    payable: &[String],
) -> Result<Vec<AttentionMoney>, sqlx::Error> {
    sqlx::query_as!(
        AttentionMoney,
        r#"SELECT upper(c.asset_type) AS "currency!", c.decimals,
                  sum(i.total - i.paid_total)::bigint AS "amount!"
           FROM invoices i
           JOIN currencies c ON c.workspace_id = i.workspace_id AND c.id = i.currency_id
           WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND NOT i.is_credit
             AND i.status = ANY($2)
           GROUP BY c.asset_type, c.decimals
           ORDER BY 1"#,
        workspace_id,
        payable,
    )
    .fetch_all(db)
    .await
}

/// An unpaid invoice row (number still raw).
pub struct InvoiceRow {
    pub id: Uuid,
    pub invoice_number: Option<i64>,
    pub invoice_appendix: Option<serde_json::Value>,
    pub status: String,
    pub customer_name: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub overdue: bool,
    pub open: i64,
    pub currency: String,
    pub decimals: i16,
}

/// Unpaid invoices, earliest due date first (none last). Customer name as quick search.
pub async fn invoice_items(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    payable: &[String],
    today: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<InvoiceRow>, sqlx::Error> {
    sqlx::query_as!(
        InvoiceRow,
        r#"SELECT i.id, i.invoice_number, i.invoice_appendix, i.status,
                  coalesce(nullif(r.company_name, ''), nullif(r.alias, ''),
                      (SELECT nullif(concat_ws(' ', c.first_name, c.middle_name, c.last_name), '')
                       FROM contacts c
                       WHERE c.workspace_id = r.workspace_id AND c.relation_id = r.id
                         AND c.deleted_at IS NULL
                       ORDER BY c.created_at, c.id LIMIT 1)) AS customer_name,
                  i.due_date, coalesce(i.due_date < $3, false) AS "overdue!",
                  i.total - i.paid_total AS "open!",
                  upper(cu.asset_type) AS "currency!", cu.decimals
           FROM invoices i
           JOIN currencies cu ON cu.workspace_id = i.workspace_id AND cu.id = i.currency_id
           LEFT JOIN relations r ON r.workspace_id = i.workspace_id AND r.id = i.relation_id
           WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND NOT i.is_credit
             AND i.status = ANY($2)
           ORDER BY i.due_date NULLS LAST, i.created_at, i.id
           LIMIT $4"#,
        workspace_id,
        payable,
        today,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Store rows below zero, as `stock::repo::list` with `negative` counts them.
pub async fn negative_stock(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!"
           FROM stocks s
           JOIN products p ON p.id = s.product_id AND p.deleted_at IS NULL
           JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
           WHERE s.workspace_id = $1 AND s.deleted_at IS NULL AND s.stock_lvl < 0 AND w.is_store"#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Live stock-tracked products whose sellable available is at most `at_most` (0: the storefront's
/// `available_stock` sum and `sold_out`), lowest first, with the total on each row.
// shortcut: aggregates every tracked product of the workspace per call; fine at catalog size,
// keep a per-product availability column if catalogs reach tens of thousands.
pub async fn sold_out(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    at_most: i64,
    limit: i64,
) -> Result<(Vec<AttentionStockItem>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"WITH a AS (
               SELECT p.id, p.name,
                      coalesce((SELECT sum(s.stock_lvl - s.reserved)
                                FROM stocks s
                                LEFT JOIN lots l ON l.id = s.lot_id
                                WHERE s.workspace_id = $1 AND s.product_id = p.id
                                  AND s.deleted_at IS NULL
                                  AND (s.lot_id IS NULL OR (l.status = 'released'
                                       AND (l.expires_on IS NULL
                                            OR l.expires_on >= current_date)))),
                               0) AS available
               FROM products p
               WHERE p.workspace_id = $1 AND p.deleted_at IS NULL AND p.has_stock)
           SELECT id AS "product_id!", name AS "product_name!",
                  available::numeric(19, 2)::text AS "available!",
                  count(*) OVER () AS "total!"
           FROM a
           WHERE available <= $3::bigint
           ORDER BY available, name, id
           LIMIT $2"#,
        workspace_id,
        limit,
        at_most,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let items = rows
        .into_iter()
        .map(|r| AttentionStockItem {
            product_id: r.product_id,
            product_name: r.product_name,
            available: r.available,
        })
        .collect();
    Ok((items, total))
}

/// Open till shifts (no `closed_at`), oldest first, with the total on each row.
pub async fn open_shifts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    limit: i64,
) -> Result<(Vec<AttentionShift>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT d.id, d.register_id, r.name AS register_name, u.name AS opened_by,
                  d.opened_at, count(*) OVER () AS "total!"
           FROM drawer_shifts d
           JOIN till_registers r ON r.workspace_id = d.workspace_id AND r.id = d.register_id
           JOIN till_users u ON u.workspace_id = d.workspace_id AND u.id = d.opened_by
           WHERE d.workspace_id = $1 AND d.closed_at IS NULL
           ORDER BY d.opened_at, d.id
           LIMIT $2"#,
        workspace_id,
        limit,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let items = rows
        .into_iter()
        .map(|r| AttentionShift {
            id: r.id,
            register_id: r.register_id,
            register_name: r.register_name,
            opened_by: r.opened_by,
            opened_at: r.opened_at,
        })
        .collect();
    Ok((items, total))
}

/// Lot counts: quarantined lots, and orders holding stock on a lot off sale (the
/// `lots::repo::holds` join, over every lot not released).
pub struct LotCounts {
    pub quarantined: i64,
    pub held_orders: i64,
}

/// Counts of quarantined lots and of orders held on lots off sale.
pub async fn lot_counts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<LotCounts, sqlx::Error> {
    sqlx::query_as!(
        LotCounts,
        r#"SELECT (SELECT count(*) FROM lots
                   WHERE workspace_id = $1 AND status = 'quarantine') AS "quarantined!",
                  (SELECT count(DISTINCT r.related_id)
                   FROM stock_reservations r
                   JOIN stocks s ON s.workspace_id = r.workspace_id AND s.id = r.stock_id
                   JOIN lots l ON l.workspace_id = s.workspace_id AND l.id = s.lot_id
                   WHERE r.workspace_id = $1 AND r.related_domain = 'order'
                     AND l.status <> 'released') AS "held_orders!""#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Quarantined lots, longest in quarantine (oldest change) first, with their held orders.
pub async fn lot_items(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    limit: i64,
) -> Result<Vec<AttentionLot>, sqlx::Error> {
    sqlx::query_as!(
        AttentionLot,
        r#"SELECT l.id, l.code, l.product_id, p.name AS product_name,
                  (SELECT count(DISTINCT r.related_id)
                   FROM stock_reservations r
                   JOIN stocks s ON s.workspace_id = r.workspace_id AND s.id = r.stock_id
                   WHERE r.workspace_id = $1 AND s.lot_id = l.id
                     AND r.related_domain = 'order') AS "held_orders!"
           FROM lots l
           JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
           WHERE l.workspace_id = $1 AND l.status = 'quarantine'
           ORDER BY l.updated_at, l.id
           LIMIT $2"#,
        workspace_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Failed webhook deliveries (dead letters), oldest first, with the total on each row.
pub async fn failed_deliveries(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    limit: i64,
) -> Result<(Vec<AttentionDelivery>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT d.id, d.endpoint_id, e.type AS event_type, d.attempts, d.last_error,
                  d.updated_at, count(*) OVER () AS "total!"
           FROM webhook_deliveries d
           JOIN webhook_events e ON e.workspace_id = d.workspace_id AND e.id = d.event_id
           WHERE d.workspace_id = $1 AND d.status = 'failed'
           ORDER BY d.updated_at, d.id
           LIMIT $2"#,
        workspace_id,
        limit,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let items = rows
        .into_iter()
        .map(|r| AttentionDelivery {
            id: r.id,
            endpoint_id: r.endpoint_id,
            event_type: r.event_type,
            attempts: r.attempts,
            last_error: r.last_error,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((items, total))
}

/// A below-minimum product with the caller's mark and the group totals (window counts, so the
/// headline and the items come from one statement).
pub struct StockRow {
    pub ref_id: Uuid,
    pub label: String,
    pub available: String,
    pub minimum: String,
    pub shortfall: String,
    pub open_orders: Option<i64>,
    pub acked: bool,
    pub snoozed: bool,
    pub total: i64,
    pub acked_n: i64,
    pub snoozed_n: i64,
    pub impact: Option<i64>,
}

/// Products whose sellable available (the `sold_out` rule) is below their minimum: Σ row
/// `minimal_stock_level` when above 0, else `threshold`. `user`'s marks joined (nil = none);
/// snoozed rows last; then most open orders, largest shortfall, name. `with_orders` false
/// leaves `open_orders` and `impact` null.
// shortcut: aggregates every tracked product like `sold_out`; same upgrade path.
pub async fn below_minimum(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    threshold: i64,
    with_orders: bool,
    limit: i64,
) -> Result<Vec<StockRow>, sqlx::Error> {
    sqlx::query_as!(
        StockRow,
        r#"WITH a AS (
               SELECT p.id, p.name,
                      coalesce((SELECT sum(s.stock_lvl - s.reserved)
                                FROM stocks s
                                LEFT JOIN lots l ON l.id = s.lot_id
                                WHERE s.workspace_id = $1 AND s.product_id = p.id
                                  AND s.deleted_at IS NULL
                                  AND (s.lot_id IS NULL OR (l.status = 'released'
                                       AND (l.expires_on IS NULL
                                            OR l.expires_on >= current_date)))),
                               0) AS available,
                      coalesce((SELECT sum(s.minimal_stock_level) FROM stocks s
                                WHERE s.workspace_id = $1 AND s.product_id = p.id
                                  AND s.deleted_at IS NULL), 0) AS row_min
               FROM products p
               WHERE p.workspace_id = $1 AND p.deleted_at IS NULL AND p.has_stock),
           d AS (
               SELECT a.id, a.name, a.available, b.minimum,
                      b.minimum - a.available AS shortfall,
                      CASE WHEN $4 THEN (
                          SELECT count(DISTINCT ol.order_id)
                          FROM order_lines ol
                          JOIN orders o ON o.workspace_id = ol.workspace_id AND o.id = ol.order_id
                          WHERE ol.workspace_id = $1 AND ol.product_id = a.id
                            AND o.status IN ('draft', 'open')) END AS open_orders,
                      coalesce(mk.state = 'acknowledged', false) AS acked,
                      coalesce(mk.state = 'snoozed' AND mk.snoozed_until > now(), false) AS snoozed
               FROM a
               CROSS JOIN LATERAL (SELECT (CASE WHEN a.row_min > 0 THEN a.row_min
                                                ELSE $3::bigint END)::numeric AS minimum) b
               LEFT JOIN attention_marks mk
                      ON mk.workspace_id = $1 AND mk.user_id = $2
                     AND mk.kind = 'stock.below_minimum' AND mk.ref_id = a.id
               WHERE a.available < b.minimum)
           SELECT d.id AS "ref_id!", d.name AS "label!",
                  d.available::numeric(19, 2)::text AS "available!",
                  d.minimum::numeric(19, 2)::text AS "minimum!",
                  d.shortfall::numeric(19, 2)::text AS "shortfall!",
                  d.open_orders, d.acked AS "acked!", d.snoozed AS "snoozed!",
                  count(*) OVER () AS "total!",
                  count(*) FILTER (WHERE d.acked) OVER () AS "acked_n!",
                  count(*) FILTER (WHERE d.snoozed) OVER () AS "snoozed_n!",
                  CASE WHEN $4 THEN (
                      SELECT count(DISTINCT ol.order_id)
                      FROM order_lines ol
                      JOIN orders o ON o.workspace_id = ol.workspace_id AND o.id = ol.order_id
                      WHERE ol.workspace_id = $1 AND o.status IN ('draft', 'open')
                        AND ol.product_id IN (SELECT id FROM d WHERE NOT d.snoozed)) END AS impact
           FROM d
           ORDER BY d.snoozed, d.open_orders DESC NULLS LAST, d.shortfall DESC, d.name, d.id
           LIMIT $5"#,
        workspace_id,
        user_id,
        threshold,
        with_orders,
        limit,
    )
    .fetch_all(db)
    .await
}

/// An alert item of a non-stock kind with the caller's mark and the group totals.
pub struct ItemRow {
    pub ref_id: Uuid,
    pub label: String,
    pub invoice_number: Option<i64>,
    pub invoice_appendix: Option<serde_json::Value>,
    pub amount: Option<i64>,
    pub currency: Option<String>,
    pub decimals: Option<i16>,
    pub since: Option<DateTime<Utc>>,
    pub acked: bool,
    pub snoozed: bool,
    pub total: i64,
    pub acked_n: i64,
    pub snoozed_n: i64,
}

/// Draft and open orders, oldest first (snoozed last), with `user`'s marks.
pub async fn open_order_items(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ItemRow>, sqlx::Error> {
    sqlx::query_as!(
        ItemRow,
        r#"WITH d AS (
               SELECT o.id, o.number, o.total, upper(c.asset_type) AS currency, c.decimals,
                      o.created_at,
                      coalesce(mk.state = 'acknowledged', false) AS acked,
                      coalesce(mk.state = 'snoozed' AND mk.snoozed_until > now(), false) AS snoozed
               FROM orders o
               JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
               LEFT JOIN attention_marks mk
                      ON mk.workspace_id = $1 AND mk.user_id = $2
                     AND mk.kind = 'orders.open' AND mk.ref_id = o.id
               WHERE o.workspace_id = $1 AND o.status IN ('draft', 'open'))
           SELECT d.id AS "ref_id!", d.number AS "label!", NULL::bigint AS invoice_number,
                  NULL::jsonb AS invoice_appendix, d.total AS amount, d.currency AS "currency?",
                  d.decimals AS "decimals?", d.created_at AS since,
                  d.acked AS "acked!", d.snoozed AS "snoozed!",
                  count(*) OVER () AS "total!",
                  count(*) FILTER (WHERE d.acked) OVER () AS "acked_n!",
                  count(*) FILTER (WHERE d.snoozed) OVER () AS "snoozed_n!"
           FROM d
           ORDER BY d.snoozed, d.created_at, d.id
           LIMIT $3"#,
        workspace_id,
        user_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Overdue invoices (unpaid, due before `today`), earliest due first (snoozed last).
pub async fn overdue_invoice_items(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    payable: &[String],
    today: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<ItemRow>, sqlx::Error> {
    sqlx::query_as!(
        ItemRow,
        r#"WITH d AS (
               SELECT i.id, i.invoice_number, i.invoice_appendix, i.due_date, i.created_at,
                      i.total - i.paid_total AS open,
                      upper(cu.asset_type) AS currency, cu.decimals,
                      coalesce(nullif(r.company_name, ''), nullif(r.alias, ''),
                          (SELECT nullif(concat_ws(' ', c.first_name, c.middle_name, c.last_name), '')
                           FROM contacts c
                           WHERE c.workspace_id = r.workspace_id AND c.relation_id = r.id
                             AND c.deleted_at IS NULL
                           ORDER BY c.created_at, c.id LIMIT 1)) AS customer,
                      coalesce(mk.state = 'acknowledged', false) AS acked,
                      coalesce(mk.state = 'snoozed' AND mk.snoozed_until > now(), false) AS snoozed
               FROM invoices i
               JOIN currencies cu ON cu.workspace_id = i.workspace_id AND cu.id = i.currency_id
               LEFT JOIN relations r ON r.workspace_id = i.workspace_id AND r.id = i.relation_id
               LEFT JOIN attention_marks mk
                      ON mk.workspace_id = $1 AND mk.user_id = $2
                     AND mk.kind = 'invoices.overdue' AND mk.ref_id = i.id
               WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND NOT i.is_credit
                 AND i.status = ANY($3) AND i.due_date < $4)
           SELECT d.id AS "ref_id!", coalesce(d.customer, 'Unnamed customer') AS "label!",
                  d.invoice_number,
                  d.invoice_appendix, d.open AS amount, d.currency AS "currency?",
                  d.decimals AS "decimals?", d.due_date AS since,
                  d.acked AS "acked!", d.snoozed AS "snoozed!",
                  count(*) OVER () AS "total!",
                  count(*) FILTER (WHERE d.acked) OVER () AS "acked_n!",
                  count(*) FILTER (WHERE d.snoozed) OVER () AS "snoozed_n!"
           FROM d
           ORDER BY d.snoozed, d.due_date, d.created_at, d.id
           LIMIT $5"#,
        workspace_id,
        user_id,
        payable,
        today,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Open till shifts, oldest first (snoozed last); label = register name.
pub async fn open_shift_items(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ItemRow>, sqlx::Error> {
    sqlx::query_as!(
        ItemRow,
        r#"WITH d AS (
               SELECT s.id, r.name, s.opened_at,
                      coalesce(mk.state = 'acknowledged', false) AS acked,
                      coalesce(mk.state = 'snoozed' AND mk.snoozed_until > now(), false) AS snoozed
               FROM drawer_shifts s
               JOIN till_registers r ON r.workspace_id = s.workspace_id AND r.id = s.register_id
               LEFT JOIN attention_marks mk
                      ON mk.workspace_id = $1 AND mk.user_id = $2
                     AND mk.kind = 'till.open_shifts' AND mk.ref_id = s.id
               WHERE s.workspace_id = $1 AND s.closed_at IS NULL)
           SELECT d.id AS "ref_id!", d.name AS "label!", NULL::bigint AS invoice_number,
                  NULL::jsonb AS invoice_appendix, NULL::bigint AS amount,
                  NULL::text AS currency, NULL::smallint AS decimals, d.opened_at AS since,
                  d.acked AS "acked!", d.snoozed AS "snoozed!",
                  count(*) OVER () AS "total!",
                  count(*) FILTER (WHERE d.acked) OVER () AS "acked_n!",
                  count(*) FILTER (WHERE d.snoozed) OVER () AS "snoozed_n!"
           FROM d
           ORDER BY d.snoozed, d.opened_at, d.id
           LIMIT $3"#,
        workspace_id,
        user_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Quarantined lots, longest in quarantine first (snoozed last); label = lot code.
pub async fn quarantined_lot_items(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ItemRow>, sqlx::Error> {
    sqlx::query_as!(
        ItemRow,
        r#"WITH d AS (
               SELECT l.id, l.code, l.updated_at,
                      coalesce(mk.state = 'acknowledged', false) AS acked,
                      coalesce(mk.state = 'snoozed' AND mk.snoozed_until > now(), false) AS snoozed
               FROM lots l
               LEFT JOIN attention_marks mk
                      ON mk.workspace_id = $1 AND mk.user_id = $2
                     AND mk.kind = 'lots.quarantined' AND mk.ref_id = l.id
               WHERE l.workspace_id = $1 AND l.status = 'quarantine')
           SELECT d.id AS "ref_id!", d.code AS "label!", NULL::bigint AS invoice_number,
                  NULL::jsonb AS invoice_appendix, NULL::bigint AS amount,
                  NULL::text AS currency, NULL::smallint AS decimals, d.updated_at AS since,
                  d.acked AS "acked!", d.snoozed AS "snoozed!",
                  count(*) OVER () AS "total!",
                  count(*) FILTER (WHERE d.acked) OVER () AS "acked_n!",
                  count(*) FILTER (WHERE d.snoozed) OVER () AS "snoozed_n!"
           FROM d
           ORDER BY d.snoozed, d.updated_at, d.id
           LIMIT $3"#,
        workspace_id,
        user_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Failed webhook deliveries, oldest first (snoozed last); label = event type.
pub async fn failed_delivery_items(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ItemRow>, sqlx::Error> {
    sqlx::query_as!(
        ItemRow,
        r#"WITH d AS (
               SELECT w.id, e.type, w.updated_at,
                      coalesce(mk.state = 'acknowledged', false) AS acked,
                      coalesce(mk.state = 'snoozed' AND mk.snoozed_until > now(), false) AS snoozed
               FROM webhook_deliveries w
               JOIN webhook_events e ON e.workspace_id = w.workspace_id AND e.id = w.event_id
               LEFT JOIN attention_marks mk
                      ON mk.workspace_id = $1 AND mk.user_id = $2
                     AND mk.kind = 'webhooks.failed' AND mk.ref_id = w.id
               WHERE w.workspace_id = $1 AND w.status = 'failed')
           SELECT d.id AS "ref_id!", d.type AS "label!", NULL::bigint AS invoice_number,
                  NULL::jsonb AS invoice_appendix, NULL::bigint AS amount,
                  NULL::text AS currency, NULL::smallint AS decimals, d.updated_at AS since,
                  d.acked AS "acked!", d.snoozed AS "snoozed!",
                  count(*) OVER () AS "total!",
                  count(*) FILTER (WHERE d.acked) OVER () AS "acked_n!",
                  count(*) FILTER (WHERE d.snoozed) OVER () AS "snoozed_n!"
           FROM d
           ORDER BY d.snoozed, d.updated_at, d.id
           LIMIT $3"#,
        workspace_id,
        user_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Does `ref_id` name a live record of `kind`'s type in the workspace (`attention_ref_exists`)?
pub async fn ref_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    kind: &str,
    ref_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT attention_ref_exists($1, $2, $3) AS "ok!""#,
        workspace_id,
        kind,
        ref_id,
    )
    .fetch_one(db)
    .await
}

/// Insert or replace `user_id`'s mark; a snooze ends at the next local midnight in
/// `general.timezone` (UTC fallback) for `until_tomorrow`, else 7 days from now.
pub async fn set_mark(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    m: &NewMark,
) -> Result<Mark, sqlx::Error> {
    sqlx::query_as!(
        Mark,
        r#"WITH z AS (
               SELECT coalesce((SELECT n.name FROM pg_timezone_names n
                                WHERE n.name = (SELECT s.data #>> '{general,timezone}'
                                                FROM settings s WHERE s.workspace_id = $1)),
                               'UTC') AS tz)
           INSERT INTO attention_marks (workspace_id, user_id, kind, ref_id, state, snoozed_until)
           SELECT $1, $2, $3, $4, $5,
                  CASE $6::text
                      WHEN 'until_tomorrow'
                          THEN ((now() AT TIME ZONE z.tz)::date + 1)::timestamp AT TIME ZONE z.tz
                      WHEN 'seven_days' THEN now() + interval '7 days'
                  END
           FROM z
           ON CONFLICT (workspace_id, user_id, kind, ref_id) DO UPDATE
               SET state = EXCLUDED.state, snoozed_until = EXCLUDED.snoozed_until,
                   created_at = now()
           RETURNING id, kind, ref_id, state, snoozed_until, created_at"#,
        workspace_id,
        user_id,
        m.kind,
        m.ref_id,
        m.state,
        m.snooze,
    )
    .fetch_one(db)
    .await
}

/// Delete `user_id`'s mark `id`; rows removed (0 = not theirs or gone).
pub async fn delete_mark(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "DELETE FROM attention_marks WHERE workspace_id = $1 AND user_id = $2 AND id = $3",
        workspace_id,
        user_id,
        id,
    )
    .execute(db)
    .await?
    .rows_affected())
}

/// Drop snoozes over a day past their end, acknowledgements over 30 days old and marks whose
/// record is gone. Rows removed.
pub async fn prune_marks(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "DELETE FROM attention_marks m
         WHERE (m.state = 'snoozed' AND m.snoozed_until < now() - interval '1 day')
            OR (m.state = 'acknowledged' AND m.created_at < now() - interval '30 days')
            OR NOT attention_ref_exists(m.workspace_id, m.kind, m.ref_id)"
    )
    .execute(db)
    .await?
    .rows_affected())
}
