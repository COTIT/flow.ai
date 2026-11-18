//! WF-01 intake against the in-process fake Odoo (real C3a client, real JSON-2 calls). Names
//! carry the PLAN §9 / R3-§9 acceptance ids. "Verified against the documented contract", not live.

use chrono::{TimeZone, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    activation,
    evaluate::{self, Outcome, PairCtx, Seen},
    fake_odoo::{self, Line, Order, Shared},
    full_scan, poll, sweep, tick,
};
use crate::{
    error::AppError,
    integrations::{
        capability::PurchaseRequestSource, pairs::repo as pairs, providers::SourceClient,
    },
    jobs::runner::Step,
    modules::approvals::repo as approvals,
    workflows::testkit::{self, Kit},
};

pub(super) const T0: &str = "2026-11-17 10:00:00";

pub(super) struct W {
    pub(super) pool: PgPool,
    pub(super) k: Kit,
    pub(super) client: SourceClient,
    pub(super) odoo: Shared,
}

pub(super) async fn map(pool: &PgPool, k: &Kit, entity: &str, source: &str, target: &str) {
    sqlx::query(
        "INSERT INTO integration_mappings (workspace_id, pair_id, entity, source_id, target_id,
             source_label, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(k.ws)
    .bind(k.pair)
    .bind(entity)
    .bind(source)
    .bind(target)
    .bind(format!("{entity} {source}"))
    .bind(k.requester)
    .execute(pool)
    .await
    .unwrap();
}

/// Active pair (activated 2026-11-17 00:00), both connections active, every mapping of
/// `Order::draft`, EUR policy, an approver holding `integration.approve_high`.
pub(super) async fn world(pool: PgPool) -> W {
    let k = testkit::kit(&pool, "w").await;
    sqlx::query("UPDATE integration_connections SET status = 'active' WHERE workspace_id = $1")
        .bind(k.ws)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE integration_pairs SET status = 'active', activated_at = $2 WHERE id = $1")
        .bind(k.pair)
        .bind(Utc.with_ymd_and_hms(2026, 11, 17, 0, 0, 0).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    for (e, s, t) in [
        ("company", "1", "3"),
        ("vendor", "42", "1187"),
        ("currency", "1", "1"),
        ("product", "9", "501"),
        ("uom", "1", "1"),
    ] {
        map(&pool, &k, e, s, t).await;
    }
    map(&pool, &k, "user", "2", &k.requester.to_string()).await;
    sqlx::query(
        "INSERT INTO approval_policies (workspace_id, workflow_key, currency_ref, threshold_minor, created_by)
         VALUES ($1, 'wf01_purchase', 'EUR', 1000000, $2)",
    )
    .bind(k.ws)
    .bind(k.requester)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "WITH m AS (INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)),
              t AS (INSERT INTO teams (workspace_id, name, description) VALUES ($1, 'T', 'd') RETURNING id),
              p AS (INSERT INTO team_permissions (team_id, permission) SELECT id, 'integration.approve_high' FROM t)
         INSERT INTO team_members (team_id, user_id) SELECT id, $2 FROM t",
    )
    .bind(k.ws)
    .bind(k.approver)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)")
        .bind(k.ws)
        .bind(k.requester)
        .execute(&pool)
        .await
        .unwrap();
    let (client, odoo) = fake_odoo::start().await;
    W {
        pool,
        k,
        client,
        odoo,
    }
}

impl W {
    pub(super) async fn pair(&self) -> PairCtx {
        let row = pairs::find(&self.pool, self.k.ws, self.k.pair)
            .await
            .unwrap()
            .unwrap();
        PairCtx::new(&row, true)
    }

    pub(super) fn put(&self, id: i64, o: Order) {
        self.odoo.lock().unwrap().orders.insert(id, o);
    }

    pub(super) fn edit(&self, id: i64, f: impl FnOnce(&mut Order)) {
        f(self.odoo.lock().unwrap().orders.get_mut(&id).unwrap());
    }

    pub(super) async fn poll(&self) -> Result<Step, AppError> {
        poll::poll_pair(&self.pool, &self.pair().await, &self.client).await
    }

    pub(super) async fn sweep(&self) {
        sweep::sweep_pair(&self.pool, &self.pair().await, &self.client)
            .await
            .unwrap();
    }

    pub(super) async fn scan(&self) {
        full_scan::scan_pair(&self.pool, &self.pair().await, &self.client)
            .await
            .unwrap();
    }

    /// `(revision, status, invalidated_reason, blocked_reasons)` of a subject, oldest first.
    pub(super) async fn history(&self, id: i64) -> Vec<(i32, String, Option<String>, Vec<String>)> {
        sqlx::query_as(
            "SELECT revision, status, invalidated_reason, blocked_reasons FROM approval_requests
             WHERE workspace_id = $1 AND subject_ref LIKE '%:purchase.order:' || $2 ORDER BY revision",
        )
        .bind(self.k.ws)
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    pub(super) async fn latest(&self, id: i64) -> (Uuid, String, Value) {
        sqlx::query_as(
            "SELECT id, fingerprint, snapshot FROM approval_requests
             WHERE workspace_id = $1 AND subject_ref LIKE '%:purchase.order:' || $2
             ORDER BY revision DESC LIMIT 1",
        )
        .bind(self.k.ws)
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    pub(super) async fn count(&self, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql)
            .bind(self.k.ws)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub(super) async fn requests(&self) -> i64 {
        self.count("SELECT count(*) FROM approval_requests WHERE workspace_id = $1")
            .await
    }

    /// Approve the latest revision the way C2 decide does, with its `queued` execution.
    pub(super) async fn approve(&self, id: i64, status: &str, phase: &str) -> Uuid {
        let (rid, fp, _) = self.latest(id).await;
        assert!(
            approvals::decide_cas(&self.pool, self.k.ws, rid, "approved", self.k.approver, &fp)
                .await
                .unwrap()
        );
        let attempts = i16::from(phase != "prechecking");
        sqlx::query_scalar(
            "INSERT INTO workflow_executions (workspace_id, approval_request_id, workflow_key,
                 correlation_id, status, phase, remote_attempts, target_ref)
             VALUES ($1, $2, 'wf01_purchase', 'vz-' || replace(gen_random_uuid()::text, '-', ''),
                 $3, $4, $5, CASE WHEN $3 LIKE 'succeeded%' THEN 'netsuite:x:purchaseOrder:1' END)
             RETURNING id",
        )
        .bind(self.k.ws)
        .bind(rid)
        .bind(status)
        .bind(phase)
        .bind(attempts)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    pub(super) async fn incidents(&self) -> Vec<(String, String)> {
        sqlx::query_as(
            "SELECT kind, subject_ref FROM workflow_incidents WHERE workspace_id = $1 AND status = 'open'
             ORDER BY kind",
        )
        .bind(self.k.ws)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

fn status(h: &[(i32, String, Option<String>, Vec<String>)]) -> Vec<(i32, &str, Option<&str>)> {
    h.iter()
        .map(|(r, s, i, _)| (*r, s.as_str(), i.as_deref()))
        .collect()
}

// PLAN 1: a new RFQ → exactly one pending request with the exact snapshot; a second poll keeps it.
#[sqlx::test]
async fn t01_new_rfq_one_pending_request_with_exact_snapshot(pool: PgPool) {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    assert_eq!(w.poll().await.unwrap(), Step::Done);
    assert_eq!(status(&w.history(17).await), [(1, "pending", None)]);
    let (_, fp, snap) = w.latest(17).await;
    assert_eq!(fp.len(), 64);
    let pair = w.pair().await;
    assert_eq!(snap["v"], 1);
    assert_eq!(snap["source"]["id"], "17");
    assert_eq!(snap["source"]["state"], "draft");
    assert_eq!(snap["source"]["connection_id"], pair.source.to_string());
    assert_eq!(snap["target"]["connection_id"], pair.target.to_string());
    assert_eq!(snap["company"]["target_id"], "3");
    assert_eq!(snap["vendor"]["target_id"], "1187");
    assert_eq!(snap["vendor"]["label"], "Vendor 42");
    assert_eq!(snap["currency"]["ref"], "EUR");
    assert_eq!(snap["currency"]["decimals"], 2);
    assert_eq!(snap["requester"]["user_id"], w.k.requester.to_string());
    let line = &snap["lines"][0];
    assert_eq!(line["source_line_id"], "55");
    assert_eq!(line["product"]["target_id"], "501");
    assert_eq!(line["quantity"], "2.00000000");
    assert_eq!(line["price_unit"], "150.00000000");
    assert_eq!(line["subtotal_minor"], 30000);
    assert_eq!(snap["amount_untaxed_minor"], 30000);
    assert_eq!(snap["amount_tax_minor"], 6300);
    assert_eq!(snap["amount_total_minor"], 36300);
    assert_eq!(snap["tax_note"], "odoo_estimate_target_calculates");
    assert_eq!(snap["mapping_ids"].as_object().unwrap().len(), 6);
    assert_eq!(snap["location"], Value::Null);
    let (amount, perm): (i64, String) = sqlx::query_as(
        "SELECT amount_minor, required_permission FROM approval_requests WHERE workspace_id = $1",
    )
    .bind(w.k.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!((amount, perm.as_str()), (30000, "integration.approve"));

    // Duplicate poll (the 120 s overlap re-reads it too): still one request, one proposal.
    w.poll().await.unwrap();
    w.poll().await.unwrap();
    assert_eq!(w.requests().await, 1);
    assert_eq!(
        w.count(
            "SELECT count(*) FROM workflow_events WHERE workspace_id = $1 AND kind = 'proposal'"
        )
        .await,
        1
    );
}

// PLAN 4: changed while pending → old invalidated, new revision; the stale fingerprint is refused.
#[sqlx::test]
async fn t04_change_while_pending_replaces_and_stale_fingerprint_cannot_approve(pool: PgPool) {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    w.poll().await.unwrap();
    let (old_id, old_fp, _) = w.latest(17).await;
    w.edit(17, |o| {
        o.write_date = "2026-11-17 10:05:00".into();
        o.lines[0].qty = "3.0";
        o.lines[0].subtotal = "450.00";
        o.untaxed = "450.00";
        o.tax = "94.50";
        o.total = "544.50";
    });
    w.poll().await.unwrap();
    assert_eq!(
        status(&w.history(17).await),
        [
            (1, "invalidated", Some("source_changed")),
            (2, "pending", None)
        ]
    );
    let (new_id, new_fp, _) = w.latest(17).await;
    assert_ne!(old_fp, new_fp);
    // The decide CAS (C2 route → 409 approval_stale / approval_already_decided) refuses both.
    for (id, fp) in [(new_id, &old_fp), (old_id, &old_fp)] {
        assert!(
            !approvals::decide_cas(&w.pool, w.k.ws, id, "approved", w.k.approver, fp)
                .await
                .unwrap()
        );
    }
    // A touch without a material change keeps the revision.
    w.edit(17, |o| o.write_date = "2026-11-17 10:06:00".into());
    w.poll().await.unwrap();
    assert_eq!(w.history(17).await.len(), 2);
}

// PLAN 3 (C4a half; the executor's cancel is C4b): intake never touches an approved request
// with a queued execution. After the executor cancels + invalidates, the next scan re-creates.
#[sqlx::test]
async fn t03_change_after_approval_left_to_the_executor(pool: PgPool) {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    w.poll().await.unwrap();
    let exec = w.approve(17, "queued", "prechecking").await;
    w.edit(17, |o| {
        o.write_date = "2026-11-17 10:05:00".into();
        o.vendor = 43;
    });
    w.poll().await.unwrap();
    w.sweep().await;
    w.scan().await;
    assert_eq!(status(&w.history(17).await), [(1, "approved", None)]);
    let s: String = sqlx::query_scalar("SELECT status FROM workflow_executions WHERE id = $1")
        .bind(exec)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(s, "queued");
    assert!(w.incidents().await.is_empty());

    // What C4b's pre-check does (R3-§5): execution cancelled, approval invalidated.
    sqlx::query(
        "UPDATE workflow_executions SET status = 'cancelled', reason = 'source_changed' WHERE id = $1",
    )
    .bind(exec)
    .execute(&w.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE approval_requests SET status = 'invalidated', invalidated_reason = 'source_changed',
             decided_by = NULL, decided_at = NULL WHERE workspace_id = $1",
    )
    .bind(w.k.ws)
    .execute(&w.pool)
    .await
    .unwrap();
    w.scan().await;
    let h = w.history(17).await;
    assert_eq!(
        status(&h),
        [
            (1, "invalidated", Some("source_changed")),
            (2, "blocked", None)
        ]
    );
    assert_eq!(h[1].3, vec!["unmapped:vendor:43"]);
}

// T29 detection part: a change while the write is in flight only raises an incident.
#[sqlx::test]
async fn t29_change_during_write_opens_incident_only(pool: PgPool) {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    w.poll().await.unwrap();
    w.approve(17, "running", "write_sent").await;
    w.edit(17, |o| o.lines[0].price = "151.0");
    w.sweep().await;
    w.sweep().await;
    assert_eq!(status(&w.history(17).await), [(1, "approved", None)]);
    let inc = w.incidents().await;
    assert_eq!(inc.len(), 1, "idempotent across sweeps");
    assert_eq!(inc[0].0, "source_changed_during_write");
}

// PLAN 9: every missing mapping listed at once; adding them → next sweep → pending.
#[sqlx::test]
async fn t09_missing_mappings_block_then_sweep_unblocks(pool: PgPool) {
    let w = world(pool).await;
    let mut o = Order::draft(T0, 55);
    o.vendor = 43;
    o.lines[0].product = 8;
    o.lines.push(Line {
        id: 56,
        product: 7,
        ..o.lines[0].clone()
    });
    o.untaxed = "600.00";
    o.total = "663.00";
    w.put(17, o);
    w.poll().await.unwrap();
    let h = w.history(17).await;
    assert_eq!(h[0].1, "blocked");
    assert_eq!(
        h[0].3,
        vec![
            "unmapped:vendor:43",
            "unmapped:product:8",
            "unmapped:product:7"
        ]
    );
    let (_, _, snap) = w.latest(17).await;
    assert_eq!(snap["vendor"]["target_id"], Value::Null);

    for (e, s, t) in [
        ("vendor", "43", "1188"),
        ("product", "8", "502"),
        ("product", "7", "503"),
    ] {
        map(&w.pool, &w.k, e, s, t).await;
    }
    w.sweep().await;
    let h = w.history(17).await;
    assert_eq!(status(&h), [(1, "pending", None)]);
    let (_, _, snap) = w.latest(17).await;
    assert_eq!(snap["vendor"]["target_id"], "1188");
    assert_eq!(snap["lines"][1]["product"]["target_id"], "503");
    // company, vendor, currency, user, uom 1 (both lines), products 8 and 7.
    assert_eq!(snap["mapping_ids"].as_object().unwrap().len(), 7);
}

// PLAN 10: no policy for the currency, unmapped subsidiary (company), amount not exact.
#[sqlx::test]
async fn t10_policy_subsidiary_and_exactness_block(pool: PgPool) {
    let w = world(pool).await;
    w.odoo.lock().unwrap().currencies.insert(2, ("USD", 2));
    map(&w.pool, &w.k, "currency", "2", "2").await;
    let mut usd = Order::draft(T0, 55);
    usd.currency = 2;
    w.put(17, usd);
    let mut inexact = Order::draft(T0, 56);
    inexact.tax = "63.005";
    inexact.total = "363.005";
    w.put(18, inexact);
    w.poll().await.unwrap();
    assert_eq!(w.history(17).await[0].3, vec!["no_policy:USD"]);
    assert_eq!(w.history(18).await[0].3, vec!["amount_not_exact"]);

    // The poll lists mapped companies only, so an unmapped subsidiary reaches intake through
    // a read (sweep/full scan of a company whose mapping was deleted).
    let mut other = Order::draft(T0, 57);
    other.company = 5;
    w.put(19, other);
    let (found, _) = w.client.read(&["19".into()]).await.unwrap();
    let pair = w.pair().await;
    let mut tx = w.pool.begin().await.unwrap();
    let out = evaluate::intake(&mut tx, &pair, "19", Seen::Found(&found[0]))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(out, Outcome::Created);
    assert_eq!(w.history(19).await[0].3, vec!["unmapped:company:5"]);
}

// T30 per O6 (strict rule A): sub-cent line base blocks before any write.
#[sqlx::test]
async fn t30_rate_precision_blocks(pool: PgPool) {
    let w = world(pool).await;
    let mut o = Order::draft(T0, 55);
    o.lines[0].qty = "3.0";
    o.lines[0].price = "0.333";
    o.lines[0].subtotal = "1.00";
    o.untaxed = "1.00";
    o.tax = "0.00";
    o.total = "1.00";
    w.put(17, o);
    w.poll().await.unwrap();
    assert_eq!(w.history(17).await[0].3, vec!["rate_precision"]);
}

// C6-O1: a down-payment line (no product) blocks only its RFQ, in words; the batch carries on.
#[sqlx::test]
async fn o1_downpayment_line_blocks_only_its_rfq(pool: PgPool) {
    let w = world(pool).await;
    let mut o = Order::draft(T0, 55);
    o.lines.push(Line {
        id: 56,
        product: 0,
        qty: "1.0",
        price: "50.0",
        subtotal: "50.00",
        taxes: vec![],
        downpayment: true,
        ..o.lines[0].clone()
    });
    o.untaxed = "350.00";
    o.total = "413.00";
    w.put(17, o);
    w.put(18, Order::draft(T0, 65));
    assert_eq!(w.poll().await.unwrap(), Step::Done);
    let h = w.history(17).await;
    assert_eq!(h[0].1, "blocked");
    assert_eq!(h[0].3, vec!["downpayment_line"], "no unmapped noise");
    let (_, _, snap) = w.latest(17).await;
    assert_eq!(
        snap["lines"].as_array().unwrap().len(),
        2,
        "money never dropped"
    );
    assert_eq!(snap["amount_untaxed_minor"], 35000);
    assert_eq!(status(&w.history(18).await), [(1, "pending", None)]);
}

// PLAN 17: a crash mid-page rolls the page back; the cursor stays; the rerun makes no duplicate.
#[sqlx::test]
async fn t17_crash_mid_page_keeps_cursor_and_no_duplicates(pool: PgPool) {
    let w = world(pool).await;
    for id in [101, 102, 103] {
        w.put(id, Order::draft(T0, id * 10));
    }
    sqlx::raw_sql(
        "CREATE FUNCTION wf01_test_crash() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN IF NEW.subject_ref LIKE '%:102' THEN RAISE EXCEPTION 'crash'; END IF; RETURN NEW; END $$;
         CREATE TRIGGER wf01_test_crash BEFORE INSERT ON approval_requests
         FOR EACH ROW EXECUTE FUNCTION wf01_test_crash();",
    )
    .execute(&w.pool)
    .await
    .unwrap();
    assert!(w.poll().await.is_err());
    assert_eq!(w.requests().await, 0, "page 101..103 rolled back as one");
    assert_eq!(
        w.count("SELECT count(*) FROM integration_poll_cursors WHERE workspace_id = $1")
            .await,
        0
    );
    sqlx::query("DROP TRIGGER wf01_test_crash ON approval_requests")
        .execute(&w.pool)
        .await
        .unwrap();
    w.poll().await.unwrap();
    w.poll().await.unwrap();
    assert_eq!(w.requests().await, 3);
}

// PLAN 18: 250 RFQs, equal write_date across page boundaries → none skipped, none twice.
#[sqlx::test]
async fn t18_equal_write_dates_across_pages_lose_nothing(pool: PgPool) {
    let w = world(pool).await;
    for id in 1..=250 {
        w.put(id, Order::draft(T0, 1000 + id));
    }
    assert_eq!(w.poll().await.unwrap(), Step::Done);
    assert_eq!(w.requests().await, 250);
    let lists = w
        .odoo
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|c| *c == "purchase.order/search_read")
        .count();
    assert_eq!(lists, 6, "3 cursor pages + 3 reads");
    let (ts, id): (chrono::DateTime<Utc>, i64) = sqlx::query_as(
        "SELECT cursor_ts, cursor_id FROM integration_poll_cursors WHERE workspace_id = $1",
    )
    .bind(w.k.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(
        (ts, id),
        (Utc.with_ymd_and_hms(2026, 11, 17, 10, 0, 0).unwrap(), 250)
    );
}

// PLAN 19 + T34: cancelled while pending → invalidated; confirmed after success → incident;
// deleted after success → incident.
#[sqlx::test]
async fn t19_t34_cancel_and_duplicate_obligation(pool: PgPool) {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    w.put(18, Order::draft(T0, 56));
    w.put(19, Order::draft(T0, 57));
    w.poll().await.unwrap();
    w.edit(17, |o| o.state = "cancel");
    w.sweep().await;
    assert_eq!(
        status(&w.history(17).await),
        [(1, "invalidated", Some("source_cancelled"))]
    );

    w.approve(18, "succeeded", "done").await;
    w.approve(19, "succeeded", "done").await;
    w.edit(18, |o| o.state = "purchase");
    w.odoo.lock().unwrap().orders.remove(&19);
    w.sweep().await;
    w.sweep().await;
    let src = w.pair().await.source;
    assert_eq!(
        w.incidents().await,
        [
            (
                "duplicate_obligation_risk".to_string(),
                format!("odoo:{src}:purchase.order:18")
            ),
            (
                "source_gone_after_execution".to_string(),
                format!("odoo:{src}:purchase.order:19")
            ),
        ]
    );
    assert_eq!(status(&w.history(18).await), [(1, "approved", None)]);
}

// T33: control drift pauses the pair with an incident and no intake; activation refuses.
#[sqlx::test]
async fn t33_control_drift_pauses_and_activation_refuses(pool: PgPool) {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    w.odoo
        .lock()
        .unwrap()
        .companies
        .insert(1, ("one_step", "0.0"));
    assert_eq!(w.poll().await.unwrap(), Step::Done);
    assert_eq!(w.requests().await, 0);
    let (st, why): (String, Option<String>) =
        sqlx::query_as("SELECT status, paused_reason FROM integration_pairs WHERE id = $1")
            .bind(w.k.pair)
            .fetch_one(&w.pool)
            .await
            .unwrap();
    assert_eq!(
        (st.as_str(), why.as_deref()),
        ("paused", Some("odoo_control_drift"))
    );
    let src = w.pair().await.source;
    assert_eq!(
        w.incidents().await,
        [(
            "odoo_control_drift".to_string(),
            format!("odoo:{src}:res.company:1")
        )]
    );
    assert!(
        super::ready(&w.pool, w.k.ws, w.k.pair)
            .await
            .unwrap()
            .is_none()
    );

    let code = |r: Result<(), AppError>| match r {
        Err(AppError::Reason { code, message, .. }) => (code, message),
        other => panic!("{other:?}"),
    };
    let label = vec![("1".to_string(), Some("Acme BV".to_string()))];
    let (c, m) = code(activation::verify(&w.client, &label).await);
    assert_eq!(c, "source_controls_unmet");
    assert!(m.contains("Acme BV"), "{m}");
    w.odoo
        .lock()
        .unwrap()
        .companies
        .insert(1, ("two_step", "5000.0"));
    assert_eq!(
        code(activation::verify(&w.client, &label).await).0,
        "source_controls_unmet"
    );
    let (c, _) = code(activation::verify(&w.client, &[("9".into(), None)]).await);
    assert_eq!(c, "source_controls_unmet", "company hidden or unknown");
    assert_eq!(
        code(activation::verify(&w.client, &[]).await).0,
        "source_controls_unmet"
    );
    w.odoo
        .lock()
        .unwrap()
        .companies
        .insert(1, ("two_step", "0.0"));
    activation::verify(&w.client, &label).await.unwrap();
}

// T35: a late commit carrying an older write_date is missed by the cursor, caught by the scan.
#[sqlx::test]
async fn t35_full_scan_catches_a_late_commit(pool: PgPool) {
    let w = world(pool).await;
    w.put(17, Order::draft("2026-11-17 10:00:00", 55));
    w.put(18, Order::draft("2026-11-17 11:00:00", 56));
    w.poll().await.unwrap();
    w.edit(17, |o| {
        o.write_date = "2026-11-17 09:00:00".into();
        o.vendor = 43;
    });
    w.poll().await.unwrap();
    assert_eq!(w.history(17).await.len(), 1, "older than cursor − overlap");
    w.scan().await;
    assert_eq!(
        status(&w.history(17).await),
        [
            (1, "invalidated", Some("source_changed")),
            (2, "blocked", None)
        ]
    );
    w.scan().await;
    assert_eq!(w.history(17).await.len(), 2);
    assert_eq!(w.history(18).await.len(), 1);
}

// T36: rejected with the same fingerprint → no new request (one skip event); resubmit → +1.
#[sqlx::test]
async fn t36_rejected_same_fingerprint_skips_until_resubmit(pool: PgPool) {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    w.poll().await.unwrap();
    let (id, fp, _) = w.latest(17).await;
    assert!(
        approvals::decide_cas(&w.pool, w.k.ws, id, "rejected", w.k.approver, &fp)
            .await
            .unwrap()
    );
    for at in ["2026-11-17 10:01:00", "2026-11-17 10:02:00"] {
        w.edit(17, |o| o.write_date = at.into());
        w.poll().await.unwrap();
        w.scan().await;
    }
    assert_eq!(status(&w.history(17).await), [(1, "rejected", None)]);
    assert_eq!(
        w.count(
            "SELECT count(*) FROM workflow_events WHERE workspace_id = $1
             AND kind = 'skipped_previously_decided'"
        )
        .await,
        1
    );
    let again = approvals::insert_resubmission(&w.pool, w.k.ws, id, w.k.requester)
        .await
        .unwrap();
    assert!(again.is_some());
    w.edit(17, |o| o.write_date = "2026-11-17 10:03:00".into());
    w.poll().await.unwrap();
    assert_eq!(
        status(&w.history(17).await),
        [(1, "rejected", None), (2, "pending", None)]
    );
    // A real change after the rejection makes a new revision without a resubmit.
    w.edit(17, |o| {
        o.write_date = "2026-11-17 10:04:00".into();
        o.vendor = 43;
    });
    w.poll().await.unwrap();
    assert_eq!(w.history(17).await.len(), 3);
}

// O-9: the system tick fans out keyed workspace jobs and never stacks them.
#[sqlx::test]
async fn tick_fans_out_per_pair_jobs_once(pool: PgPool) {
    let w = world(pool).await;
    assert_eq!(tick::fan_out(&w.pool).await.unwrap(), 3);
    tick::fan_out(&w.pool).await.unwrap();
    let jobs: Vec<(String, Option<Uuid>, String, String, Value)> = sqlx::query_as(
        "SELECT kind, workspace_id, dedupe_key, concurrency_key, payload FROM jobs ORDER BY kind",
    )
    .fetch_all(&w.pool)
    .await
    .unwrap();
    let p = w.k.pair;
    assert_eq!(jobs.len(), 3, "dedupe keeps one live job per kind and pair");
    for (j, kind) in jobs
        .iter()
        .zip(["wf01_full_scan", "wf01_poll", "wf01_sweep"])
    {
        assert_eq!(j.0, kind);
        assert_eq!(j.1, Some(w.k.ws));
        assert_eq!(j.2, format!("{kind}:{p}"));
        assert_eq!(j.3, format!("wf01:{p}"));
        assert_eq!(j.4["pair_id"], p.to_string());
    }
    sqlx::query("DELETE FROM jobs")
        .execute(&w.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE integration_pairs SET last_polled_at = now(), last_swept_at = now(),
             last_full_scan_at = now() - interval '61 minutes' WHERE id = $1",
    )
    .bind(p)
    .execute(&w.pool)
    .await
    .unwrap();
    assert_eq!(
        tick::fan_out(&w.pool).await.unwrap(),
        1,
        "only the full scan is due"
    );
    sqlx::query("UPDATE integration_pairs SET status = 'paused', paused_reason = 'manual'")
        .execute(&w.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM jobs")
        .execute(&w.pool)
        .await
        .unwrap();
    assert_eq!(
        tick::fan_out(&w.pool).await.unwrap(),
        0,
        "paused pairs are skipped"
    );
}

// Another workspace's pair, mappings and requests are never read or written.
#[sqlx::test]
async fn intake_stays_in_its_workspace(pool: PgPool) {
    let w = world(pool).await;
    let other = testkit::kit(&w.pool, "other").await;
    map(&w.pool, &other, "vendor", "42", "9999").await;
    w.put(17, Order::draft(T0, 55));
    w.poll().await.unwrap();
    let (_, _, snap) = w.latest(17).await;
    assert_eq!(snap["vendor"]["target_id"], "1187");
    let foreign: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_requests WHERE workspace_id = $1")
            .bind(other.ws)
            .fetch_one(&w.pool)
            .await
            .unwrap();
    assert_eq!(foreign, 0);
}
