//! Invoice responses: Laravel InvoiceResource (detail), InvoiceIndexResource (list rows) and
//! ItemResource (lines). Document totals come from `pricing::engine` (VAT per rate); the
//! per-line display numbers follow Laravel HasItemPricing on exact decimals.

use std::{collections::HashMap, str::FromStr};

use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    modules::{
        invoices::repo::{InvoiceRow, ItemRow},
        pricing::{
            dto::{MarginResource, SummaryResource},
            engine::{Margin, RateTotal, Summary},
        },
        relations::address::{Address, AddressView},
    },
    money::{Zero, format_decimals, php_number},
};

/// Embedded rows, by id, already in their resource shape.
#[derive(Default)]
pub struct Embeds {
    pub relations: HashMap<Uuid, Value>,
    pub currencies: HashMap<Uuid, Value>,
    pub products: HashMap<Uuid, Value>,
    pub warehouses: HashMap<Uuid, Value>,
}

/// `summary` as stored on the invoice: exact decimals as text, plus how many lines carry a
/// discount (Laravel's `discounts` list has one null per such line).
pub fn stored_summary(s: &Summary, discounted: usize) -> Value {
    let text = |d: Decimal| Value::String(d.normalize().to_string());
    json!({
        "gross": text(s.gross), "discount": text(s.discount), "excl": text(s.excl),
        "vat": text(s.vat), "total": text(s.total), "discounted": discounted,
        "vats": s.vats.iter()
            .map(|v| json!({ "rate": text(v.rate), "base": text(v.base), "vat": text(v.vat) }))
            .collect::<Vec<_>>(),
    })
}

/// Read back `stored_summary` (an empty object for an invoice never written is all zero).
pub fn read_summary(v: &Value) -> (Summary, usize) {
    let num = |v: &Value, k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .and_then(|s| Decimal::from_str(s).ok())
            .unwrap_or_default()
    };
    let vats = v
        .get("vats")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|r| RateTotal {
                    rate: num(r, "rate"),
                    base: num(r, "base"),
                    vat: num(r, "vat"),
                })
                .collect()
        })
        .unwrap_or_default();
    let summary = Summary {
        gross: num(v, "gross"),
        discount: num(v, "discount"),
        excl: num(v, "excl"),
        vat: num(v, "vat"),
        total: num(v, "total"),
        vats,
        lines: Vec::new(),
    };
    let discounted = v.get("discounted").and_then(Value::as_u64).unwrap_or(0);
    (summary, usize::try_from(discounted).unwrap_or(0))
}

/// Laravel `invoice_nr`: drafts have no number yet; a cancelled invoice prints "N/A".
// shortcut: no numbering until F6 (send); print the number once invoices get one.
fn invoice_nr(row: &InvoiceRow) -> Value {
    if row.status == "cancelled" {
        Value::from("N/A")
    } else {
        Value::Null
    }
}

fn billing_address(row: &InvoiceRow) -> Value {
    row.billing_address
        .clone()
        .and_then(|v| serde_json::from_value::<Address>(v).ok())
        .and_then(|a| serde_json::to_value(AddressView::from(&a)).ok())
        .unwrap_or(Value::Null)
}

fn customer_ref(row: &InvoiceRow) -> Value {
    row.customer_ref
        .clone()
        .unwrap_or_else(|| json!({ "ordered_by": null, "po_number": null, "reference": null }))
}

/// One invoice in detail (Laravel InvoiceResource). `summary`/`margin` come from `lines`.
pub fn detail(
    row: &InvoiceRow,
    lines: &[ItemRow],
    summary: &Summary,
    margin: &Margin,
    e: &Embeds,
) -> Value {
    let discounted = lines.iter().filter(|l| discounted(l)).count();
    let items: Vec<Value> = lines.iter().map(|l| item(l, e)).collect();
    // shortcut: no payments until F7, so nothing is paid and the whole total is open.
    let balance = json!({ "open": php_number(summary.total), "paid": 0,
                          "total": php_number(summary.total) });
    json!({
        "id": row.id,
        "invoice_nr": invoice_nr(row),
        "status": row.status,
        "reference": row.reference,
        "payment_hash": null,
        "payment_method": row.payment_method,
        "balance": balance,
        "note": row.note,
        "send_date": row.send_date,
        "due_date": row.due_date,
        "items": items,
        "summary": SummaryResource::of_document(summary, discounted),
        "is_credit": row.is_credit,
        "previous_invoice_id": null,
        "mollie_refunds": [],
        "addressed_contact": row.addressed_contact,
        "billing_address": billing_address(row),
        "marge_calculation_summary": MarginResource::new(margin),
        "import_id": null,
        "import_source": null,
        "customer_ref": customer_ref(row),
        "created_at": row.created_at,
        "last_updated_at": row.updated_at,
        "meta": row.meta,
        "relationships": {
            "pdf_file": null,
            "estimate": null,
            "relation": e.relations.get(&row.relation_id),
            "currency": e.currencies.get(&row.currency_id),
            "payments": [],
            "custom_sections": [],
            "parent_invoice": null,
            "child_invoices": [],
            "order": null,
            "work_order": null,
            "collective_invoice": null,
            "cash_register": null,
        },
    })
}

/// One list row (Laravel InvoiceIndexResource), from the stored totals.
pub fn index(row: &InvoiceRow, e: &Embeds) -> Value {
    let (summary, discounted) = read_summary(&row.summary);
    let on_account = row
        .meta
        .as_ref()
        .and_then(|m| m.get("on_account"))
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "id": row.id,
        "invoice_nr": invoice_nr(row),
        "status": row.status,
        "balance": { "paid": 0, "open": php_number(summary.total) },
        "reference": row.reference,
        "payment_hash": null,
        "payment_method": row.payment_method,
        "note": row.note,
        "has_activities": false,
        "has_note_activity": false,
        "is_on_account": on_account,
        "send_date": row.send_date,
        "due_date": row.due_date,
        "is_credit": row.is_credit,
        // Laravel formatted the stored total with two decimals, trailing zeros kept.
        "total_amount": format_decimals(Some(summary.total), 2, false, Zero::Falsy),
        "summary": SummaryResource::of_document(&summary, discounted),
        "created_at": row.created_at,
        "last_updated_at": row.updated_at,
        "has_collective_invoice": false,
        "import_id": null,
        "meta": row.meta,
        "relationships": {
            "relation": e.relations.get(&row.relation_id),
            "currency": e.currencies.get(&row.currency_id),
        },
    })
}

fn discounted(l: &ItemRow) -> bool {
    l.discount_percentage > Decimal::ZERO
        && l.quantity
            .checked_mul(l.price)
            .is_some_and(|g| g > Decimal::ZERO)
}

/// Laravel MoneyHelper::formatDecimals.
fn fd(v: Decimal, dp: u32) -> String {
    format_decimals(Some(v), dp, true, Zero::Falsy)
}

/// One line (Laravel ItemResource). Display numbers are exact until printed, as Laravel's
/// floats were; overflowing ones print as 0 (the stored lines summed without overflow).
fn item(l: &ItemRow, e: &Embeds) -> Value {
    let hundred = Decimal::ONE_HUNDRED;
    let mul = |a: Decimal, b: Decimal| a.checked_mul(b).unwrap_or_default();
    let sub = |a: Decimal, b: Decimal| a.checked_sub(b).unwrap_or_default();
    let vat_calc = Decimal::ONE + l.vat / hundred;
    let pct = l.discount_percentage / hundred;
    let sum = mul(l.quantity, l.price);
    let discount = mul(sum, pct);
    let total = sub(sum, discount);
    let vat_amount = sub(mul(total, vat_calc), total);
    let price_incl = mul(l.price, vat_calc);
    let product = l.product_id.and_then(|id| e.products.get(&id));
    let product_purchase = product
        .and_then(|p| p.get("purchase_price"))
        .and_then(|v| match v {
            Value::String(s) => Decimal::from_str(s).ok(),
            Value::Number(n) => Decimal::from_str(&n.to_string()).ok(),
            _ => None,
        })
        .unwrap_or_default();
    let purchase_each = if l.purchase_price > Decimal::ZERO {
        l.purchase_price
    } else {
        product_purchase
    };
    let single = mul(l.price, Decimal::ONE - pct);
    let marge_each = if purchase_each > Decimal::ZERO {
        sub(single, purchase_each)
    } else {
        Decimal::ZERO
    };
    let marge_percentage = (!purchase_each.is_zero() && !single.is_zero())
        .then(|| sub(single, purchase_each).checked_div(single))
        .flatten()
        .map(|r| fd(mul(r, hundred), 2));
    let available_stock = product
        .and_then(|p| p.get("total_available_stock"))
        .cloned()
        .unwrap_or(Value::from(0));
    let mut relationships = serde_json::Map::new();
    if let Some(p) = product {
        relationships.insert("product".into(), p.clone());
    }
    relationships.insert(
        "mutate_from_warehouse".into(),
        l.mutate_from_warehouse_id
            .and_then(|id| e.warehouses.get(&id))
            .cloned()
            .unwrap_or(Value::Null),
    );
    json!({
        "id": l.id,
        "title": l.title,
        "description": l.description,
        "quantity": fd(l.quantity, 2),
        "price": fd(l.price, 4),
        "price_incl": fd(price_incl, 4),
        "price_incl_vat": fd(price_incl, 2),
        "total_each_price_incl": fd(mul(price_incl, Decimal::ONE - pct), 4),
        "marge": fd(marge_each, 2),
        "marge_percentage": marge_percentage,
        "marge_total": fd(mul(marge_each, l.quantity), 2),
        "vat": fd(l.vat, 2),
        "credited_amount": fd(Decimal::ZERO, 2),
        "from_multi_set": false,
        "is_parent_multi_set_product": false,
        "amount_left_to_credit": fd(l.quantity, 2),
        "returnable_serial_numbers": [],
        "subtotal": fd(sum, 2),
        "total": fd(total, 2),
        "total_incl": fd(total.checked_add(vat_amount).unwrap_or_default(), 2),
        "vat_amount": fd(vat_amount, 4),
        "discount_fixed": fd(Decimal::ZERO, 2),
        "discount_percentage": fd(l.discount_percentage, 2),
        "discount_amount": fd(discount, 2),
        "discount_amount_incl": fd(mul(discount, vat_calc), 2),
        "purchase_price": fd(purchase_each, 2),
        "purchase_price_total": fd(mul(purchase_each, l.quantity), 2),
        "purchase_price_incl": fd(mul(purchase_each, vat_calc), 2),
        "available_stock": available_stock,
        "accepted": false,
        "picked": fd(Decimal::ZERO, 2),
        "qty_shipped": fd(Decimal::ZERO, 2),
        "qty_cancelled": fd(Decimal::ZERO, 2),
        "billed": false,
        "mutated_from_stock": fd(Decimal::ZERO, 2),
        "backorder": fd(Decimal::ZERO, 2),
        "position": l.position,
        "able_to_pick": false,
        "stock_id": null,
        "serial_numbers": [],
        "meta": null,
        "product_variant_id": l.product_variant_id,
        "created_at": l.created_at,
        "last_updated_at": l.updated_at,
        "relationships": relationships,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_summary_reads_back() {
        let s = Summary {
            gross: Decimal::new(1021, 2),
            discount: Decimal::ONE,
            excl: Decimal::new(921, 2),
            vat: Decimal::new(193, 2),
            total: Decimal::new(1114, 2),
            vats: vec![RateTotal {
                rate: Decimal::new(2100, 2),
                base: Decimal::new(921, 2),
                vat: Decimal::new(193, 2),
            }],
            lines: Vec::new(),
        };
        let v = stored_summary(&s, 1);
        assert_eq!(
            v["vats"],
            json!([{ "rate": "21", "base": "9.21", "vat": "1.93" }])
        );
        let (back, n) = read_summary(&v);
        assert_eq!((back, n), (s, 1));
        let (zero, n) = read_summary(&json!({}));
        assert_eq!((zero, n), (Summary::default(), 0));
    }
}
