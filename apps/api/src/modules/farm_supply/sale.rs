//! Farmers credited at sale (G12, agreed price per kg). When an order takes jars off the shelf,
//! each supply lot among them owes `price_per_kg × kg in those jars`, rounded once, split over
//! the batch's farmers by kg (largest remainder): one `sale` attribution, one passbook credit
//! per farmer and `sale.attributed`, all in the farm workspace, inside the sale's transaction.
//! Returns are cumulative: the returned jars fill the order's takes in take order ([`fill`], the
//! same spread that puts the stock back into those lots), each lot's debit is the same credit
//! function on its returned jars, and only the change since the last return is written. A full
//! return nets every farmer to exactly 0; a partial one may move a single minor unit between
//! farmers (largest remainder), which the next return evens out.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::AppError,
    modules::{
        farm_supply::{
            credit::{credit_total, hundredths, split},
            repo::{self, NewAttribution, TakeRow, WeightRow},
            service::lineage,
        },
        stock::reserve::Back,
        webhooks::events::{self, SALE_ATTRIBUTED},
    },
};

fn overflow() -> AppError {
    AppError::Internal("farmer credit out of range".into())
}

/// Each farmer's part of `total` by kg.
fn shares(total: i64, w: &[WeightRow]) -> Result<Vec<i64>, AppError> {
    let weights = w
        .iter()
        .map(|w| hundredths(w.kg).map(|kg| (w.person_id, kg)))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(overflow)?;
    Ok(split(total, &weights))
}

/// What one attribution writes: the row, the passbook lines and the event.
struct Booking<'a> {
    attribution: NewAttribution<'a>,
    weights: &'a [WeightRow],
    /// Signed passbook amount per farmer (same order as `weights`).
    amounts: Vec<i64>,
    line_kind: &'static str,
}

async fn book(conn: &mut PgConnection, b: Booking<'_>) -> Result<(), AppError> {
    let a = &b.attribution;
    let id = repo::insert_attribution(&mut *conn, a).await?;
    for (w, amount) in b.weights.iter().zip(&b.amounts) {
        if *amount != 0 {
            repo::attribution_line(
                &mut *conn,
                a.workspace_id,
                w.village_id,
                w.person_id,
                a.currency_id,
                b.line_kind,
                *amount,
                id,
            )
            .await?;
        }
    }
    let payload = json!({
        "attribution_id": id,
        "kind": a.kind,
        "sold_workspace_id": a.sold_workspace_id,
        "order_number": a.order_number,
        "sold_lot_id": a.sold_lot_id,
        "batch_id": a.batch_id,
        "jars": format!("{:.2}", a.jars),
        "price_per_kg": a.price_per_kg,
        "currency_id": a.currency_id,
        "total": a.total,
        // Thin (outbox payloads stay under 16 KiB): each farmer's amount is in the passbook.
        "farmers": b.weights.len(),
    });
    events::emit(conn, a.workspace_id, SALE_ATTRIBUTED, payload).await?;
    Ok(())
}

/// Credit the farmers of every supply lot the order `order_id` (of `sold_ws`) took.
pub async fn on_sale(
    conn: &mut PgConnection,
    sold_ws: Uuid,
    order_id: Uuid,
    order_number: &str,
) -> Result<(), AppError> {
    let takes = repo::order_takes(&mut *conn, sold_ws, order_id).await?;
    for t in &takes {
        let Some(lot) = t.lot_id else { continue };
        let Some(l) = lineage(&mut *conn, lot).await? else {
            continue;
        };
        let (raw, units) = l.pack.unwrap_or((Decimal::ONE, Decimal::ONE));
        let total = credit_total(l.price_per_kg, t.qty, raw, units).ok_or_else(overflow)?;
        // After the order's stock locks, like a correction approval: no deadlock between them.
        repo::share_batch(&mut *conn, l.root_workspace_id, l.batch_id).await?;
        let weights = repo::weights(&mut *conn, l.root_workspace_id, l.batch_id).await?;
        let amounts = shares(total, &weights)?;
        let attribution = NewAttribution {
            workspace_id: l.root_workspace_id,
            batch_id: l.batch_id,
            kind: "sale",
            sold_workspace_id: sold_ws,
            order_id,
            order_number,
            sold_lot_id: lot,
            jars: t.qty,
            pack_raw_kg: raw,
            pack_units: units,
            price_per_kg: l.price_per_kg,
            currency_id: l.currency_id,
            total,
        };
        let booking = Booking {
            attribution,
            weights: &weights,
            amounts,
            line_kind: "sale_credit",
        };
        book(conn, booking).await?;
    }
    Ok(())
}

/// After the order's returned quantities changed: `returned` = units returned so far per
/// product (cumulative). Debits the change on each credited lot.
pub async fn on_return(
    conn: &mut PgConnection,
    sold_ws: Uuid,
    order_id: Uuid,
    order_number: &str,
    returned: &BTreeMap<Uuid, Decimal>,
) -> Result<(), AppError> {
    let takes = repo::order_takes(&mut *conn, sold_ws, order_id).await?;
    for (product, back) in returned {
        let buckets: Vec<&TakeRow> = takes.iter().filter(|t| t.product_id == *product).collect();
        let qty: Vec<Decimal> = buckets.iter().map(|t| t.qty).collect();
        for (t, target_jars) in buckets.iter().zip(fill(&qty, *back)) {
            let Some(lot) = t.lot_id else { continue };
            let Some(sale) = repo::sale_of(&mut *conn, sold_ws, order_id, lot).await? else {
                continue;
            };
            // Before reading what farmers hold: an approval's splits count in it.
            repo::share_batch(&mut *conn, sale.workspace_id, sale.batch_id).await?;
            let (done_jars, done) =
                repo::returned_so_far(&mut *conn, sold_ws, order_id, lot).await?;
            let jars = target_jars - done_jars;
            if jars.is_zero() {
                continue;
            }
            let target = credit_total(
                sale.price_per_kg,
                target_jars,
                sale.pack_raw_kg,
                sale.pack_units,
            )
            .ok_or_else(overflow)?;
            let weights = repo::weights(&mut *conn, sale.workspace_id, sale.batch_id).await?;
            // Debit targets per farmer, minus what earlier returns already took back.
            let amounts: Vec<i64> = shares(target, &weights)?
                .into_iter()
                .zip(&weights)
                .map(|(debit, w)| {
                    let so_far = done.iter().find(|d| d.0 == w.person_id).map_or(0, |d| d.1);
                    -debit - so_far
                })
                .collect();
            let attribution = NewAttribution {
                workspace_id: sale.workspace_id,
                batch_id: sale.batch_id,
                kind: "return",
                sold_workspace_id: sold_ws,
                order_id,
                order_number,
                sold_lot_id: lot,
                jars,
                pack_raw_kg: sale.pack_raw_kg,
                pack_units: sale.pack_units,
                price_per_kg: sale.price_per_kg,
                currency_id: sale.currency_id,
                total: amounts.iter().sum(),
            };
            let booking = Booking {
                attribution,
                weights: &weights,
                amounts,
                line_kind: "sale_reversal",
            };
            book(conn, booking).await?;
        }
    }
    Ok(())
}

/// Spread `returned` units over takes of `qty` each, in take order, each up to what it took.
/// Monotone: more returned never gives a take less, so the change per take is never negative.
pub fn fill(qty: &[Decimal], returned: Decimal) -> Vec<Decimal> {
    let mut left = returned;
    qty.iter()
        .map(|q| {
            let part = left.min(*q).max(Decimal::ZERO);
            left -= part;
            part
        })
        .collect()
}

/// The rows an order return puts back on stock now: per take (lot, or no-lot), what the
/// returned units after this return fill minus what they filled before it.
pub async fn restock_plan(
    conn: &mut PgConnection,
    sold_ws: Uuid,
    order_id: Uuid,
    before: &BTreeMap<Uuid, Decimal>,
    after: &BTreeMap<Uuid, Decimal>,
) -> Result<Vec<Back>, AppError> {
    let takes = repo::order_takes(&mut *conn, sold_ws, order_id).await?;
    let mut out = Vec::new();
    for (product, now) in after {
        let was = before.get(product).copied().unwrap_or_default();
        let buckets: Vec<&TakeRow> = takes.iter().filter(|t| t.product_id == *product).collect();
        let qty: Vec<Decimal> = buckets.iter().map(|t| t.qty).collect();
        let (a, b) = (fill(&qty, *now), fill(&qty, was));
        let mut placed = Decimal::ZERO;
        for ((t, a), b) in buckets.iter().zip(a).zip(b) {
            if a > b {
                placed += a - b;
                out.push(Back {
                    product_id: *product,
                    lot_id: t.lot_id,
                    quantity: a - b,
                });
            }
        }
        // Units the order never took (an untracked surplus) go back on the no-lot row.
        if *now - was > placed {
            out.push(Back {
                product_id: *product,
                lot_id: None,
                quantity: *now - was - placed,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn fill_goes_in_take_order_up_to_each_take() {
        let qty = [d("2"), d("3"), d("1")];
        assert_eq!(fill(&qty, d("0")), [d("0"), d("0"), d("0")]);
        assert_eq!(fill(&qty, d("1.5")), [d("1.5"), d("0"), d("0")]);
        assert_eq!(fill(&qty, d("4")), [d("2"), d("2"), d("0")]);
        assert_eq!(fill(&qty, d("6")), [d("2"), d("3"), d("1")]);
        assert_eq!(
            fill(&qty, d("9")),
            [d("2"), d("3"), d("1")],
            "never more than taken"
        );
    }

    #[test]
    fn fill_is_monotone_so_deltas_are_never_negative() {
        let qty = [d("0.5"), d("2"), d("1.25")];
        let mut prev = fill(&qty, Decimal::ZERO);
        for step in 1..=400 {
            let now = fill(&qty, Decimal::new(step, 2));
            assert!(now.iter().zip(&prev).all(|(a, b)| a >= b), "{step}");
            prev = now;
        }
    }
}
