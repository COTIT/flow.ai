//! Credit notes (Laravel CreditInvoice from an invoice): a draft credit note whose lines take
//! back what is left of the source's lines, at minus the quantity. Lock order: the source
//! invoice row first (credits of one invoice queue, so they can't take the same quantity
//! twice), then the new credit note. Stock and the number come when the credit note is sent.

use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::human,
    modules::{
        invoices::{
            dto::{CreditInput, CreditLine},
            repo::{self, ItemRow, NewInvoice, NewLines},
            service::{Errors, parse, store_totals},
        },
        pricing::engine,
        settings,
    },
};

const NO_ITEMS: &str = "There are no items to credit.";
const CREDIT_NOTE: &str = "A credit note can not be credited.";
const NOT_IN_INVOICE: &str = "item does not belong to given domain";

/// A credit line to write: its source, quantity taken back and unit price/discount.
struct Taken<'a> {
    source: &'a ItemRow,
    quantity: Decimal,
    price: Decimal,
    discount: Decimal,
}

/// POST /invoices/{id}/credit: the new credit note's id.
pub async fn credit(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    input: &CreditInput,
) -> Result<Uuid, AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let locked = repo::lock(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if locked.is_credit {
        return Err(AppError::Unprocessable(CREDIT_NOTE.into()));
    }
    // Laravel Invoice::canBeCredited, plus: only a sent invoice (an unsent one moved no stock
    // and has no number to credit; Laravel allowed on-account drafts of the till, F15).
    let on_account = repo::relation_terms(&mut *tx, workspace_id, locked.relation_id)
        .await?
        .is_some_and(|t| t.allow_invoicing_on_account);
    let allowed = locked.invoice_number.is_some()
        && if on_account {
            locked.status != "credited"
        } else {
            matches!(locked.status.as_str(), "paid" | "partially_paid")
        };
    if !allowed {
        return Err(AppError::Unprocessable(format!(
            "This Invoice has the status {} and does not have the correct status to be credited.",
            locked.status
        )));
    }
    let source = repo::find(&mut *tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut *tx, workspace_id, id).await?;
    let dp = match locked.dp {
        Some(dp) => u32::try_from(dp).unwrap_or(2),
        None => u32::try_from(
            repo::currency_decimals(&mut *tx, workspace_id, locked.currency_id).await?,
        )
        .unwrap_or(2),
    };
    let taken = credit_lines(&lines, input, dp)?;
    if taken.is_empty() {
        return Err(AppError::Unprocessable(NO_ITEMS.into()));
    }
    let credit_settings =
        settings::service::section_or_default(&mut *tx, workspace_id, "credit").await?;
    let credit_id = repo::insert(
        &mut tx,
        workspace_id,
        &NewInvoice {
            relation_id: source.relation_id,
            currency_id: source.currency_id,
            is_credit: true,
            payment_method: "bank_transfer",
            reference: source.reference.as_deref(),
            note: credit_settings.get("default_note").and_then(Value::as_str),
            billing_address: None,
            addressed_contact: None,
            meta: None,
            created_by: user_id,
            previous_invoice_id: Some(id),
            customer_ref: source.customer_ref.as_ref(),
        },
    )
    .await?;
    let mut n = NewLines::default();
    for (i, t) in taken.iter().enumerate() {
        let l = t.source;
        n.product_id.push(l.product_id);
        n.product_variant_id.push(l.product_variant_id);
        n.warehouse_id.push(l.mutate_from_warehouse_id);
        n.title.push(l.title.clone());
        n.description.push(l.description.clone());
        n.quantity.push(-t.quantity);
        n.price.push(t.price);
        n.vat.push(l.vat);
        n.discount.push(t.discount);
        n.purchase_price.push(l.purchase_price);
        n.position.push(i32::try_from(i).unwrap_or(i32::MAX));
        n.parent_item_id.push(Some(l.id));
    }
    repo::insert_lines(&mut tx, workspace_id, credit_id, &n).await?;
    let ids: Vec<Uuid> = taken.iter().map(|t| t.source.id).collect();
    let quantities: Vec<Decimal> = taken.iter().map(|t| t.quantity).collect();
    repo::add_credited(&mut tx, workspace_id, id, &ids, &quantities).await?;
    store_totals(&mut tx, workspace_id, credit_id, source.currency_id).await?;
    // Laravel cancelFullyCreditedUnpaidInvoice: an unpaid source with nothing left goes.
    if matches!(locked.status.as_str(), "draft" | "pending") {
        let left = lines.iter().any(|l| {
            let now = taken
                .iter()
                .find(|t| t.source.id == l.id)
                .map_or(Decimal::ZERO, |t| t.quantity);
            l.quantity > Decimal::ZERO && l.quantity - l.credit_amount - now > Decimal::ZERO
        });
        if !left {
            repo::set_status(&mut tx, workspace_id, id, "cancelled").await?;
        }
    }
    tx.commit().await?;
    Ok(credit_id)
}

/// The lines Laravel's `formatItemsAndModifyTotal(fromInvoice)` credits, after the request
/// checks (run here, under the source lock, on the current amounts):
/// - `credit_all`: every line's remaining quantity at its price and discount.
/// - Else each selected line with `credit_amount` > 0: that quantity; with a `return_amount`
///   (incl. VAT, for that quantity) the unit price becomes return / (1 + VAT) / quantity and
///   the discount goes.
fn credit_lines<'a>(
    lines: &'a [ItemRow],
    input: &CreditInput,
    dp: u32,
) -> Result<Vec<Taken<'a>>, AppError> {
    let mut errors = Errors::default();
    for sel in &input.items {
        let field = |k: &str| format!("items.{}.{k}", sel.index);
        let Some(l) = lines.iter().find(|l| l.id == sel.id) else {
            errors.add(&field("id"), NOT_IN_INVOICE);
            continue;
        };
        let left = l.quantity - l.credit_amount;
        if sel.credit_amount.is_some_and(|q| q > left) {
            errors.add(
                &field("credit_amount"),
                format!(
                    "more items are being credited while the maximum amount is: {}",
                    left.normalize()
                ),
            );
            continue;
        }
        // Laravel's cap (`abs(...) < 0`) never fired; this one holds the refund to what the
        // credited quantity cost, priced as the invoice was.
        if let (Some(ret), Some(q)) = (sel.return_amount, sel.credit_amount)
            && q > Decimal::ZERO
        {
            let f = field("return_amount");
            let cap = paid_for(l, q, dp).ok_or_else(out_of_range)?;
            if ret < Decimal::ZERO {
                errors.add(&f, format!("The {} must be at least 0.", human(&f)));
            } else if ret > cap {
                errors.add(
                    &f,
                    format!("The {} may not be greater than {cap}.", human(&f)),
                );
            }
        }
    }
    errors.into_result()?;
    let mut taken = Vec::new();
    for l in lines {
        let left = l.quantity - l.credit_amount;
        let (quantity, price, discount) = if input.credit_all {
            (left, l.price, l.discount_percentage)
        } else {
            // Laravel took the first selection of a line.
            let Some(quantity) = input
                .items
                .iter()
                .find(|s| s.id == l.id)
                .and_then(|s: &CreditLine| s.credit_amount.map(|q| (q, s.return_amount)))
            else {
                continue;
            };
            match quantity {
                (q, Some(ret)) if !ret.is_zero() && q > Decimal::ZERO => (
                    q,
                    unit_price(ret, l.vat, q).ok_or_else(out_of_range)?,
                    Decimal::ZERO,
                ),
                (q, _) => (q, l.price, l.discount_percentage),
            }
        };
        if quantity > Decimal::ZERO && quantity <= left {
            taken.push(Taken {
                source: l,
                quantity,
                price,
                discount,
            });
        }
    }
    Ok(taken)
}

fn out_of_range() -> AppError {
    AppError::field("items", "The item totals are out of range.")
}

/// Incl. VAT total of `quantity` of line `l` at `dp` decimals, as the invoice priced it
/// (`pricing::engine`: gross, discount and the line's VAT each rounded).
fn paid_for(l: &ItemRow, quantity: Decimal, dp: u32) -> Option<Decimal> {
    let a = engine::amounts(
        &engine::Line {
            quantity,
            price: l.price,
            vat: l.vat,
            discount: l.discount_percentage,
            purchase_price: None,
            set_child: false,
        },
        dp,
    )?;
    a.excl.checked_add(a.line_vat)
}

/// Excl. VAT unit price of a refund `ret` (incl. VAT) over `quantity`, at the column's 8
/// decimals.
fn unit_price(ret: Decimal, vat: Decimal, quantity: Decimal) -> Option<Decimal> {
    let hundred = Decimal::ONE_HUNDRED;
    ret.checked_mul(hundred)?
        .checked_div(hundred + vat)?
        .checked_div(quantity)
        .map(|v| v.round_dp_with_strategy(8, RoundingStrategy::MidpointAwayFromZero))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn the_refund_cap_is_the_line_as_priced() {
        let line = |price: &str, discount: &str| ItemRow {
            id: uuid::Uuid::nil(),
            product_id: None,
            product_variant_id: None,
            mutate_from_warehouse_id: None,
            title: String::new(),
            description: None,
            quantity: d("3"),
            price: d(price),
            vat: d("21"),
            discount_percentage: d(discount),
            purchase_price: Decimal::ZERO,
            position: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            credit_amount: Decimal::ZERO,
            mutated_from_stock: Decimal::ZERO,
        };
        // 0.375 -> 0.38 excl, 0.08 VAT: the line shows 0.46 (a single rounding gave 0.45).
        assert_eq!(paid_for(&line("0.125", "0"), d("3"), 2), Some(d("0.46")));
        assert_eq!(paid_for(&line("0.125", "0"), d("3"), 3), Some(d("0.454")));
        assert_eq!(paid_for(&line("10", "10"), d("2"), 2), Some(d("21.78")));
    }

    #[test]
    fn refunds_turn_into_excl_unit_prices() {
        assert_eq!(unit_price(d("12.10"), d("21"), d("2")), Some(d("5")));
        assert_eq!(
            unit_price(d("10"), d("21"), d("3")),
            Some(d("2.75482094")),
            "8 decimals, half away from zero"
        );
        assert_eq!(unit_price(d("1"), d("0"), Decimal::ZERO), None);
    }
}
