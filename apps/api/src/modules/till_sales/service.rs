//! Till sale rules. A sale is a completed order (source `till`) whose lines hold what the
//! till charged, priced through the pricing engine; the same lines at catalog prices give
//! `server_total`, and a difference is kept (`price_mismatch`), never refused. Stock is taken
//! at once from the register's store warehouse and never refused (`Floor::None`). Cash rounds
//! to the currency's step and goes into the open drawer shift. One `client_ref` per register
//! is one sale: a replay answers the stored sale, another body under it is 409.

use std::collections::HashMap;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        cash::{repo as cash, service::round_cash},
        currencies, farm_supply,
        orders::{
            dto::LineInput,
            repo::{self as orders, NewOrder},
            service::{self as order_rules, Priced},
            trail,
        },
        settings,
        stock::{
            repo::{Cause, Floor},
            reserve::{self, Want},
        },
        till::repo::SessionRow,
        till_sales::{
            dto::{NewQuote, NewSale, Payment, QuoteLine, QuoteResource, SaleResource},
            repo::{self, NewSaleRow, SaleRow},
        },
    },
};

const REUSED: &str = "This client_ref was used for another sale on this register.";
const NO_SHIFT: &str = "No shift is open on this register: open the drawer first.";
const TOO_LITTLE: &str = "The tendered cash is less than the amount due.";
const TOO_BIG: &str = "The sale total is out of range.";
/// Largest sale total (minor units): far above any till basket, far below i64 overflow.
const TOTAL_MAX: i64 = 1_000_000_000_000;

/// The cash step of `currency` (ISO code, any case) in `cash_register.cash_rounding`; 1 when absent.
async fn cash_step(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    currency: &str,
) -> Result<i64, AppError> {
    let section =
        settings::service::section_or_default(&mut *conn, workspace_id, "cash_register").await?;
    let step = section["cash_rounding"]
        .as_array()
        .into_iter()
        .flatten()
        // Seeded currencies keep Laravel's lowercase codes ('eur').
        .find(|r| {
            r["currency"]
                .as_str()
                .is_some_and(|c| c.eq_ignore_ascii_case(currency))
        })
        .and_then(|r| r["step"].as_i64());
    Ok(step.unwrap_or(1))
}

/// A basket priced the way a sale books it.
struct SalePrice {
    currency_id: Uuid,
    /// At the prices the till charged.
    charged: Priced,
    /// At today's catalog prices.
    server: Priced,
    mismatched: usize,
    /// Cash rounded to the currency's step; card = total.
    due: i64,
}

/// Price `lines` for the session's register: the till's prices, the catalog's, and what is
/// due for `payment` (cash below due is refused). The sale and the quote both call this.
async fn price_sale(
    conn: &mut sqlx::PgConnection,
    s: &SessionRow,
    lines: &[LineInput],
    payment: &Payment,
) -> Result<SalePrice, AppError> {
    let ws = s.workspace_id;
    let (currency_id, dp) = order_rules::currency(&mut *conn, ws, Some(s.currency_id)).await?;
    let charged = order_rules::price(&mut *conn, ws, currency_id, dp, lines, false).await?;
    let catalog: Vec<LineInput> = lines
        .iter()
        .map(|l| LineInput {
            unit_price: None,
            ..l.clone()
        })
        .collect();
    if charged.total > TOTAL_MAX {
        return Err(AppError::field("lines", TOO_BIG));
    }
    let server = order_rules::price(&mut *conn, ws, currency_id, dp, &catalog, false).await?;
    let mismatched = charged
        .lines
        .iter()
        .zip(&server.lines)
        .filter(|(c, s)| c.unit_price != s.unit_price)
        .count();
    let due = match payment {
        Payment::Cash { tendered } => {
            let code = currencies::repo::find(&mut *conn, ws, currency_id)
                .await?
                .map(|c| c.asset_type)
                .unwrap_or_default();
            let due = round_cash(charged.total, cash_step(&mut *conn, ws, &code).await?);
            if tendered.is_some_and(|t| t < due) {
                return Err(AppError::field("payment.tendered", TOO_LITTLE));
            }
            due
        }
        Payment::Card { .. } => charged.total,
    };
    Ok(SalePrice {
        currency_id,
        charged,
        server,
        mismatched,
        due,
    })
}

/// What a sale of `q` would book now; reads only, in a transaction that is rolled back.
pub async fn quote(
    state: &AppState,
    s: &SessionRow,
    q: NewQuote,
) -> Result<QuoteResource, AppError> {
    let mut tx = state.db.begin().await?;
    let p = price_sale(&mut tx, s, &q.lines, &q.payment).await?;
    tx.rollback().await?;
    let tendered = match q.payment {
        Payment::Cash { tendered } => tendered,
        Payment::Card { .. } => None,
    };
    Ok(QuoteResource {
        lines: p
            .charged
            .lines
            .iter()
            .map(|l| QuoteLine {
                product_id: l.product_id,
                product_variant_id: l.product_variant_id,
                quantity: format!("{:.2}", l.quantity),
                unit_price: l.unit_price,
                line_excl: l.line_excl,
                line_vat: l.line_vat,
                line_total: l.line_excl + l.line_vat,
            })
            .collect(),
        subtotal: p.charged.subtotal,
        vat: p.charged.vat,
        total: p.charged.total,
        server_total: p.server.total,
        price_mismatch: p.charged.total - p.server.total,
        mismatched_lines: i32::try_from(p.mismatched).unwrap_or(i32::MAX),
        due: p.due,
        rounding: p.due - p.charged.total,
        change: tendered.map(|t| t - p.due),
    })
}

/// Book a till sale, or answer the one already booked under its `client_ref`. `true` when
/// this call wrote it.
pub async fn sell(
    state: &AppState,
    s: &SessionRow,
    sale: NewSale,
) -> Result<(bool, SaleRow), AppError> {
    let ws = s.workspace_id;
    let mut tx = state.db.begin().await?;
    repo::lock_ref(&mut tx, ws, s.register_id, &sale.client_ref).await?;
    if let Some(found) = repo::find(&mut *tx, ws, s.register_id, &sale.client_ref).await? {
        if found.request_hash != sale.hash {
            return Err(AppError::Conflict(REUSED));
        }
        return Ok((false, found));
    }

    let SalePrice {
        currency_id,
        charged,
        server,
        mismatched,
        due,
    } = price_sale(&mut tx, s, &sale.lines, &sale.payment).await?;
    let (shift_id, tendered, card_reference) = match &sale.payment {
        Payment::Cash { tendered } => {
            cash::lock_register(&mut tx, ws, s.register_id).await?;
            let shift = cash::open_shift(&mut *tx, ws, s.register_id)
                .await?
                .ok_or(AppError::Conflict(NO_SHIFT))?;
            (Some(shift.id), *tendered, None)
        }
        Payment::Card { reference } => (None, None, reference.as_deref()),
    };

    let number = order_rules::number(&mut tx, ws).await?;
    let order_id = orders::insert(
        &mut tx,
        ws,
        &NewOrder {
            number: &number,
            status: "completed",
            source: "till",
            relation_id: None,
            currency_id,
            warehouse_id: s.warehouse_id,
            customer: None,
            reference: None,
            note: None,
            expires_at: None,
            created_by: None,
            app_key_id: None,
        },
    )
    .await?;
    orders::write_lines(
        &mut tx,
        ws,
        order_id,
        &charged.lines,
        (charged.subtotal, charged.vat, charged.total),
        &charged.summary,
    )
    .await?;
    let written = orders::lines(&mut *tx, ws, &[order_id]).await?;
    let quantities: Vec<_> = written
        .iter()
        .map(|l| (l.id, l.quantity, l.quantity, l.returned))
        .collect();
    orders::set_line_quantities(&mut tx, ws, &quantities).await?;
    let sold_at = sale.sold_at.unwrap_or_else(Utc::now);
    repo::stamp_order(&mut *tx, ws, order_id, sold_at).await?;

    let wants: Vec<Want> = charged
        .lines
        .iter()
        .filter(|l| l.tracked)
        .map(|l| Want {
            product_id: l.product_id,
            quantity: l.quantity,
        })
        .collect();
    let reason = format!("till sale {number}");
    let cause = Cause {
        reason: Some(&reason),
        related: Some(("order", order_id)),
        ..Cause::default()
    };
    // Floor::None never comes back short.
    reserve::take(&mut tx, ws, s.warehouse_id, &wants, Floor::None, &cause).await?;
    farm_supply::sale::on_sale(&mut tx, ws, order_id, &number).await?;

    let server_prices = Value::Array(
        server
            .lines
            .iter()
            .map(|l| json!({ "product_id": l.product_id, "unit_price": l.unit_price }))
            .collect(),
    );
    repo::insert(
        &mut *tx,
        ws,
        &NewSaleRow {
            order_id,
            register_id: s.register_id,
            till_user_id: s.till_user_id,
            shift_id,
            client_ref: &sale.client_ref,
            request_hash: &sale.hash,
            method: if tendered.is_some() { "cash" } else { "card" },
            total: charged.total,
            server_total: server.total,
            mismatched_lines: i32::try_from(mismatched).unwrap_or(i32::MAX),
            server_prices: &server_prices,
            due,
            tendered,
            card_reference,
            sold_at,
        },
    )
    .await?;
    if let Some(shift_id) = shift_id {
        repo::book_cash(
            &mut *tx,
            ws,
            s.register_id,
            shift_id,
            order_id,
            due,
            s.till_user_id,
        )
        .await?;
    }
    let by = trail::Actor::TillUser(s.till_user_id);
    order_rules::created_event(&mut tx, ws, order_id, by, Some(sold_at)).await?;
    trail::log(&mut tx, ws, order_id, "completed", by, None, Some(sold_at)).await?;
    let row = repo::find(&mut *tx, ws, s.register_id, &sale.client_ref)
        .await?
        .ok_or_else(|| AppError::Internal("till sale vanished".into()))?;
    tx.commit().await?;
    Ok((true, row))
}

/// The register's sale under `client_ref`, else 404 (another register's ref included).
pub async fn get(state: &AppState, s: &SessionRow, client_ref: &str) -> Result<SaleRow, AppError> {
    repo::find(&state.db, s.workspace_id, s.register_id, client_ref)
        .await?
        .ok_or(AppError::NotFound)
}

/// One page of till sales and the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &repo::ListFilter,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SaleRow>, i64), AppError> {
    Ok(repo::list(&state.db, workspace_id, f, limit, offset).await?)
}

/// `rows` as resources with their booked lines: one statement for every row's lines.
pub async fn resources(
    state: &AppState,
    workspace_id: Uuid,
    rows: Vec<SaleRow>,
) -> Result<Vec<SaleResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.order_id).collect();
    let mut by_order: HashMap<Uuid, Vec<orders::LineRow>> = HashMap::new();
    for l in orders::lines(&state.db, workspace_id, &ids).await? {
        by_order.entry(l.order_id).or_default().push(l);
    }
    Ok(rows
        .into_iter()
        .map(|r| {
            let lines = by_order.remove(&r.order_id).unwrap_or_default();
            SaleResource::new(r, &lines)
        })
        .collect())
}
