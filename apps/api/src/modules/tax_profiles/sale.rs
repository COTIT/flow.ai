//! Active-book sales tax (M6-A): one SQL call (`sale_tax_lines` over `tax_line`) prices every
//! line's tax per component from its covering verified profile and sums the document. Quote
//! and commit both call [`lines`], so they agree; Rust only passes the money through.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::AppError,
    modules::{
        accounting::service::unsupported,
        pricing::engine::{RateTotal, Summary},
    },
};

/// When and for whom a sale is taxed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Facts {
    /// Exemptions by customer.
    pub relation: Option<Uuid>,
    /// Commit time (Till: `sold_at`); its date in the book timezone is the tax point. None = now.
    pub at: Option<DateTime<Utc>>,
    /// Explicit tax point date (standalone invoice date); wins over `at`.
    pub on: Option<NaiveDate>,
}

/// One line's tax: legacy projections (Σ rate, Σ tax) and its components (snapshot JSON).
#[derive(Debug, Clone)]
pub struct Line {
    pub rate: Decimal,
    pub tax: i64,
    /// Line amount + tax.
    pub gross: i64,
    /// `[{line_no, profile_id, code, rate, base, tax, exempt, tax_date}]`.
    pub components: Value,
}

/// A priced document: per line, SQL totals and the per-rate summary.
#[derive(Debug, Clone)]
pub struct Taxed {
    pub lines: Vec<Line>,
    /// Σ line amounts excl tax.
    pub subtotal: i64,
    pub vat: i64,
    pub total: i64,
    /// (rate, base, vat) per distinct projected rate, first-seen order.
    pub vats: Vec<(Decimal, i64, i64)>,
}

/// Tax of `amounts` (line excl, minor units) for `products` (None = free line).
pub async fn lines(
    conn: &mut PgConnection,
    ws: Uuid,
    facts: Facts,
    products: &[Option<Uuid>],
    amounts: &[i64],
) -> Result<Taxed, AppError> {
    let rows = sqlx::query!(
        r#"WITH d AS (
               SELECT coalesce($3::date, (coalesce($2::timestamptz, now())
                                          AT TIME ZONE coalesce(s.timezone, 'UTC'))::date) AS on_date
               FROM accounting_settings s WHERE s.workspace_id = $1),
           c AS (SELECT t.*, d.on_date FROM d, sale_tax_lines($1, d.on_date, $4, $5, $6) t),
           l AS (SELECT c.n, sum(c.rate) AS rate, sum(c.tax)::int8 AS tax,
                        jsonb_agg(jsonb_build_object('line_no', c.line_no, 'profile_id', c.profile_id,
                                  'code', c.code, 'rate', c.rate, 'base', c.base, 'tax', c.tax,
                                  'exempt', c.exempt, 'tax_date', c.on_date)
                                  ORDER BY c.line_no) AS components
                 FROM c GROUP BY c.n),
           a AS (SELECT (o - 1)::int AS n, x FROM unnest($6::int8[]) WITH ORDINALITY u(x, o))
           SELECT l.rate AS "rate!", l.tax AS "tax!", l.components AS "components!",
                  (a.x + l.tax)::int8 AS "gross!",
                  (sum(a.x) OVER (PARTITION BY l.rate))::int8 AS "rate_base!",
                  (sum(l.tax) OVER (PARTITION BY l.rate))::int8 AS "rate_tax!",
                  (sum(a.x) OVER ())::int8 AS "subtotal!",
                  (sum(l.tax) OVER ())::int8 AS "vat!",
                  (sum(a.x) OVER () + sum(l.tax) OVER ())::int8 AS "total!"
           FROM a JOIN l USING (n) ORDER BY a.n"#,
        ws,
        facts.at,
        facts.on,
        facts.relation,
        products as &[Option<Uuid>],
        amounts,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(refused)?;
    if rows.len() != amounts.len() {
        return Err(AppError::Internal("tax profile without components".into()));
    }
    let rows: Vec<Row> = rows
        .into_iter()
        .map(|r| Row {
            rate: r.rate,
            tax: r.tax,
            gross: r.gross,
            components: r.components,
            rate_base: r.rate_base,
            rate_tax: r.rate_tax,
            totals: (r.subtotal, r.vat, r.total),
        })
        .collect();
    Ok(taxed(rows))
}

/// One SQL result row of [`lines`] / [`stored_items`].
struct Row {
    rate: Decimal,
    tax: i64,
    gross: i64,
    components: Value,
    rate_base: i64,
    rate_tax: i64,
    totals: (i64, i64, i64),
}

fn taxed(rows: Vec<Row>) -> Taxed {
    let mut vats: Vec<(Decimal, i64, i64)> = Vec::new();
    for r in &rows {
        if !vats.iter().any(|v| v.0 == r.rate) {
            vats.push((r.rate.normalize(), r.rate_base, r.rate_tax));
        }
    }
    let (subtotal, vat, total) = rows.first().map_or((0, 0, 0), |r| r.totals);
    Taxed {
        lines: rows
            .into_iter()
            .map(|r| Line {
                rate: r.rate.normalize(),
                tax: r.tax,
                gross: r.gross,
                components: r.components,
            })
            .collect(),
        subtotal,
        vat,
        total,
        vats,
    }
}

/// The snapshot of standalone invoice items `ids` (amounts = their line excl, same order)
/// summed like [`lines`]; None when any item has no snapshot (setup book or legacy row).
pub async fn stored_items(
    conn: &mut PgConnection,
    ws: Uuid,
    ids: &[Uuid],
    amounts: &[i64],
) -> Result<Option<Taxed>, AppError> {
    let rows = sqlx::query!(
        r#"WITH l AS (SELECT t.item_id, sum(t.rate) AS rate, sum(t.tax)::int8 AS tax,
                             jsonb_agg(jsonb_build_object('line_no', t.line_no,
                                       'profile_id', t.profile_id, 'code', t.code, 'rate', t.rate,
                                       'base', t.base, 'tax', t.tax, 'exempt', t.exempt,
                                       'tax_date', t.tax_date) ORDER BY t.line_no) AS components
                      FROM item_taxes t WHERE t.workspace_id = $1 AND t.item_id = ANY($2)
                      GROUP BY t.item_id),
                a AS (SELECT (o - 1)::int AS n, id, x
                      FROM unnest($2::uuid[], $3::int8[]) WITH ORDINALITY u(id, x, o))
           SELECT l.rate AS "rate!", l.tax AS "tax!", l.components AS "components!",
                  (a.x + l.tax)::int8 AS "gross!",
                  (sum(a.x) OVER (PARTITION BY l.rate))::int8 AS "rate_base!",
                  (sum(l.tax) OVER (PARTITION BY l.rate))::int8 AS "rate_tax!",
                  (sum(a.x) OVER ())::int8 AS "subtotal!",
                  (sum(l.tax) OVER ())::int8 AS "vat!",
                  (sum(a.x) OVER () + sum(l.tax) OVER ())::int8 AS "total!"
           FROM a JOIN l ON l.item_id = a.id ORDER BY a.n"#,
        ws,
        ids,
        amounts,
    )
    .fetch_all(&mut *conn)
    .await?;
    if ids.is_empty() || rows.len() != ids.len() {
        return Ok(None);
    }
    Ok(Some(taxed(
        rows.into_iter()
            .map(|r| Row {
                rate: r.rate,
                tax: r.tax,
                gross: r.gross,
                components: r.components,
                rate_base: r.rate_base,
                rate_tax: r.rate_tax,
                totals: (r.subtotal, r.vat, r.total),
            })
            .collect(),
    )))
}

/// Put `t`'s tax and totals into engine summary `s` (minor units shifted to `dp` decimals).
pub fn into_summary(s: &mut Summary, t: &Taxed, dp: u32) {
    let d = |m: i64| Decimal::new(m, dp);
    for (a, l) in s.lines.iter_mut().zip(&t.lines) {
        a.line_vat = d(l.tax);
    }
    s.excl = d(t.subtotal);
    s.vat = d(t.vat);
    s.total = d(t.total);
    s.vats = t
        .vats
        .iter()
        .map(|&(rate, base, vat)| RateTotal {
            rate,
            base: d(base),
            vat: d(vat),
        })
        .collect();
}

/// Line excl amounts of `s` in minor units.
pub fn amounts(s: &Summary, dp: u32) -> Result<Vec<i64>, AppError> {
    s.lines
        .iter()
        .map(|a| crate::money::minor(a.excl, dp))
        .collect()
}

/// `sale_tax_lines` refusals as 422 `unsupported_active_erp_operation`.
fn refused(e: sqlx::Error) -> AppError {
    let Some(m) = e
        .as_database_error()
        .filter(|d| d.code().as_deref() == Some("22023"))
        .map(|d| d.message().to_owned())
    else {
        return e.into();
    };
    let line = |key: &str| m.strip_prefix(key).and_then(|n| n.parse::<usize>().ok());
    match (line("tax_profile_missing:"), line("tax_profile_ambiguous:")) {
        (Some(n), _) => unsupported(&format!(
            "No verified tax profile covers line {}. Add or verify a sale tax profile for it.",
            n + 1
        )),
        (_, Some(n)) => unsupported(&format!(
            "More than one tax jurisdiction covers line {}. Selling across jurisdictions is not supported yet.",
            n + 1
        )),
        _ => e.into(),
    }
}

/// Snapshot `components` (one JSON array per row of `ids`, same order) into `order_line_taxes`
/// in one statement.
pub async fn snapshot_order_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    ids: &[Uuid],
    components: &[Value],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO order_line_taxes (workspace_id, order_line_id, line_no, profile_id, code,
                                         rate, base, tax, exempt, tax_date)
           SELECT $1, u.id, c.line_no, c.profile_id, c.code, c.rate, c.base, c.tax, c.exempt,
                  c.tax_date
           FROM unnest($2::uuid[], $3::jsonb[]) u(id, j)
           CROSS JOIN jsonb_to_recordset(u.j) c(line_no smallint, profile_id uuid, code text,
                                                rate numeric, base bigint, tax bigint,
                                                exempt boolean, tax_date date)"#,
        ws,
        ids,
        components,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// Same as [`snapshot_order_lines`] for standalone invoice items, replacing earlier rows.
pub async fn snapshot_items(
    conn: &mut PgConnection,
    ws: Uuid,
    ids: &[Uuid],
    components: &[Value],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM item_taxes WHERE workspace_id = $1 AND item_id = ANY($2)",
        ws,
        ids
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        r#"INSERT INTO item_taxes (workspace_id, item_id, line_no, profile_id, code, rate, base,
                                   tax, exempt, tax_date)
           SELECT $1, u.id, c.line_no, c.profile_id, c.code, c.rate, c.base, c.tax, c.exempt,
                  c.tax_date
           FROM unnest($2::uuid[], $3::jsonb[]) u(id, j)
           CROSS JOIN jsonb_to_recordset(u.j) c(line_no smallint, profile_id uuid, code text,
                                                rate numeric, base bigint, tax bigint,
                                                exempt boolean, tax_date date)"#,
        ws,
        ids,
        components,
    )
    .execute(conn)
    .await
    .map(|_| ())
}
