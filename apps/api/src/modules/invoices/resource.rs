//! Invoice responses: Laravel InvoiceResource (detail), InvoiceIndexResource (list rows) and
//! ItemResource (lines). Document totals come from `pricing::engine` (VAT per rate); the
//! per-line display numbers follow Laravel HasItemPricing on exact decimals.

use std::{collections::HashMap, str::FromStr};

use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    modules::{
        invoice_payments::resource as payments,
        invoices::{
            actions::InvoiceAction,
            repo::{InvoiceRow, ItemRow},
        },
        pricing::{
            dto::{MarginResource, SummaryResource},
            engine::{Amounts, Margin, RateTotal, Summary},
        },
        relations::address::{Address, AddressView},
    },
    money::{Zero, format_decimals, php_number},
    numbering::Appendix,
};

/// Embedded rows, by id, already in their resource shape.
#[derive(Default)]
pub struct Embeds {
    pub relations: HashMap<Uuid, Value>,
    pub currencies: HashMap<Uuid, Value>,
    pub products: HashMap<Uuid, Value>,
    pub warehouses: HashMap<Uuid, Value>,
    /// Detail only: the opened payment token, the sent PDF's file resource, the payments,
    /// the actions that pass now and the order it was made from.
    pub payment_hash: Option<String>,
    pub pdf_file: Option<Value>,
    pub payments: Vec<Value>,
    pub allowed_actions: Vec<InvoiceAction>,
    pub order: Option<Value>,
    /// Detail only: each line's (unit price incl. VAT, excl, total incl. VAT) in minor units,
    /// from SQL.
    pub line_cents: HashMap<Uuid, (i64, i64, i64)>,
}

/// `summary` as stored on the invoice: exact decimals as text, how many lines carry a discount
/// (Laravel's `discounts` list has one null per such line) and the decimals used.
pub fn stored_summary(s: &Summary, discounted: usize, dp: u32) -> Value {
    let text = |d: Decimal| Value::String(d.normalize().to_string());
    json!({
        "gross": text(s.gross), "discount": text(s.discount), "excl": text(s.excl),
        "vat": text(s.vat), "total": text(s.total), "discounted": discounted, "dp": dp,
        "vats": s.vats.iter()
            .map(|v| json!({ "rate": text(v.rate), "base": text(v.base), "vat": text(v.vat) }))
            .collect::<Vec<_>>(),
    })
}

/// Read back `stored_summary`: totals, discounted lines, decimals (an empty object is all zero
/// with no decimals).
pub fn read_summary(v: &Value) -> (Summary, usize, Option<u32>) {
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
    let dp = v
        .get("dp")
        .and_then(Value::as_u64)
        .and_then(|d| u32::try_from(d).ok());
    (summary, usize::try_from(discounted).unwrap_or(0), dp)
}

/// A line counted in `discounts`: a discount on a positive rounded gross (as calculate-items).
pub fn discounted(l: &ItemRow, a: &Amounts) -> bool {
    l.discount_percentage > Decimal::ZERO && a.gross > Decimal::ZERO
}

/// The formatted number (Laravel Invoice::invoice_nr); None until sent.
pub fn number(row: &InvoiceRow) -> Option<String> {
    row.invoice_number.map(|n| {
        Appendix::from_section(row.invoice_appendix.as_ref().unwrap_or(&Value::Null)).format(n)
    })
}

/// `invoice_nr` as the resources print it: a cancelled invoice prints "N/A".
fn invoice_nr(row: &InvoiceRow) -> Value {
    if row.status == "cancelled" {
        Value::from("N/A")
    } else {
        number(row).map_or(Value::Null, Value::from)
    }
}

/// Decimals the stored totals (and payments) are in.
fn dp(row: &InvoiceRow) -> u32 {
    read_summary(&row.summary).2.unwrap_or(2)
}

/// `money`: every amount of the invoice in minor units as SQL computed them, with the currency
/// code and the decimals they are in. Clients format these; they never add them up.
fn money(row: &InvoiceRow, e: &Embeds) -> Value {
    let currency = e
        .currencies
        .get(&row.currency_id)
        .map(|c| c["asset_type"].clone());
    json!({
        "currency": currency.unwrap_or(Value::Null),
        "decimals": dp(row),
        "subtotal": row.subtotal,
        "vat": row.vat_total,
        "total": row.total,
        "paid": row.paid_total,
        "open": row.open_total,
    })
}

/// Laravel getOpenBalance: total minus what is paid (negative when overpaid).
fn open(row: &InvoiceRow, total: Decimal) -> Decimal {
    total
        .checked_sub(payments::units(row.paid_total, dp(row)))
        .unwrap_or_default()
}

/// Laravel getPaidAmount as its resources printed it.
fn paid(row: &InvoiceRow) -> Value {
    payments::paid(row.paid_total, dp(row))
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
    let discounted = lines
        .iter()
        .zip(&summary.lines)
        .filter(|(l, a)| discounted(l, a))
        .count();
    let items: Vec<Value> = lines.iter().map(|l| item(l, e)).collect();
    let balance = json!({ "open": php_number(open(row, summary.total)),
                          "paid": paid(row), "total": php_number(summary.total) });
    json!({
        "id": row.id,
        "invoice_nr": invoice_nr(row),
        "status": row.status,
        "reference": row.reference,
        "payment_hash": e.payment_hash,
        "payment_method": row.payment_method,
        "balance": balance,
        "money": money(row, e),
        "note": row.note,
        "send_date": row.send_date,
        "due_date": row.due_date,
        "items": items,
        "summary": SummaryResource::of_document(summary, discounted),
        "is_credit": row.is_credit,
        "previous_invoice_id": row.previous_invoice_id,
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
        "allowed_actions": e.allowed_actions,
        "relationships": {
            "pdf_file": e.pdf_file,
            "estimate": null,
            "relation": e.relations.get(&row.relation_id),
            "currency": e.currencies.get(&row.currency_id),
            "payments": e.payments,
            "custom_sections": [],
            "parent_invoice": null,
            "child_invoices": [],
            "order": e.order,
            "work_order": null,
            "collective_invoice": null,
            "cash_register": null,
        },
    })
}

/// The customer's display name from its list resource: company name, else alias, else the
/// first contact's full name; blank counts as absent.
fn customer_name(relation: Option<&Value>) -> Option<&str> {
    let r = relation?;
    fn text(v: &Value) -> Option<&str> {
        v.as_str().map(str::trim).filter(|s| !s.is_empty())
    }
    text(&r["company_name"])
        .or_else(|| text(&r["alias"]))
        .or_else(|| text(&r["relationships"]["contacts"][0]["full_name"]))
}

/// One list row (Laravel InvoiceIndexResource), from the stored totals.
pub fn index(row: &InvoiceRow, e: &Embeds) -> Value {
    let (summary, discounted, _) = read_summary(&row.summary);
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
        "balance": { "paid": paid(row), "open": php_number(open(row, summary.total)) },
        "money": money(row, e),
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
        "customer_name": customer_name(e.relations.get(&row.relation_id)),
        "relationships": {
            "relation": e.relations.get(&row.relation_id),
            "currency": e.currencies.get(&row.currency_id),
        },
    })
}

/// Laravel MoneyHelper::formatDecimals.
fn fd(v: Decimal, dp: u32) -> String {
    format_decimals(Some(v), dp, true, Zero::Falsy)
}

/// Fields of the product a line embeds (vimdesk-web reads these; the whole list resource per
/// line made a 1000-line invoice megabytes long).
const PRODUCT_FIELDS: &[&str] = &[
    "id",
    "name",
    "product_nr",
    "reference",
    "reference_vendor",
    "sale_unit",
    "should_mutate_stock",
    "purchase_price",
    "price_incl",
    "original_price_incl",
    "total_available_stock",
];

/// One line (Laravel ItemResource). Display numbers are exact until printed, as Laravel's
/// floats were. Every op is checked; a figure that leaves Decimal's range prints 0 (stored
/// lines are within numeric(24,8) and priced without overflow, so only display products of two
/// big columns can).
fn item(l: &ItemRow, e: &Embeds) -> Value {
    let mul = |a: Decimal, b: Decimal| a.checked_mul(b).unwrap_or_default();
    let sub = |a: Decimal, b: Decimal| a.checked_sub(b).unwrap_or_default();
    let percent = |v: Decimal| v.checked_div(Decimal::ONE_HUNDRED).unwrap_or_default();
    let vat_calc = Decimal::ONE.checked_add(percent(l.vat)).unwrap_or_default();
    let pct = percent(l.discount_percentage);
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
    let single = mul(l.price, sub(Decimal::ONE, pct));
    let marge_each = if purchase_each > Decimal::ZERO {
        sub(single, purchase_each)
    } else {
        Decimal::ZERO
    };
    let marge_percentage = (!purchase_each.is_zero() && !single.is_zero())
        .then(|| sub(single, purchase_each).checked_div(single))
        .flatten()
        .map(|r| fd(mul(r, Decimal::ONE_HUNDRED), 2));
    let available_stock = product
        .and_then(|p| p.get("total_available_stock"))
        .cloned()
        .unwrap_or(Value::from(0));
    let mut relationships = serde_json::Map::new();
    if let Some(p) = product {
        let slim: serde_json::Map<String, Value> = PRODUCT_FIELDS
            .iter()
            .filter_map(|k| p.get(*k).map(|v| ((*k).to_owned(), v.clone())))
            .collect();
        relationships.insert("product".into(), Value::Object(slim));
    }
    relationships.insert(
        "mutate_from_warehouse".into(),
        l.mutate_from_warehouse_id
            .and_then(|id| e.warehouses.get(&id))
            .cloned()
            .unwrap_or(Value::Null),
    );
    let cents = e.line_cents.get(&l.id);
    json!({
        "id": l.id,
        "title": l.title,
        "description": l.description,
        "unit_price_minor": cents.map(|c| c.0),
        "subtotal_minor": cents.map(|c| c.1),
        "total_minor": cents.map(|c| c.2),
        "quantity": fd(l.quantity, 2),
        "price": fd(l.price, 4),
        "price_incl": fd(price_incl, 4),
        "price_incl_vat": fd(price_incl, 2),
        "total_each_price_incl": fd(mul(price_incl, sub(Decimal::ONE, pct)), 4),
        "marge": fd(marge_each, 2),
        "marge_percentage": marge_percentage,
        "marge_total": fd(mul(marge_each, l.quantity), 2),
        "vat": fd(l.vat, 2),
        "credited_amount": fd(l.credit_amount, 2),
        "from_multi_set": false,
        "is_parent_multi_set_product": false,
        "amount_left_to_credit": fd(sub(l.quantity, l.credit_amount), 2),
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
        "mutated_from_stock": fd(l.mutated_from_stock, 2),
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

/// Line fields the public page gets: Laravel ShortItemResource without purchase prices,
/// stock or product codes (a customer must not see costs).
const PUBLIC_ITEM_FIELDS: &[&str] = &[
    "id",
    "title",
    "description",
    "quantity",
    "price",
    "price_incl",
    "price_incl_vat",
    "vat",
    "subtotal",
    "total",
    "total_incl",
    "discount_fixed",
    "discount_percentage",
    "discount_amount",
    "discount_amount_incl",
    "credited_amount",
    "amount_left_to_credit",
];

/// Laravel InvoiceFromHashResource for the public payment page. `currency` is the short
/// currency, `company` the workspace profile.
pub fn public(
    row: &InvoiceRow,
    lines: &[ItemRow],
    summary: &Summary,
    token: &str,
    currency: Value,
    company: Value,
) -> Value {
    let discounted = lines
        .iter()
        .zip(&summary.lines)
        .filter(|(l, a)| discounted(l, a))
        .count();
    let items: Vec<Value> = lines
        .iter()
        .map(|l| {
            let mut v = item(l, &Embeds::default());
            if let Value::Object(m) = &mut v {
                m.retain(|k, _| PUBLIC_ITEM_FIELDS.contains(&k.as_str()));
                let discount = m.get("discount_percentage").cloned().unwrap_or_default();
                m.insert("discount".into(), discount);
            }
            v
        })
        .collect();
    // Laravel max(0, open); a settled, void or credited invoice has nothing to pay here (a
    // cash-rounded paid invoice may show a few cents open in the detail).
    let open = if matches!(row.status.as_str(), "paid" | "cancelled" | "credited") {
        Decimal::ZERO
    } else {
        open(row, summary.total).max(Decimal::ZERO)
    };
    json!({
        "invoice_nr": number(row),
        "is_credit": row.is_credit,
        "status": row.status,
        "reference": row.reference,
        "balance": { "open": php_number(open), "paid": paid(row), "total": php_number(summary.total) },
        "payment_hash": token,
        "payment_method": row.payment_method,
        "company_details": company,
        "due_date": row.due_date,
        "items": items,
        "summary": SummaryResource::of_document(summary, discounted),
        "relationships": { "order_fields": null, "currency": currency },
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
        let v = stored_summary(&s, 1, 2);
        assert_eq!(
            v["vats"],
            json!([{ "rate": "21", "base": "9.21", "vat": "1.93" }])
        );
        assert_eq!(read_summary(&v), (s, 1, Some(2)));
        assert_eq!(read_summary(&json!({})), (Summary::default(), 0, None));
    }
}
