//! Source revision fingerprint (PLAN §6a, R3 override): SHA-256 hex of canonical JSON with
//! sorted keys and decimals as normalized strings. `write_date` and `picking_type_id` are left
//! out, so a touch without a material change keeps the fingerprint.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::integrations::capability::PurchaseRequest;

/// Same value whatever the scale: `3`, `3.0` and `3.00000000` agree.
fn num(d: Decimal) -> Value {
    Value::String(d.normalize().to_string())
}

/// Fingerprint of the fields a revision depends on. BTreeMap keeps keys sorted.
pub fn of(req: &PurchaseRequest) -> String {
    let mut lines: Vec<_> = req.lines.iter().collect();
    lines.sort_by_key(|l| (l.id.parse::<i64>().unwrap_or(i64::MAX), l.id.clone()));
    let lines: Vec<Value> = lines
        .into_iter()
        .map(|l| {
            let mut taxes = l.tax_ids.clone();
            taxes.sort();
            let m: BTreeMap<&str, Value> = BTreeMap::from([
                ("date_planned", json!(l.date_planned)),
                ("id", json!(l.id)),
                ("price_unit", num(l.price_unit)),
                ("product_id", json!(l.product_id)),
                ("quantity", num(l.quantity)),
                ("tax_ids", json!(taxes)),
                ("uom_id", json!(l.uom_id)),
            ]);
            json!(m)
        })
        .collect();
    let doc: BTreeMap<&str, Value> = BTreeMap::from([
        ("amount_tax", num(req.amount_tax)),
        ("amount_total", num(req.amount_total)),
        ("amount_untaxed", num(req.amount_untaxed)),
        ("buyer_id", json!(req.buyer_id)),
        ("company_id", json!(req.company_id)),
        ("currency_id", json!(req.currency_id)),
        ("lines", Value::Array(lines)),
        ("vendor_id", json!(req.vendor_id)),
    ]);
    let text = serde_json::to_string(&doc).unwrap_or_default();
    hex::encode(Sha256::digest(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::integrations::capability::{RequestLine, SourceState};

    fn line(id: &str, qty: &str, taxes: &[&str]) -> RequestLine {
        RequestLine {
            id: id.into(),
            product_id: "9".into(),
            uom_id: "1".into(),
            quantity: Decimal::from_str(qty).unwrap(),
            price_unit: Decimal::from_str("150").unwrap(),
            subtotal: Decimal::from_str("300").unwrap(),
            tax_ids: taxes.iter().map(|t| t.to_string()).collect(),
            date_planned: Some("2026-11-20 00:00:00".into()),
        }
    }

    fn req(lines: Vec<RequestLine>) -> PurchaseRequest {
        PurchaseRequest {
            id: "17".into(),
            state: SourceState::Draft,
            company_id: "1".into(),
            vendor_id: "42".into(),
            currency_id: "1".into(),
            currency_ref: "EUR".into(),
            currency_decimals: 2,
            buyer_id: Some("2".into()),
            location_id: None,
            date_order: Some("2026-11-17 10:00:00".into()),
            lines,
            amount_untaxed: Decimal::from_str("300.00").unwrap(),
            amount_tax: Decimal::from_str("63.00").unwrap(),
            amount_total: Decimal::from_str("363.00").unwrap(),
            labels: Default::default(),
        }
    }

    #[test]
    fn hex_sha256_stable_across_order_scale_and_labels() {
        let a = of(&req(vec![
            line("10", "2", &["1", "2"]),
            line("9", "1", &[]),
        ]));
        assert!(a.len() == 64 && a.chars().all(|c| c.is_ascii_hexdigit()));
        let mut b = req(vec![
            line("9", "1.000", &[]),
            line("10", "2.0", &["2", "1"]),
        ]);
        b.labels.insert("vendor:42".into(), "Azure".into());
        b.state = SourceState::Sent;
        b.date_order = None;
        assert_eq!(
            a,
            of(&b),
            "line order, decimal scale, tax order, labels, state"
        );
    }

    #[test]
    fn material_changes_change_it() {
        let base = of(&req(vec![line("9", "2", &["1"])]));
        type Change = Box<dyn Fn(&mut PurchaseRequest)>;
        let changes: Vec<Change> = vec![
            Box::new(|r| r.vendor_id = "43".into()),
            Box::new(|r| r.company_id = "2".into()),
            Box::new(|r| r.currency_id = "3".into()),
            Box::new(|r| r.buyer_id = None),
            Box::new(|r| r.lines[0].quantity = Decimal::from(3)),
            Box::new(|r| r.lines[0].price_unit = Decimal::from(151)),
            Box::new(|r| r.lines[0].product_id = "8".into()),
            Box::new(|r| r.lines[0].uom_id = "2".into()),
            Box::new(|r| r.lines[0].tax_ids.push("5".into())),
            Box::new(|r| r.lines[0].date_planned = None),
            Box::new(|r| r.amount_tax = Decimal::from(62)),
            Box::new(|r| r.amount_total = Decimal::from(362)),
            Box::new(|r| r.amount_untaxed = Decimal::from(299)),
            Box::new(|r| r.lines.push(line("11", "1", &[]))),
        ];
        for (i, change) in changes.iter().enumerate() {
            let mut r = req(vec![line("9", "2", &["1"])]);
            change(&mut r);
            assert_ne!(of(&r), base, "change {i}");
        }
    }
}
