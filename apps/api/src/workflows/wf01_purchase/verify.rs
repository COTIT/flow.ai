//! WF-01 read-back verify (R3-§2b): V1 ids and lines exact in Rust (no arithmetic), V2/V3 money
//! in SQL against the approved request; then settle: succeeded (I3 `verify_match`), variance, or
//! mismatch → reconciliation + `target_conflict`. Never PATCH/PUT.

use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::execute::{Job, Loaded};
use crate::{
    error::AppError,
    integrations::{
        capability::TargetPo,
        links::{self, NewLink},
    },
    jobs::runner::Step,
    workflows::{
        events::{self, Actor},
        executions::{self, Patch, Status},
        incidents,
    },
};

/// Result of comparing a NetSuite PO with the approved snapshot.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// V1, V2 and V3 hold.
    Match,
    /// V1 and V2 hold; tax differs beyond tolerance or is missing (V3).
    Variance,
    /// V1 or V2 fails: the fields that differ.
    Mismatch(Vec<&'static str>),
}

fn dec(v: &Value) -> Option<Decimal> {
    v.as_str()?.parse().ok()
}

/// V1: vendor, subsidiary, currency, and lines as a multiset of (item, quantity, rate).
/// Unit is not compared: `units` is a string, id vs name UNVERIFIED (facts N1 l.168; L3).
fn v1(s: &Value, po: &TargetPo) -> Vec<&'static str> {
    let mut bad = vec![];
    for (name, want, got) in [
        ("vendor", &s["vendor"], &po.vendor_id),
        ("subsidiary", &s["company"], &po.subsidiary_id),
        ("currency", &s["currency"], &po.currency_id),
    ] {
        if want["target_id"].as_str() != Some(got.as_str()) {
            bad.push(name);
        }
    }
    let mut want: Vec<(String, Option<Decimal>, Option<Decimal>)> = s["lines"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|l| {
            (
                l["product"]["target_id"].as_str().unwrap_or("").to_owned(),
                dec(&l["quantity"]),
                dec(&l["price_unit"]),
            )
        })
        .collect();
    let mut got: Vec<(String, Option<Decimal>, Option<Decimal>)> = po
        .lines
        .iter()
        .map(|l| (l.item_id.clone(), Some(l.quantity), Some(l.rate)))
        .collect();
    want.sort();
    got.sort();
    if want != got {
        bad.push("lines");
    }
    bad
}

/// Compare (V1 in Rust, V2/V3 in SQL `numeric`, tolerance from the pair config).
pub(super) async fn compare(
    db: &mut PgConnection,
    ws: Uuid,
    e: &Loaded,
    po: &TargetPo,
) -> Result<Verdict, AppError> {
    let mut bad = v1(&e.snapshot, po);
    let money = sqlx::query!(
        r#"SELECT ($3::numeric * power(10::numeric, r.currency_decimals) = r.amount_minor) AS "untaxed_ok?",
                  (abs($4::numeric * power(10::numeric, r.currency_decimals)
                       - (r.snapshot->>'amount_tax_minor')::numeric)
                   <= coalesce((p.config->>'tax_tolerance_minor')::numeric, 0)) AS "tax_ok?"
           FROM approval_requests r
           JOIN integration_pairs p ON p.workspace_id = r.workspace_id AND p.id = r.pair_id
           WHERE r.workspace_id = $1 AND r.id = $2"#,
        ws,
        e.approval_id,
        po.untaxed,
        po.tax,
    )
    .fetch_one(&mut *db)
    .await?;
    if money.untaxed_ok != Some(true) {
        bad.push("untaxed");
    }
    Ok(if !bad.is_empty() {
        Verdict::Mismatch(bad)
    } else if money.tax_ok == Some(true) {
        Verdict::Match
    } else {
        Verdict::Variance
    })
}

fn link<'a>(e: &'a Loaded, po_id: &'a str) -> NewLink<'a> {
    NewLink {
        correlation_id: &e.correlation_id,
        source_connection_id: e.source_connection_id,
        source_model: "purchase.order",
        source_id: e.snapshot["source"]["id"].as_str().unwrap_or(""),
        target_connection_id: e.target_connection_id,
        target_type: "purchaseOrder",
        target_id: Some(po_id),
    }
}

/// `succeeded` / `succeeded_with_variance` from `running` (fenced) or `awaiting_reconciliation`,
/// with the link and, on variance, a `tax_variance` incident. False when the row moved.
pub(super) async fn succeed(
    tx: &mut PgConnection,
    ws: Uuid,
    token: Uuid,
    e: &Loaded,
    po_id: &str,
    variance: bool,
) -> Result<bool, AppError> {
    let target = e.target_ref(po_id);
    if !executions::finish_succeeded(tx, ws, e.id, token, &target, variance).await? {
        return Ok(false);
    }
    links::upsert(tx, ws, &link(e, po_id), "linked").await?;
    if variance {
        let d =
            json!({"target_id": po_id, "detail": "NetSuite tax differs from the Odoo estimate."});
        incidents::open(tx, ws, "tax_variance", &e.subject_ref, e.refs(), d).await?;
        let p = json!({"kind": "tax_variance"});
        events::record(&mut *tx, ws, e.at(), "incident_opened", p, Actor::System).await?;
    }
    Ok(true)
}

/// Mismatch evidence: event, link `conflict`, and (unless a human already decided) a
/// `target_conflict` incident.
pub(super) async fn flag_mismatch(
    tx: &mut PgConnection,
    ws: Uuid,
    e: &Loaded,
    po_id: &str,
    fields: &[&str],
    incident: bool,
) -> Result<(), AppError> {
    let p = json!({"target_id": po_id, "fields": fields});
    events::record(&mut *tx, ws, e.at(), "verify_mismatch", p, Actor::System).await?;
    links::upsert(tx, ws, &link(e, po_id), "conflict").await?;
    if !incident {
        return Ok(());
    }
    let d = json!({"target_id": po_id, "fields": fields});
    incidents::open(tx, ws, "target_conflict", &e.subject_ref, e.refs(), d).await?;
    let p = json!({"kind": "target_conflict"});
    events::record(&mut *tx, ws, e.at(), "incident_opened", p, Actor::System).await?;
    Ok(())
}

/// Compare a found PO and settle the execution (execute: fenced on `j.token`; reconcile:
/// the row is `awaiting_reconciliation`, unowned).
pub(super) async fn settle(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    po: TargetPo,
) -> Result<Step, AppError> {
    let mut tx = db.begin().await?;
    match compare(&mut tx, j.ws, e, &po).await? {
        v @ (Verdict::Match | Verdict::Variance) => {
            let variance = matches!(v, Verdict::Variance);
            if !succeed(&mut tx, j.ws, j.token, e, &po.id, variance).await? {
                // Row moved or another owner holds it: keep the read-back as evidence only.
                drop(tx);
                let ev = json!({"target_id": po.id, "variance": variance});
                events::record(db, j.ws, e.at(), "stale_worker_result", ev, Actor::System).await?;
                return Ok(Step::Done);
            }
        }
        Verdict::Mismatch(fields) => {
            if e.status == "running" || e.status == "queued" {
                let patch = Patch {
                    error_class: Some("conflict".into()),
                    error_detail: Some(
                        "The order in NetSuite differs from the approved order.".into(),
                    ),
                    target_ref: Some(e.target_ref(&po.id)),
                    ..Patch::default()
                };
                let to = Status::AwaitingReconciliation;
                let running = &[Status::Running];
                if !executions::transition(&mut *tx, j.ws, e.id, j.token, running, to, patch)
                    .await?
                {
                    return Ok(Step::Done);
                }
            }
            flag_mismatch(&mut tx, j.ws, e, &po.id, &fields, true).await?;
        }
    }
    tx.commit().await?;
    Ok(Step::Done)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::integrations::capability::NewPoLine;

    fn po(rate: &str) -> TargetPo {
        TargetPo {
            id: "1".into(),
            external_id: None,
            vendor_id: "1187".into(),
            subsidiary_id: "3".into(),
            currency_id: "1".into(),
            lines: vec![NewPoLine {
                item_id: "501".into(),
                unit_id: Some("Units".into()),
                quantity: Decimal::from_str("2").unwrap(),
                rate: Decimal::from_str(rate).unwrap(),
            }],
            untaxed: None,
            tax: None,
        }
    }

    #[test]
    fn v1_exact_decimals_scale_free_unit_ignored() {
        let s = json!({"vendor": {"target_id": "1187"}, "company": {"target_id": "3"},
            "currency": {"target_id": "1"},
            "lines": [{"product": {"target_id": "501"}, "quantity": "2.00000000", "price_unit": "150.00000000"}]});
        assert!(v1(&s, &po("150")).is_empty());
        assert_eq!(v1(&s, &po("150.0000001")), vec!["lines"]);
        let mut other = po("150");
        other.vendor_id = "9".into();
        assert_eq!(v1(&s, &other), vec!["vendor"]);
    }
}
