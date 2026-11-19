//! LIVE smoke kit (PLAN REV 5): one `#[ignore]` test per gate L1–L6, run against real sandboxes
//! through the real adapters. Missing `LIVE_*` env → SKIP (green). NetSuite writes need
//! `LIVE_NETSUITE_ENVIRONMENT=sandbox` + `LIVE_NETSUITE_CONFIRM_SANDBOX=yes` (sandbox host form is
//! UNVERIFIED, facts netsuite-rest.md l.27). Odoo is read only. Asserts = documented contract;
//! `=== OBSERVED ===` blocks = docs silent, copy into `docs/integrations/facts/*.md`.
//! Runbook: `docs/integrations/SANDBOX.md`.

use std::{str::FromStr, time::Duration};

use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    integrations::{
        capability::{
            CapabilityCheck, ConnectionProbe, CreateOutcome, Cursor, NewPo, NewPoLine,
            ProviderError, PurchaseOrderSink, PurchaseRequest, PurchaseRequestSource, SourceState,
            TargetPo,
        },
        providers::{netsuite, odoo},
    },
    outbound::Outbound,
};

/// JSON keys whose values are never printed.
const SECRET_KEYS: &[&str] = &[
    "access_token",
    "client_assertion",
    "authorization",
    "api_key",
    "private_key",
];

/// Text with every known secret value and every `SECRET_KEYS` JSON value replaced.
fn redact(text: &str, secrets: &[String]) -> String {
    fn walk(v: &mut Value) {
        match v {
            Value::Object(m) => {
                for (k, val) in m.iter_mut() {
                    if SECRET_KEYS.contains(&k.to_ascii_lowercase().as_str()) {
                        *val = json!("[REDACTED]");
                    } else {
                        walk(val);
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(walk),
            _ => {}
        }
    }
    let mut out = match serde_json::from_str::<Value>(text) {
        Ok(mut v) => {
            walk(&mut v);
            serde_json::to_string_pretty(&v).unwrap_or_default()
        }
        Err(_) => text.to_owned(),
    };
    for s in secrets.iter().filter(|s| s.len() >= 8) {
        out = out.replace(s.as_str(), "[REDACTED]");
    }
    out
}

/// The only print path: a labelled, redacted block for the facts pack.
fn observe(gate: &str, title: &str, body: &str, secrets: &[String]) {
    println!(
        "=== OBSERVED {gate}: {title} ===\n{}\n=== END {gate} ===",
        redact(body, secrets)
    );
}

fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// All `names` set, or None after a SKIP line naming the missing ones.
fn need(gate: &str, names: &[&str]) -> Option<Vec<String>> {
    let missing: Vec<&str> = names.iter().copied().filter(|n| var(n).is_none()).collect();
    if !missing.is_empty() {
        println!("SKIP {gate}: not set: {}", missing.join(", "));
        return None;
    }
    Some(names.iter().filter_map(|n| var(n)).collect())
}

/// Write guard: both sandbox switches, else SKIP.
fn sandbox_confirmed(gate: &str) -> bool {
    let env_ok = var("LIVE_NETSUITE_ENVIRONMENT").as_deref() == Some("sandbox");
    let confirm_ok = var("LIVE_NETSUITE_CONFIRM_SANDBOX").as_deref() == Some("yes");
    if !(env_ok && confirm_ok) {
        println!(
            "SKIP {gate}: writes to NetSuite need LIVE_NETSUITE_ENVIRONMENT=sandbox and \
             LIVE_NETSUITE_CONFIRM_SANDBOX=yes (sandbox host form is UNVERIFIED)"
        );
    }
    env_ok && confirm_ok
}

fn outbound() -> Outbound {
    Outbound::new(&[]).expect("outbound client")
}

fn err_json(e: &ProviderError) -> Value {
    json!({"class": e.class, "code": e.code, "http_status": e.http_status, "detail": e.detail})
}

fn checks_json(checks: &[CapabilityCheck]) -> String {
    serde_json::to_string_pretty(checks).unwrap_or_default()
}

// ---------- Odoo (L1) ----------

fn state_name(s: &SourceState) -> String {
    match s {
        SourceState::Draft => "draft".into(),
        SourceState::Sent => "sent".into(),
        SourceState::ToApprove => "to approve".into(),
        SourceState::Confirmed => "confirmed".into(),
        SourceState::Cancelled => "cancelled".into(),
        SourceState::Other(o) => format!("other:{o}"),
    }
}

fn request_json(r: &PurchaseRequest) -> Value {
    json!({
        "id": r.id, "state": state_name(&r.state), "company_id": r.company_id,
        "vendor_id": r.vendor_id, "currency_id": r.currency_id, "currency_ref": r.currency_ref,
        "currency_decimals": r.currency_decimals, "buyer_id": r.buyer_id,
        "date_order": r.date_order, "amount_untaxed": r.amount_untaxed.to_string(),
        "amount_tax": r.amount_tax.to_string(), "amount_total": r.amount_total.to_string(),
        "labels": r.labels,
        "lines": r.lines.iter().map(|l| json!({
            "id": l.id, "product_id": l.product_id, "uom_id": l.uom_id,
            "quantity": l.quantity.to_string(), "price_unit": l.price_unit.to_string(),
            "subtotal": l.subtotal.to_string(), "tax_ids": l.tax_ids,
            "date_planned": l.date_planned,
        })).collect::<Vec<_>>(),
    })
}

/// L1: Odoo end-to-end read on a Custom-plan v19 instance (facts odoo-19.md O1, O2, O5).
#[tokio::test]
#[ignore = "LIVE L1: needs a real Odoo 19; see docs/integrations/SANDBOX.md"]
async fn live_l1_odoo_read() {
    let Some(v) = need(
        "L1",
        &[
            "LIVE_ODOO_BASE_URL",
            "LIVE_ODOO_API_KEY",
            "LIVE_ODOO_COMPANY_ID",
        ],
    ) else {
        return;
    };
    let (base, key, company) = (v[0].clone(), v[1].clone(), v[2].clone());
    let secrets = vec![key.clone()];
    let client = odoo::Client::with_key(
        outbound(),
        base.trim_end_matches('/').to_owned(),
        var("LIVE_ODOO_DATABASE"),
        &key,
        Duration::from_secs(30),
    );

    let checks = client.test().await;
    observe("L1", "capability checks", &checks_json(&checks), &secrets);
    assert!(
        checks.iter().all(|c| c.ok),
        "L1: every Odoo capability check must pass"
    );

    // O5 + SANDBOX.md: two_step with amount 0 is the WF-01 control.
    let controls = client
        .read_controls(std::slice::from_ref(&company))
        .await
        .expect("L1 read_controls");
    let shown: Vec<Value> = controls
        .iter()
        .map(|c| json!({"company_id": c.company_id, "two_step": c.two_step, "threshold_zero": c.threshold_zero}))
        .collect();
    observe(
        "L1",
        "res.company controls",
        &json!(shown).to_string(),
        &secrets,
    );
    assert_eq!(controls.len(), 1, "L1: company {company} must be readable");
    assert!(
        controls[0].two_step && controls[0].threshold_zero,
        "L1: company {company} must have po_double_validation=two_step and amount 0"
    );

    let epoch = Cursor {
        ts: chrono::DateTime::UNIX_EPOCH,
        id: 0,
    };
    let page = client
        .list_changed(std::slice::from_ref(&company), &epoch, 5)
        .await
        .expect("L1 list_changed");
    let changed: Vec<Value> = page
        .items
        .iter()
        .map(|r| json!({"id": r.id, "changed_at": r.changed_at.to_rfc3339()}))
        .collect();
    observe(
        "L1",
        "list_changed from epoch, limit 5 (write_date precision, O1)",
        &json!({"items": changed, "next": page.next.map(|c| json!({"ts": c.ts.to_rfc3339(), "id": c.id}))}).to_string(),
        &secrets,
    );

    let ids = client
        .list_eligible_ids(&company, 0, 5)
        .await
        .expect("L1 list_eligible_ids");
    observe(
        "L1",
        "list_eligible_ids, limit 5",
        &json!(ids).to_string(),
        &secrets,
    );

    let Some(rfq) = var("LIVE_ODOO_RFQ_ID").or_else(|| ids.first().cloned()) else {
        println!("L1: no eligible RFQ found and LIVE_ODOO_RFQ_ID not set; read() not exercised");
        return;
    };
    let (found, gone) = client
        .read(std::slice::from_ref(&rfq))
        .await
        .expect("L1 read");
    let shown: Vec<Value> = found.iter().map(request_json).collect();
    observe(
        "L1",
        &format!("read RFQ {rfq} (many2one shape, decimals, O6 totals)"),
        &json!({"found": shown, "gone": gone}).to_string(),
        &secrets,
    );
    assert_eq!(found.len(), 1, "L1: RFQ {rfq} must be read");
    assert!(
        !found[0].lines.is_empty(),
        "L1: RFQ {rfq} must have at least one product line"
    );
}

// ---------- NetSuite (L2–L6) ----------

/// NetSuite client from env, plus the values never to print.
fn netsuite_client(gate: &str) -> Option<(netsuite::Client, Vec<String>)> {
    let v = need(
        gate,
        &[
            "LIVE_NETSUITE_REST_DOMAIN",
            "LIVE_NETSUITE_CLIENT_ID",
            "LIVE_NETSUITE_CERTIFICATE_ID",
            "LIVE_NETSUITE_PRIVATE_KEY_PATH",
        ],
    )?;
    let pem = match std::fs::read_to_string(&v[3]) {
        Ok(p) => p,
        Err(e) => panic!("{gate}: LIVE_NETSUITE_PRIVATE_KEY_PATH unreadable: {e}"),
    };
    // Each key line on its own, so a partial echo is caught too.
    let mut secrets: Vec<String> = pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("-----"))
        .map(str::to_owned)
        .collect();
    secrets.push(pem.clone());
    let domain = v[0]
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_owned();
    let client =
        netsuite::Client::with_pem(outbound(), &format!("https://{domain}"), &v[1], &v[2], pem);
    Some((client, secrets))
}

/// Ids a test PO is built from.
struct PoIds {
    vendor: String,
    subsidiary: String,
    currency: String,
    item: String,
    location: Option<String>,
}

/// Write-side setup: client, ids, sandbox guard; None = SKIP.
fn netsuite_writer(gate: &str) -> Option<(netsuite::Client, Vec<String>, PoIds)> {
    let v = need(
        gate,
        &[
            "LIVE_NETSUITE_VENDOR_ID",
            "LIVE_NETSUITE_SUBSIDIARY_ID",
            "LIVE_NETSUITE_CURRENCY_ID",
            "LIVE_NETSUITE_ITEM_ID",
        ],
    )?;
    let (client, secrets) = netsuite_client(gate)?;
    if !sandbox_confirmed(gate) {
        return None;
    }
    let ids = PoIds {
        vendor: v[0].clone(),
        subsidiary: v[1].clone(),
        currency: v[2].clone(),
        item: v[3].clone(),
        location: var("LIVE_NETSUITE_LOCATION_ID"),
    };
    Some((client, secrets, ids))
}

/// `VERZO-SMOKE-<gate>-<8 random hex of a v7 uuid>`: inside the documented eid charset (N2 l.194).
fn smoke_eid(gate: &str) -> String {
    format!(
        "VERZO-SMOKE-{gate}-{}",
        &Uuid::now_v7().simple().to_string()[24..]
    )
}

/// One-line PO: quantity 1 at rate 1.00.
fn new_po(eid: &str, ids: &PoIds, currency: &str, location: Option<String>) -> NewPo {
    NewPo {
        external_id: eid.to_owned(),
        vendor_id: ids.vendor.clone(),
        subsidiary_id: ids.subsidiary.clone(),
        currency_id: currency.to_owned(),
        location_id: location,
        lines: vec![NewPoLine {
            item_id: ids.item.clone(),
            unit_id: None,
            quantity: Decimal::ONE,
            rate: Decimal::from_str("1.00").expect("decimal"),
        }],
    }
}

fn outcome_json(o: &CreateOutcome) -> Value {
    match o {
        CreateOutcome::Created { id, location } => {
            json!({"outcome": "created", "id": id, "location": location})
        }
        CreateOutcome::Ambiguous(e) => json!({"outcome": "ambiguous", "error": err_json(e)}),
        CreateOutcome::Rejected(e) => json!({"outcome": "rejected", "error": err_json(e)}),
        CreateOutcome::Conflict(e) => json!({"outcome": "conflict", "error": err_json(e)}),
    }
}

/// Created id, printed for manual cleanup (adapters have no delete).
fn created_id(gate: &str, eid: &str, o: &CreateOutcome) -> Option<String> {
    match o {
        CreateOutcome::Created { id, .. } => {
            println!(
                "CLEANUP {gate}: purchaseOrder internal id {id}, external id {eid}; delete manually"
            );
            Some(id.clone())
        }
        CreateOutcome::Ambiguous(_) => {
            println!(
                "CLEANUP {gate}: create was ambiguous; search purchaseOrder externalId {eid} and delete manually"
            );
            None
        }
        _ => None,
    }
}

/// Create that must succeed; returns the internal id.
async fn create_ok(
    gate: &str,
    client: &netsuite::Client,
    po: &NewPo,
    secrets: &[String],
) -> String {
    let o = client.create(po).await;
    observe(gate, "create", &outcome_json(&o).to_string(), secrets);
    created_id(gate, &po.external_id, &o)
        .unwrap_or_else(|| panic!("{gate}: create must return 204 + Location (N1 l.115, l.125)"))
}

fn target_json(t: &TargetPo) -> Value {
    json!({
        "id": t.id, "external_id": t.external_id, "vendor_id": t.vendor_id,
        "subsidiary_id": t.subsidiary_id, "currency_id": t.currency_id,
        "untaxed": t.untaxed.map(|d| d.to_string()), "tax": t.tax.map(|d| d.to_string()),
        "lines": t.lines.iter().map(|l| json!({
            "item_id": l.item_id, "unit_id": l.unit_id,
            "quantity": l.quantity.to_string(), "rate": l.rate.to_string(),
        })).collect::<Vec<_>>(),
    })
}

/// Raw GET of one PO, body printed as is (redacted).
async fn raw_po(
    gate: &str,
    client: &netsuite::Client,
    path_id: &str,
    secrets: &[String],
) -> Result<Value, ProviderError> {
    let path = format!(
        "{}/purchaseOrder/{path_id}?expandSubResources=true",
        netsuite::RECORD_PATH
    );
    let r = client.get_json(&path).await;
    let body = match &r {
        Ok(v) => v.to_string(),
        Err(e) => err_json(e).to_string(),
    };
    observe(
        gate,
        &format!("GET purchaseOrder/{path_id}"),
        &body,
        secrets,
    );
    r
}

/// L2: create, find by externalId, then POST the same externalId again (N2, N3).
#[tokio::test]
#[ignore = "LIVE L2: writes to a NetSuite sandbox; see docs/integrations/SANDBOX.md"]
async fn live_l2_netsuite_create_and_duplicate_external_id() {
    let Some((client, secrets, ids)) = netsuite_writer("L2") else {
        return;
    };
    let eid = smoke_eid("L2");
    let po = new_po(&eid, &ids, &ids.currency, ids.location.clone());
    let first = create_ok("L2", &client, &po, &secrets).await;

    // q= read-after-write is UNVERIFIED (N2 l.206): try a few times, record how many.
    let mut found = None;
    for attempt in 1..=5 {
        match client.find_by_external_id(&eid).await {
            Ok(Some(t)) => {
                observe(
                    "L2",
                    &format!("find_by_external_id hit on attempt {attempt} (2 s apart)"),
                    &target_json(&t).to_string(),
                    &secrets,
                );
                found = Some(t);
                break;
            }
            Ok(None) => println!("L2: find_by_external_id attempt {attempt}: not found yet"),
            Err(e) => observe(
                "L2",
                &format!("find_by_external_id error on attempt {attempt}"),
                &err_json(&e).to_string(),
                &secrets,
            ),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let found = found.expect("L2: q=externalId IS must find the created PO (N2 l.195)");
    assert_eq!(found.id, first, "L2: lookup must return the created id");
    assert_eq!(found.external_id.as_deref(), Some(eid.as_str()));

    // N2 l.201: GET by eid: on purchaseOrder (RAB types {id} as int32).
    let _ = raw_po("L2", &client, &format!("eid:{eid}"), &secrets).await;

    // N3: POST again with the same externalId; docs are silent.
    let again = client.create(&po).await;
    observe(
        "L2",
        "N3 second POST with the same externalId",
        &outcome_json(&again).to_string(),
        &secrets,
    );
    if let CreateOutcome::Created { id: second, .. } = &again {
        println!(
            "DUPLICATE CREATED — delete manually: first purchaseOrder id {first}, second id {second}, external id {eid}"
        );
    }
}

/// L3: which purchaseOrder fields are required under this account's features (N1).
#[tokio::test]
#[ignore = "LIVE L3: writes to a NetSuite sandbox; see docs/integrations/SANDBOX.md"]
async fn live_l3_netsuite_required_fields() {
    let Some((client, secrets, ids)) = netsuite_writer("L3") else {
        return;
    };
    let eid = smoke_eid("L3");
    let po = new_po(&eid, &ids, &ids.currency, ids.location.clone());
    let id = create_ok("L3", &client, &po, &secrets).await;

    // Item sublist writability (N1 l.104-107): the documented sample writes it.
    let read = client.read(&id).await.expect("L3 read back");
    observe(
        "L3",
        "read back (normalised)",
        &target_json(&read).to_string(),
        &secrets,
    );
    assert_eq!(read.lines.len(), 1, "L3: the item line must be written");
    assert_eq!(read.lines[0].item_id, ids.item);
    let _ = raw_po("L3", &client, &id, &secrets).await;

    // Without location: required or not depends on features (N1 l.183).
    let eid2 = smoke_eid("L3");
    let o = client
        .create(&new_po(&eid2, &ids, &ids.currency, None))
        .await;
    observe(
        "L3",
        "create without location",
        &outcome_json(&o).to_string(),
        &secrets,
    );
    created_id("L3", &eid2, &o);
}

/// L4: approval routing on a REST-created PO (N4). Mandatory production gate.
#[tokio::test]
#[ignore = "LIVE L4: writes to a NetSuite sandbox; see docs/integrations/SANDBOX.md"]
async fn live_l4_netsuite_approval_routing() {
    let Some((client, secrets, ids)) = netsuite_writer("L4") else {
        return;
    };
    let eid = smoke_eid("L4");
    let po = new_po(&eid, &ids, &ids.currency, ids.location.clone());
    let id = create_ok("L4", &client, &po, &secrets).await;
    let raw = raw_po("L4", &client, &id, &secrets)
        .await
        .expect("L4: GET of the created PO must succeed");
    let fields = json!({
        "approvalStatus": raw.get("approvalStatus"),
        "nextApprover": raw.get("nextApprover"),
        "supervisorApproval": raw.get("supervisorApproval"),
        "status": raw.get("status"),
        "orderStatus": raw.get("orderStatus"),
    });
    observe(
        "L4",
        "N4 approval fields after REST create",
        &fields.to_string(),
        &secrets,
    );
    // RAB lists approvalStatus (N4 l.235); missing means routing can't be checked: FAIL.
    assert!(
        raw.get("approvalStatus").is_some(),
        "L4: approvalStatus missing on purchaseOrder {id}; approval routing is unproven"
    );
}

/// L5: M2M token and reads on the least-privilege role (JWT, N6). Read only.
#[tokio::test]
#[ignore = "LIVE L5: needs a NetSuite account; see docs/integrations/SANDBOX.md"]
async fn live_l5_netsuite_m2m_least_privilege() {
    let Some((client, secrets)) = netsuite_client("L5") else {
        return;
    };
    let checks = client.test().await;
    observe("L5", "capability checks", &checks_json(&checks), &secrets);
    assert!(
        checks.iter().all(|c| c.ok),
        "L5: token and purchaseOrder read must pass on the least-privilege role"
    );
    // Admin-only endpoint (N5 l.255): records the REST permission-failure shape (N6 l.287).
    let r = client
        .get_json("/services/rest/system/v1/governanceLimits")
        .await;
    let body = match &r {
        Ok(v) => v.to_string(),
        Err(e) => err_json(e).to_string(),
    };
    observe(
        "L5",
        "GET governanceLimits as non-admin (N6 permission error)",
        &body,
        &secrets,
    );
}

/// L6: currency the vendor is not set up for (docs silent).
#[tokio::test]
#[ignore = "LIVE L6: writes to a NetSuite sandbox; see docs/integrations/SANDBOX.md"]
async fn live_l6_netsuite_currency_vendor_restriction() {
    let Some(other) = need("L6", &["LIVE_NETSUITE_OTHER_CURRENCY_ID"]) else {
        return;
    };
    let Some((client, secrets, ids)) = netsuite_writer("L6") else {
        return;
    };
    let eid = smoke_eid("L6");
    let o = client
        .create(&new_po(&eid, &ids, &other[0], ids.location.clone()))
        .await;
    observe(
        "L6",
        "create with a currency not on the vendor",
        &outcome_json(&o).to_string(),
        &secrets,
    );
    created_id("L6", &eid, &o);
}

#[test]
fn printed_output_never_holds_secrets() {
    let token = "eyJhbGciOiJFUzI1NiJ9.secret-token-value";
    let sample = json!({
        "access_token": token,
        "client_assertion": "aaa.bbb.ccc",
        "nested": {"Authorization": "Bearer xyz-123456789"},
        "detail": format!("echo {token}"),
        "approvalStatus": {"id": "1"},
    })
    .to_string();
    let out = redact(&sample, &[token.to_owned()]);
    assert!(!out.contains(token) && !out.contains("secret-token-value"));
    assert!(!out.contains("aaa.bbb.ccc") && !out.contains("xyz-123456789"));
    assert!(out.contains("approvalStatus"), "{out}");
    // Plain text: only the known value goes.
    let line = redact(&format!("key={token} ok"), &[token.to_owned()]);
    assert_eq!(line, "key=[REDACTED] ok");
}
