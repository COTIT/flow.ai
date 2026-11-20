//! Item line maths (Laravel HasItemPricing + BuildItemsSummary). Each line's amounts round to
//! the currency's decimals; VAT is computed once per rate on the summed line amounts (user
//! decision 2026-10-21), so total = excl + VAT always. All maths runs in Postgres
//! (`price_lines`, `price_margin`) through [`price`] / [`margin_of`]; this file holds the
//! shapes. `tests/fixtures/pricing_golden.json` freezes the former Rust engine's results.

use rust_decimal::Decimal;
use sqlx::PgConnection;

use super::repo;

/// One validated item line.
#[derive(Debug, Clone, Copy, Default)]
pub struct Line {
    pub quantity: Decimal,
    /// Unit price excl VAT.
    pub price: Decimal,
    /// VAT percent, e.g. 21.
    pub vat: Decimal,
    /// Discount percent, 0..=100.
    pub discount: Decimal,
    pub purchase_price: Option<Decimal>,
    /// Child line of a multi-product set: margin skips it (Laravel).
    pub set_child: bool,
}

/// One line's rounded amounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Amounts {
    /// quantity × price.
    pub gross: Decimal,
    pub discount: Decimal,
    /// gross − discount.
    pub excl: Decimal,
    /// This line's own VAT, rounded on its own: for per-line display only. Document VAT is
    /// per rate (`Summary::vats`); the sum of these can differ from it by half a cent per line.
    pub line_vat: Decimal,
}

/// VAT of one rate: `round(base × rate / 100)` over the summed line amounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateTotal {
    pub rate: Decimal,
    /// Σ line excl at this rate.
    pub base: Decimal,
    pub vat: Decimal,
}

/// Totals over all lines, VAT grouped by exact rate in first-seen order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub gross: Decimal,
    pub discount: Decimal,
    pub excl: Decimal,
    /// Σ VAT per rate.
    pub vat: Decimal,
    /// excl + vat.
    pub total: Decimal,
    /// One entry per distinct rate, first-seen order.
    pub vats: Vec<RateTotal>,
    /// Per line, same order as the input.
    pub lines: Vec<Amounts>,
}

/// Margin of the item lines (Laravel BuildItemsMargeCalculationSummary, `items` part).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Margin {
    /// Σ purchase price × quantity, each rounded.
    pub cost: Decimal,
    /// Σ line excl.
    pub revenue: Decimal,
    /// revenue − cost.
    pub amount: Decimal,
    /// amount / revenue × 100 as a float (only ever shown as a JSON number). Laravel divides
    /// by 1 when revenue is 0.
    pub percentage: f64,
}

/// A document priced in Postgres: summary, and its margin or the line it overflowed on.
#[derive(Debug, Clone)]
pub struct Priced {
    pub summary: Summary,
    pub margin: Result<Margin, usize>,
}

fn need(v: Option<Decimal>) -> Result<Decimal, sqlx::Error> {
    v.ok_or_else(|| sqlx::Error::Decode("price_lines: amount missing without overflow".into()))
}

fn margin_row(
    cost: Option<Decimal>,
    revenue: Option<Decimal>,
    amount: Option<Decimal>,
    percentage: Option<f64>,
    over: Option<i32>,
) -> Result<Result<Margin, usize>, sqlx::Error> {
    if let Some(i) = over {
        return Ok(Err(i as usize));
    }
    let percentage =
        percentage.ok_or_else(|| sqlx::Error::Decode("price_margin: percentage missing".into()))?;
    Ok(Ok(Margin {
        cost: need(cost)?,
        revenue: need(revenue)?,
        amount: need(amount)?,
        percentage,
    }))
}

/// Price `lines` at `dp` in one query (`price_lines`). Ok(Err(i)): line i (the last line for
/// a rate's VAT or the total) left Decimal's range.
pub async fn price(
    conn: &mut PgConnection,
    lines: &[Line],
    dp: u32,
) -> Result<Result<Priced, usize>, sqlx::Error> {
    let rows = repo::price_lines(&mut *conn, dp, lines).await?;
    let Some(first) = rows.first() else {
        return Ok(Ok(Priced {
            summary: Summary::default(),
            margin: Ok(Margin::default()),
        }));
    };
    if let Some(i) = first.overflow_at {
        return Ok(Err(i as usize));
    }
    let mut s = Summary {
        gross: need(first.gross_total)?,
        discount: need(first.discount_total)?,
        excl: need(first.excl_total)?,
        vat: need(first.vat_total)?,
        total: need(first.total)?,
        ..Summary::default()
    };
    for r in &rows {
        s.lines.push(Amounts {
            gross: need(r.gross)?,
            discount: need(r.discount)?,
            excl: need(r.excl)?,
            line_vat: need(r.line_vat)?,
        });
        // Rows come in line order, so rates come in first-seen order.
        if r.n == r.rate_first {
            s.vats.push(RateTotal {
                rate: r.rate,
                base: need(r.rate_base)?,
                vat: need(r.rate_vat)?,
            });
        }
    }
    let margin = margin_row(
        first.cost,
        first.revenue,
        first.margin_amount,
        first.margin_percentage,
        first.margin_overflow_at,
    )?;
    Ok(Ok(Priced { summary: s, margin }))
}

/// Margin of `lines` whose revenue per line is `revenues` (priced elsewhere, e.g. copied from
/// recognitions), in Postgres (`price_margin`). Ok(Err(i)) as [`price`]'s margin.
pub async fn margin_of(
    conn: &mut PgConnection,
    lines: &[Line],
    revenues: &[Decimal],
    dp: u32,
) -> Result<Result<Margin, usize>, sqlx::Error> {
    let r = repo::price_margin(&mut *conn, dp, lines, revenues).await?;
    margin_row(
        r.cost,
        r.revenue,
        r.margin_amount,
        r.margin_percentage,
        r.margin_overflow_at,
    )
}
