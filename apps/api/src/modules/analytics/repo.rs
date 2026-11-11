//! SQL for sales analytics and the invoice export. "Sold" = the `orders::repo::sales_summary`
//! rule: status open, completed, partially_returned or returned, at confirmed_at, else paid_at,
//! completed_at, created_at. Order net = `subtotal`, VAT = stored `vat` (pricing engine writes
//! total = subtotal + vat), so gross = net + VAT. Compare reads VAT as `total - subtotal`.
// shortcut: each query scans the workspace's orders in range like sales_summary (no index on
// the sold-at expression); add one when a workspace passes ~100k orders.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Workspace time zone (`UTC` when unset or not a zone Postgres knows) and the local midnight
/// starting each of `days`, in order.
pub async fn midnights(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    days: &[NaiveDate],
) -> Result<(String, Vec<DateTime<Utc>>), sqlx::Error> {
    let r = sqlx::query!(
        r#"WITH z AS (
               SELECT coalesce((SELECT n.name FROM pg_timezone_names n
                                WHERE n.name = (SELECT s.data #>> '{general,timezone}'
                                                FROM settings s WHERE s.workspace_id = $1)),
                               'UTC') AS tz)
           SELECT z.tz AS "tz!",
                  array(SELECT u.d::timestamp AT TIME ZONE z.tz
                        FROM unnest($2::date[]) WITH ORDINALITY AS u(d, i) ORDER BY u.i)
                      AS "at!: Vec<DateTime<Utc>>"
           FROM z"#,
        workspace_id,
        days,
    )
    .fetch_one(db)
    .await?;
    Ok((r.tz, r.at))
}

/// A customer total, or the walk-in total when `relation_id` is None.
pub struct CustomerTotal {
    pub currency: String,
    pub decimals: i16,
    pub relation_id: Option<Uuid>,
    pub name: String,
    pub orders: i64,
    pub gross_total: i64,
    pub net_total: i64,
    pub vat_total: i64,
}

/// Top `limit` customers per currency by gross total (then name, id), plus one walk-in row
/// per currency. Ordered by currency, walk-in last, rank. With `product_id`/`variant_id` only
/// those lines count (`line_excl`, `line_vat`) and only orders holding them.
#[allow(clippy::too_many_arguments)]
pub async fn top_customers(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    source: Option<&str>,
    limit: i64,
    product_id: Option<Uuid>,
    variant_id: Option<Uuid>,
) -> Result<Vec<CustomerTotal>, sqlx::Error> {
    sqlx::query_as!(
        CustomerTotal,
        r#"WITH g AS (
               SELECT upper(c.asset_type) AS currency, c.decimals, o.relation_id,
                      count(*) AS orders,
                      sum(coalesce(l.net, o.subtotal))::bigint AS net,
                      sum(coalesce(l.vat, o.vat))::bigint AS vat
               FROM orders o
               JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
               -- Filtered: that product's lines of the order; no filter: no row, order totals.
               LEFT JOIN LATERAL (
                   SELECT sum(ol.line_excl) AS net, sum(ol.line_vat) AS vat
                   FROM order_lines ol
                   WHERE ol.workspace_id = o.workspace_id AND ol.order_id = o.id
                     AND ($6::uuid IS NULL OR ol.product_id = $6)
                     AND ($7::uuid IS NULL OR ol.product_variant_id = $7)
                   HAVING count(*) > 0) l ON $6::uuid IS NOT NULL OR $7::uuid IS NOT NULL
               WHERE o.workspace_id = $1
                 AND (($6::uuid IS NULL AND $7::uuid IS NULL) OR l.net IS NOT NULL)
                 AND o.status IN ('open', 'completed', 'partially_returned', 'returned')
                 AND coalesce(o.confirmed_at, o.paid_at, o.completed_at, o.created_at) >= $2
                 AND coalesce(o.confirmed_at, o.paid_at, o.completed_at, o.created_at) < $3
                 AND ($4::text IS NULL OR o.source = $4)
               GROUP BY 1, 2, 3),
           r AS (
               SELECT g.*, g.net + g.vat AS gross,
                      coalesce(nullif(rel.company_name, ''), rel.alias, '') AS name,
                      row_number() OVER (
                          PARTITION BY g.currency, g.relation_id IS NULL
                          ORDER BY g.net + g.vat DESC, coalesce(nullif(rel.company_name, ''), rel.alias, ''),
                                   g.relation_id) AS rank
               FROM g
               LEFT JOIN relations rel ON rel.workspace_id = $1 AND rel.id = g.relation_id)
           SELECT r.currency AS "currency!", r.decimals AS "decimals!", r.relation_id,
                  r.name AS "name!", r.orders AS "orders!", r.gross AS "gross_total!",
                  r.net AS "net_total!", r.vat AS "vat_total!"
           FROM r
           WHERE r.relation_id IS NULL OR r.rank <= $5
           ORDER BY r.currency, r.relation_id IS NULL, r.rank"#,
        workspace_id,
        from,
        to,
        source,
        limit,
        product_id,
        variant_id,
    )
    .fetch_all(db)
    .await
}

/// A product (or variant) total.
pub struct ProductTotal {
    pub currency: String,
    pub decimals: i16,
    pub product_id: Uuid,
    pub name: String,
    pub variant_id: Option<Uuid>,
    pub sku: Option<String>,
    pub title: String,
    pub units: String,
    pub lines: i64,
    pub orders: i64,
    pub gross_total: i64,
    pub net_total: i64,
    pub vat_total: i64,
}

/// Top `limit` products per currency by line totals incl VAT; each variant apart when
/// `by_variant`; only `product_id` / `variant_id` lines when set. Ordered by currency, rank.
#[allow(clippy::too_many_arguments)]
pub async fn top_products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    source: Option<&str>,
    limit: i64,
    by_variant: bool,
    product_id: Option<Uuid>,
    variant_id: Option<Uuid>,
) -> Result<Vec<ProductTotal>, sqlx::Error> {
    sqlx::query_as!(
        ProductTotal,
        r#"WITH g AS (
               SELECT upper(c.asset_type) AS currency, c.decimals, l.product_id,
                      CASE WHEN $6 THEN l.product_variant_id END AS variant_id,
                      max(l.title) AS title, sum(l.quantity) AS units, count(*) AS lines,
                      count(DISTINCT o.id) AS orders, sum(l.line_excl + l.line_vat)::bigint AS gross,
                      sum(l.line_excl)::bigint AS net, sum(l.line_vat)::bigint AS vat
               FROM orders o
               JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
               JOIN order_lines l ON l.workspace_id = o.workspace_id AND l.order_id = o.id
               WHERE o.workspace_id = $1
                 AND o.status IN ('open', 'completed', 'partially_returned', 'returned')
                 AND coalesce(o.confirmed_at, o.paid_at, o.completed_at, o.created_at) >= $2
                 AND coalesce(o.confirmed_at, o.paid_at, o.completed_at, o.created_at) < $3
                 AND ($4::text IS NULL OR o.source = $4)
                 AND ($7::uuid IS NULL OR l.product_id = $7)
                 AND ($8::uuid IS NULL OR l.product_variant_id = $8)
               GROUP BY 1, 2, 3, 4),
           r AS (
               SELECT g.*, row_number() OVER (
                          PARTITION BY g.currency
                          ORDER BY g.gross DESC, g.product_id, g.variant_id NULLS FIRST) AS rank
               FROM g)
           SELECT r.currency AS "currency!", r.decimals AS "decimals!", r.product_id AS "product_id!",
                  p.name, r.variant_id, v.sku, r.title AS "title!", r.units::text AS "units!",
                  r.lines AS "lines!", r.orders AS "orders!", r.gross AS "gross_total!",
                  r.net AS "net_total!", r.vat AS "vat_total!"
           FROM r
           JOIN products p ON p.workspace_id = $1 AND p.id = r.product_id
           LEFT JOIN product_variants v ON v.workspace_id = $1 AND v.id = r.variant_id
           WHERE r.rank <= $5
           ORDER BY r.currency, r.rank"#,
        workspace_id,
        from,
        to,
        source,
        limit,
        by_variant,
        product_id,
        variant_id,
    )
    .fetch_all(db)
    .await
}

/// Both periods of one currency, difference and change computed here.
pub struct CompareRow {
    pub currency: String,
    pub decimals: i16,
    pub current_orders: i64,
    pub current_gross: i64,
    pub current_net: i64,
    pub current_refunded: i64,
    pub base_orders: i64,
    pub base_gross: i64,
    pub base_net: i64,
    pub base_refunded: i64,
    pub gross_difference: i64,
    pub orders_difference: i64,
    pub gross_change_bp: Option<i64>,
    pub net_difference: i64,
    pub net_change_bp: Option<i64>,
}

/// Sales of `[from, to)` against `[base_from, base_to)` per currency sold in either. The
/// periods may overlap; an order in both counts in both.
#[allow(clippy::too_many_arguments)]
pub async fn compare(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    base_from: DateTime<Utc>,
    base_to: DateTime<Utc>,
    source: Option<&str>,
) -> Result<Vec<CompareRow>, sqlx::Error> {
    sqlx::query_as!(
        CompareRow,
        r#"WITH s AS (
               SELECT upper(c.asset_type) AS currency, c.decimals, o.total, o.subtotal,
                      coalesce(r.amount, 0) AS refunded,
                      t.at >= $2 AND t.at < $3 AS cur, t.at >= $4 AND t.at < $5 AS base
               FROM orders o
               JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
               CROSS JOIN LATERAL (
                   SELECT coalesce(o.confirmed_at, o.paid_at, o.completed_at, o.created_at) AS at) t
               LEFT JOIN LATERAL (
                   SELECT sum(p.amount) AS amount FROM order_payments p
                   WHERE p.workspace_id = o.workspace_id AND p.order_id = o.id
                     AND p.kind = 'refund' AND p.status = 'refunded') r ON true
               WHERE o.workspace_id = $1
                 AND o.status IN ('open', 'completed', 'partially_returned', 'returned')
                 AND ((t.at >= $2 AND t.at < $3) OR (t.at >= $4 AND t.at < $5))
                 AND ($6::text IS NULL OR o.source = $6)),
           a AS (
               SELECT currency, decimals,
                      count(*) FILTER (WHERE cur) AS co,
                      coalesce(sum(total) FILTER (WHERE cur), 0)::bigint AS cg,
                      coalesce(sum(subtotal) FILTER (WHERE cur), 0)::bigint AS cn,
                      coalesce(sum(refunded) FILTER (WHERE cur), 0)::bigint AS cr,
                      count(*) FILTER (WHERE base) AS bo,
                      coalesce(sum(total) FILTER (WHERE base), 0)::bigint AS bg,
                      coalesce(sum(subtotal) FILTER (WHERE base), 0)::bigint AS bn,
                      coalesce(sum(refunded) FILTER (WHERE base), 0)::bigint AS br
               FROM s GROUP BY 1, 2)
           SELECT currency AS "currency!", decimals AS "decimals!",
                  co AS "current_orders!", cg AS "current_gross!", cn AS "current_net!",
                  cr AS "current_refunded!",
                  bo AS "base_orders!", bg AS "base_gross!", bn AS "base_net!",
                  br AS "base_refunded!",
                  cg - bg AS "gross_difference!", co - bo AS "orders_difference!",
                  CASE WHEN bg <> 0 THEN round((cg - bg) * 10000::numeric / bg)::bigint END
                      AS gross_change_bp,
                  cn - bn AS "net_difference!",
                  CASE WHEN bn <> 0 THEN round((cn - bn) * 10000::numeric / bn)::bigint END
                      AS net_change_bp
           FROM a ORDER BY 1"#,
        workspace_id,
        from,
        to,
        base_from,
        base_to,
        source,
    )
    .fetch_all(db)
    .await
}

/// One exported invoice line with its invoice header; line fields are None for an invoice
/// without lines.
pub struct ExportLine {
    pub invoice_id: Uuid,
    pub number: i64,
    pub appendix: Option<Value>,
    pub invoice_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub is_credit: bool,
    pub status: String,
    pub customer: String,
    pub vat_number: Option<String>,
    pub currency: String,
    pub decimals: i16,
    pub title: Option<String>,
    pub quantity: Option<String>,
    pub unit_price: Option<i64>,
    pub vat_rate: Option<String>,
    pub discount: Option<String>,
    pub line_excl: Option<i64>,
    pub line_vat: Option<i64>,
    pub invoice_excl: i64,
    pub invoice_total: i64,
}

/// Lines of numbered invoices (incl credit notes and cancelled) dated in `[from, to)`, dates
/// read in `tz`, by number then line position; at most `limit` rows. Line cents priced like
/// `invoices::repo::line_cents`.
pub async fn export_lines(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    tz: &str,
    limit: i64,
) -> Result<Vec<ExportLine>, sqlx::Error> {
    sqlx::query_as!(
        ExportLine,
        r#"SELECT i.id AS invoice_id, i.invoice_number AS "number!", i.invoice_appendix AS appendix,
                  (i.send_date AT TIME ZONE $4)::date AS "invoice_date!",
                  (i.due_date AT TIME ZONE $4)::date AS due_date,
                  i.is_credit, i.status,
                  coalesce(nullif(r.company_name, ''), r.alias, '') AS "customer!", r.vat_number,
                  upper(c.asset_type) AS "currency!", c.decimals,
                  l.title AS "title?", trim_scale(l.quantity)::text AS quantity,
                  round(l.price * power(10::numeric, c.decimals))::bigint AS unit_price,
                  trim_scale(l.vat)::text AS vat_rate,
                  trim_scale(l.discount_percentage)::text AS discount,
                  e.excl::bigint AS line_excl, round(e.excl * l.vat / 100)::bigint AS line_vat,
                  i.subtotal AS invoice_excl, i.total AS invoice_total
           FROM invoices i
           JOIN relations r ON r.workspace_id = i.workspace_id AND r.id = i.relation_id
           JOIN currencies c ON c.workspace_id = i.workspace_id AND c.id = i.currency_id
           LEFT JOIN items l ON l.workspace_id = i.workspace_id AND l.invoice_id = i.id
           LEFT JOIN LATERAL (
               SELECT round(l.quantity * l.price * power(10::numeric, c.decimals)) AS gross) g ON true
           LEFT JOIN LATERAL (
               SELECT g.gross - round(g.gross * l.discount_percentage / 100) AS excl) e ON true
           WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.invoice_number IS NOT NULL
             AND i.send_date >= $2 AND i.send_date < $3
           ORDER BY i.invoice_number, l.position, l.id
           LIMIT $5"#,
        workspace_id,
        from,
        to,
        tz,
        limit,
    )
    .fetch_all(db)
    .await
}

/// A day of one currency (currency None: nothing sold in range; the row only carries `tz`).
pub struct DailyRow {
    pub tz: String,
    pub currency: Option<String>,
    pub decimals: Option<i16>,
    pub day: Option<NaiveDate>,
    pub gross: i64,
    pub net: i64,
    pub orders: i64,
}

/// Sold orders per local day (`general.timezone`, UTC fallback) in `[from, to)`, every day for
/// every currency sold in range, ordered by currency then day. One statement.
pub async fn daily(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    source: Option<&str>,
) -> Result<Vec<DailyRow>, sqlx::Error> {
    sqlx::query_as!(
        DailyRow,
        r#"WITH z AS (
               SELECT coalesce((SELECT n.name FROM pg_timezone_names n
                                WHERE n.name = (SELECT s.data #>> '{general,timezone}'
                                                FROM settings s WHERE s.workspace_id = $1)),
                               'UTC') AS tz),
           s AS (
               SELECT upper(c.asset_type) AS currency, c.decimals,
                      (coalesce(o.confirmed_at, o.paid_at, o.completed_at, o.created_at)
                          AT TIME ZONE z.tz)::date AS day,
                      sum(o.subtotal)::bigint AS net, sum(o.vat)::bigint AS vat,
                      count(*) AS orders
               FROM orders o
               CROSS JOIN z
               JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
               WHERE o.workspace_id = $1
                 AND o.status IN ('open', 'completed', 'partially_returned', 'returned')
                 AND coalesce(o.confirmed_at, o.paid_at, o.completed_at, o.created_at)
                     >= $2::date::timestamp AT TIME ZONE z.tz
                 AND coalesce(o.confirmed_at, o.paid_at, o.completed_at, o.created_at)
                     < $3::date::timestamp AT TIME ZONE z.tz
                 AND ($4::text IS NULL OR o.source = $4)
               GROUP BY 1, 2, 3),
           g AS (
               SELECT c.currency, c.decimals, d::date AS day
               FROM (SELECT DISTINCT currency, decimals FROM s) c
               CROSS JOIN generate_series($2::date, $3::date - 1, interval '1 day') d)
           SELECT z.tz AS "tz!", g.currency AS "currency?", g.decimals AS "decimals?",
                  g.day AS "day?",
                  coalesce(s.net + s.vat, 0)::bigint AS "gross!",
                  coalesce(s.net, 0)::bigint AS "net!",
                  coalesce(s.orders, 0) AS "orders!"
           FROM z
           LEFT JOIN (g LEFT JOIN s ON s.currency = g.currency AND s.day = g.day) ON true
           ORDER BY g.currency, g.day"#,
        workspace_id,
        from,
        to,
        source,
    )
    .fetch_all(db)
    .await
}
