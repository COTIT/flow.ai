//! Old → new rows a review card leads with, from captured facts and the validated request.
//!
//! Each row is `{field, label?, before?, after}`; money rows carry `currency` and
//! `before_minor`/`after_minor` integers instead. Rows only describe; the write path decides.

use crate::{
    AppError,
    modules::{orders, stock},
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

/// Quantity text: exact, no trailing zeros.
fn qty(d: Decimal) -> String {
    d.normalize().to_string()
}

fn row(field: &str, label: &str, before: String, after: String) -> Value {
    json!({"field":field,"label":label,"before":before,"after":after})
}

/// Money row in integer minor units; `before` absent for a new amount.
pub fn money(field: &str, currency: &Value, before: Option<i64>, after: i64) -> Value {
    // Stored codes are lowercase on invoices; every row shows the ISO code.
    let code = currency["code"].as_str().map(str::to_uppercase);
    let mut entry = json!({"field":field,"currency":{"code":code,"decimals":currency["decimals"]},"after_minor":after});
    if let Some(before) = before {
        entry["before_minor"] = before.into();
    }
    entry
}

fn decimal(value: &Value) -> Result<Decimal, AppError> {
    value
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AppError::Internal("review quantity is not a decimal".into()))
}

/// Warehouse name, then the bin when the slot has one.
fn label(slot: &Value) -> String {
    let warehouse = slot["warehouse_name"].as_str().unwrap_or_default();
    match slot["bin_name"].as_str() {
        Some(bin) => format!("{warehouse} · {bin}"),
        None => warehouse.to_owned(),
    }
}

fn slots(slots: &Value) -> impl Iterator<Item = &Value> {
    slots.as_array().into_iter().flatten()
}

/// Min/max rows where the requested value differs from the slot's.
fn requirement_rows(rows: &mut Vec<Value>, slot: &Value, wanted: [(&str, Option<i32>); 2]) {
    for (field, value) in wanted {
        if let Some(value) = value
            && slot[field].as_i64() != Some(value.into())
        {
            rows.push(row(
                field,
                &label(slot),
                slot[field].to_string(),
                value.to_string(),
            ));
        }
    }
}

/// set-stock-levels: absent level sets 0, kept to 2 places; min/max only when sent.
pub fn stock_levels(locked: &Value, entries: &[stock::dto::Entry]) -> Result<Vec<Value>, AppError> {
    let mut rows = Vec::new();
    for (slot, e) in slots(&locked["slots"]).zip(entries) {
        let before = decimal(&slot["before"])?;
        let after = crate::money::round(e.stock_lvl.unwrap_or_default(), 2);
        if after != before {
            rows.push(row("stock", &label(slot), qty(before), qty(after)));
        }
        requirement_rows(
            &mut rows,
            slot,
            [("minimal", e.minimal), ("maximum", e.maximum)],
        );
    }
    Ok(rows)
}

/// set-stock-requirements: an absent min or max sets 0.
pub fn requirements(locked: &Value, entries: &[stock::dto::Entry]) -> Vec<Value> {
    let mut rows = Vec::new();
    for (slot, e) in slots(&locked["slots"]).zip(entries) {
        let wanted = [
            ("minimal", Some(e.minimal.unwrap_or_default())),
            ("maximum", Some(e.maximum.unwrap_or_default())),
        ];
        requirement_rows(&mut rows, slot, wanted);
    }
    rows
}

/// Add or take stock: nothing for untracked products or a change that rounds to 0.
pub fn mutation(locked: &Value, change: Decimal) -> Result<Vec<Value>, AppError> {
    let change = crate::money::round(change, 2);
    let Some(slot) = slots(&locked["slots"]).next() else {
        return Ok(Vec::new());
    };
    if locked["tracks_stock"] != true || change.is_zero() {
        return Ok(Vec::new());
    }
    let before = decimal(&slot["before"])?;
    Ok(vec![row(
        "stock",
        &label(slot),
        qty(before),
        qty(before + change),
    )])
}

/// Move between bins: source down, target up by the same quantity.
pub fn moved(locked: &Value, quantity: Decimal) -> Result<Vec<Value>, AppError> {
    let mut rows = Vec::new();
    for (slot, (field, sign)) in slots(&locked["slots"]).zip([("stock_from", -1), ("stock_to", 1)])
    {
        let before = decimal(&slot["before"])?;
        let after = before + quantity * Decimal::from(sign);
        rows.push(row(field, &label(slot), qty(before), qty(after)));
    }
    Ok(rows)
}

/// A lot's level at one warehouse before and after the change.
pub fn lot(warehouse: &str, lot: &str, level: Decimal, change: Decimal) -> Vec<Value> {
    if change.is_zero() {
        return Vec::new();
    }
    vec![row(
        "stock",
        &format!("{warehouse} · {lot}"),
        qty(level),
        qty(level + change),
    )]
}

/// Rows for an order write, from its locked state, optional priced quote and the request.
pub fn order(
    operation: &str,
    facts: &Value,
    lines: &[orders::repo::LineRow],
    body: Value,
) -> Result<Vec<Value>, AppError> {
    let locked = &facts["locked"];
    let currency = json!({"code":locked["currency"].as_str().map(str::to_uppercase),"decimals":locked["decimals"]});
    let status = |after: &str| {
        let before = locked["status"].as_str().unwrap_or_default();
        Ok(if before == after {
            Vec::new()
        } else {
            vec![json!({"field":"status","before":before,"after":after})]
        })
    };
    let quantities = |field: &str, add: bool| -> Result<Vec<Value>, AppError> {
        let wanted =
            super::preconditions::validated::<orders::dto::LinesQuantityRequest>(body.clone())?;
        Ok(wanted
            .lines
            .iter()
            .filter_map(|(id, q)| {
                let line = lines.iter().find(|l| l.id == *id)?;
                let before = if add { line.returned } else { line.picked };
                let after = if add { before + q } else { *q };
                (after != before).then(|| row(field, &line.title, qty(before), qty(after)))
            })
            .collect())
    };
    let total = |priced: &Value, before: Option<i64>| {
        Ok(match priced["total_minor"].as_i64() {
            Some(after) if before != Some(after) => vec![money("total", &currency, before, after)],
            _ => Vec::new(),
        })
    };
    match operation {
        "orders_confirm" => status("open"),
        "orders_complete" => status("completed"),
        "orders_cancel" => status("cancelled"),
        "orders_pick" => quantities("picked", false),
        "orders_returns" => quantities("returned", true),
        "orders_update" => total(&facts["priced"], locked["total_minor"].as_i64()),
        "orders_invoice" => total(&facts["priced"], None),
        "orders_refund" => {
            let refund = super::preconditions::validated::<orders::dto::RefundRequest>(body)?;
            let decimals = locked["decimals"]
                .as_u64()
                .and_then(|d| u32::try_from(d).ok())
                .ok_or_else(|| AppError::Internal("order currency decimals".into()))?;
            let amount = crate::money::minor(refund.amount, decimals)?;
            let paid = locked["paid_minor"].as_i64().unwrap_or_default();
            Ok(vec![
                money("refund", &currency, None, amount),
                money("paid", &currency, Some(paid), paid - amount),
            ])
        }
        _ => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(before: &str, bin: Option<&str>) -> Value {
        json!({"warehouse_name":"Main","bin_name":bin,"before":before,"minimal":2,"maximum":0})
    }

    fn entry(level: Option<&str>, minimal: Option<i32>) -> stock::dto::Entry {
        stock::dto::Entry {
            warehouse_id: uuid::Uuid::nil(),
            bin_id: None,
            stock_lvl: level.map(|l| l.parse().unwrap()),
            reason: None,
            minimal,
            maximum: None,
        }
    }

    #[test]
    fn levels_round_like_the_write_and_skip_unchanged_values() {
        let locked = json!({"slots":[slot("14", None), slot("3", Some("H-1"))]});
        let rows = stock_levels(
            &locked,
            &[entry(Some("130.004"), Some(2)), entry(None, Some(4))],
        )
        .unwrap();
        assert_eq!(
            rows,
            vec![
                row("stock", "Main", "14".into(), "130".into()),
                row("stock", "Main · H-1", "3".into(), "0".into()),
                row("minimal", "Main · H-1", "2".into(), "4".into()),
            ]
        );
    }

    #[test]
    fn untracked_or_zero_mutations_list_nothing() {
        let tracked = json!({"tracks_stock":true,"slots":[slot("1.5", None)]});
        assert!(mutation(&tracked, Decimal::new(1, 3)).unwrap().is_empty());
        assert!(
            mutation(
                &json!({"tracks_stock":false,"slots":[slot("1", None)]}),
                Decimal::ONE
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            mutation(&tracked, Decimal::new(-15, 1)).unwrap(),
            vec![row("stock", "Main", "1.5".into(), "0".into())]
        );
    }

    #[test]
    fn money_rows_stay_integer_minor_units() {
        let eur = json!({"code":"EUR","decimals":2,"id":"x"});
        assert_eq!(
            money("paid", &eur, Some(1250), 750),
            json!({"field":"paid","currency":{"code":"EUR","decimals":2},"before_minor":1250,"after_minor":750})
        );
        assert_eq!(money("total", &eur, None, 1)["before_minor"], Value::Null);
    }
}
