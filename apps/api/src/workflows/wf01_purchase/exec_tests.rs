//! WF-01 execute + reconcile (C4b) against the in-process fake Odoo and fake NetSuite (real C3a
//! and C3b clients). Names carry the R3-§9 acceptance ids. Every test asserts DB state and the
//! fake NetSuite's POST log. "Verified against the documented contract", not live.

use std::{sync::Arc, time::Duration};

use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{Barrier, Notify};
use uuid::Uuid;

use super::{
    execute::{self, Job},
    fake_netsuite::{self, Post, Shared as Ns},
    fake_odoo::Order,
    link, reconcile,
    tests::{T0, W, world},
    tick,
};
use crate::{
    integrations::providers::SinkClient,
    jobs::{
        kinds::{WF01_EXECUTE, WF01_RECONCILE},
        repo::{self as jobs, Claimed, NewJob},
        runner::Step,
    },
    workflows::{
        dispatch,
        repo::{self as wrepo, HumanMove, NewResolution},
    },
};

struct X {
    w: W,
    sink: Arc<SinkClient>,
    ns: Ns,
}

/// RFQ 17 polled into a pending request; fake NetSuite up.
async fn x(pool: PgPool) -> X {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    w.poll().await.unwrap();
    let (sink, ns) = fake_netsuite::start().await;
    X {
        w,
        sink: Arc::new(sink),
        ns,
    }
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct Row {
    status: String,
    phase: String,
    reason: Option<String>,
    error_class: Option<String>,
    remote_attempts: i16,
    target_ref: Option<String>,
}

impl X {
    fn job(&self, e: Uuid, token: Uuid) -> Job {
        Job {
            ws: self.w.k.ws,
            execution_id: e,
            token,
            attempt: 1,
            max_attempts: 5,
        }
    }

    async fn exec(&self, e: Uuid) -> Step {
        self.run(self.job(e, Uuid::now_v7())).await
    }

    async fn run(&self, j: Job) -> Step {
        execute::execute(&self.w.pool, &j, Ok(&self.w.client), Ok(&*self.sink))
            .await
            .unwrap()
    }

    async fn reconcile(&self, e: Uuid, attempt: i32) -> Result<Step, crate::error::AppError> {
        let j = Job {
            attempt,
            max_attempts: 10,
            ..self.job(e, Uuid::now_v7())
        };
        reconcile::reconcile(&self.w.pool, &j, Ok(&*self.sink)).await
    }

    async fn row(&self, e: Uuid) -> Row {
        sqlx::query_as(
            "SELECT status, phase, reason, error_class, remote_attempts, target_ref
             FROM workflow_executions WHERE id = $1",
        )
        .bind(e)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }

    async fn outcomes(&self, e: Uuid) -> Vec<Option<String>> {
        sqlx::query_scalar(
            "SELECT outcome FROM workflow_execution_attempts WHERE execution_id = $1 ORDER BY attempt_no",
        )
        .bind(e)
        .fetch_all(&self.w.pool)
        .await
        .unwrap()
    }

    async fn kinds(&self, e: Uuid) -> Vec<String> {
        sqlx::query_scalar("SELECT kind FROM workflow_events WHERE execution_id = $1 ORDER BY id")
            .bind(e)
            .fetch_all(&self.w.pool)
            .await
            .unwrap()
    }

    async fn eid(&self, e: Uuid) -> String {
        sqlx::query_scalar("SELECT correlation_id FROM workflow_executions WHERE id = $1")
            .bind(e)
            .fetch_one(&self.w.pool)
            .await
            .unwrap()
    }

    async fn sql(&self, q: &'static str) {
        sqlx::query(q)
            .bind(self.w.k.ws)
            .execute(&self.w.pool)
            .await
            .unwrap();
    }

    fn posts(&self) -> usize {
        fake_netsuite::posts(&self.ns).len()
    }

    async fn jobs_of(&self, kind: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE workspace_id = $1 AND kind = $2")
            .bind(self.w.k.ws)
            .bind(kind)
            .fetch_one(&self.w.pool)
            .await
            .unwrap()
    }

    async fn claim(&self) -> Claimed {
        jobs::claim(&self.w.pool, &[WF01_EXECUTE], 100, Duration::from_secs(300))
            .await
            .unwrap()
            .unwrap()
    }

    /// Expire a claimed job's lease and let `reap` requeue it (same job, next claim gets a
    /// new token).
    async fn expire(&self, job: Uuid) {
        sqlx::query("UPDATE jobs SET locked_until = now() - interval '1 second' WHERE id = $1")
            .bind(job)
            .execute(&self.w.pool)
            .await
            .unwrap();
        assert_eq!(jobs::reap(&self.w.pool).await.unwrap(), 1);
        // Skip the reap backoff: the requeued job is due now.
        sqlx::query("UPDATE jobs SET run_at = now() WHERE id = $1")
            .bind(job)
            .execute(&self.w.pool)
            .await
            .unwrap();
    }

    async fn enqueue(&self, e: Uuid) -> Uuid {
        let mut c = self.w.pool.acquire().await.unwrap();
        jobs::enqueue(&mut c, &execute::job(self.w.k.ws, e))
            .await
            .unwrap()
    }

    async fn incidents(&self) -> Vec<String> {
        self.w.incidents().await.into_iter().map(|i| i.0).collect()
    }

    /// The C2 `allow_retry` move (resolve route body), as a non-requester reconciler.
    async fn grant(&self, e: Uuid) {
        let mut tx = self.w.pool.begin().await.unwrap();
        let m = HumanMove {
            to: "queued",
            reason: None,
            resolution: None,
            phase: Some("prechecking"),
            grant: true,
        };
        wrepo::move_execution(&mut *tx, self.w.k.ws, e, &m)
            .await
            .unwrap();
        let r = NewResolution {
            execution_id: Some(e),
            incident_id: None,
            action: "allow_retry",
            actor: self.w.k.approver,
            reason: "Checked NetSuite: no order",
            target_id: None,
            outcome: "queued",
        };
        wrepo::insert_resolution(&mut *tx, self.w.k.ws, &r)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    async fn wait_until(&self, f: impl Fn(&fake_netsuite::State) -> bool) {
        for _ in 0..1000 {
            if f(&self.ns.lock().unwrap()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("fake NetSuite never reached the expected state");
    }
}

fn job_of(c: &Claimed, e: Uuid) -> Job {
    Job {
        ws: c.workspace_id.unwrap(),
        execution_id: e,
        token: c.token,
        attempt: c.attempts,
        max_attempts: c.max_attempts,
    }
}

// Happy path: one POST with the execution's externalId, read back, verified, linked.
#[sqlx::test]
async fn happy_path_posts_once_verifies_and_links(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    assert_eq!(x.exec(e).await, Step::Done);
    let r = x.row(e).await;
    assert_eq!((r.status.as_str(), r.phase.as_str()), ("succeeded", "done"));
    assert_eq!(r.remote_attempts, 1);
    let target: Uuid = sqlx::query_scalar("SELECT target_connection_id FROM integration_pairs")
        .fetch_one(&x.w.pool)
        .await
        .unwrap();
    assert_eq!(
        r.target_ref,
        Some(format!("netsuite:{target}:purchaseOrder:100"))
    );
    let posts = fake_netsuite::posts(&x.ns);
    assert_eq!(posts.len(), 1);
    let body: Value = serde_json::from_str(&posts[0].body).unwrap();
    assert_eq!(body["externalId"], json!(x.eid(e).await));
    assert_eq!(body["entity"]["id"], "1187");
    assert_eq!(body["subsidiary"]["id"], "3");
    assert_eq!(body["item"]["items"][0]["item"]["id"], "501");
    assert_eq!(body["item"]["items"][0]["rate"].to_string(), "150.00000000");
    assert_eq!(x.outcomes(e).await, vec![Some("created".into())]);
    let kinds = x.kinds(e).await;
    for k in [
        "claim",
        "write_intent",
        "api_request",
        "api_result",
        "verify_match",
    ] {
        assert!(kinds.iter().any(|x| x == k), "{k} in {kinds:?}");
    }
    let link: (String, Option<String>) =
        sqlx::query_as("SELECT state, target_id FROM external_links WHERE correlation_id = $1")
            .bind(x.eid(e).await)
            .fetch_one(&x.w.pool)
            .await
            .unwrap();
    assert_eq!(link, ("linked".into(), Some("100".into())));
    // A duplicate run is a no-op.
    assert_eq!(x.exec(e).await, Step::Done);
    assert_eq!(x.posts(), 1);
    assert_eq!(fake_netsuite::writes_other_than_post(&x.ns), 0);
}

// T11′: the create commits, the reply never comes → awaiting; reconcile finds it → succeeded.
#[sqlx::test]
async fn t11_commit_then_timeout_reconciles_to_succeeded(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    x.ns.lock().unwrap().posts.push_back(Post::CommitThenHang);
    assert_eq!(x.exec(e).await, Step::Done);
    let r = x.row(e).await;
    assert_eq!(r.status, "awaiting_reconciliation");
    assert_eq!(r.error_class.as_deref(), Some("ambiguous"));
    assert_eq!(x.outcomes(e).await, vec![Some("ambiguous".into())]);
    assert_eq!(x.posts(), 1);
    assert_eq!(x.jobs_of(WF01_RECONCILE).await, 1);
    assert_eq!(x.reconcile(e, 1).await.unwrap(), Step::Done);
    let r = x.row(e).await;
    assert_eq!(r.status, "succeeded");
    assert!(x.kinds(e).await.contains(&"verify_match".to_string()));
    assert_eq!(x.posts(), 1);
    assert_eq!(x.ns.lock().unwrap().stored.len(), 1);
}

// T16′ + P-2: lease expires while the POST is in flight; the second worker takes over and does
// not POST; the stale worker's late success is evidence; reconcile reads it back → succeeded.
#[sqlx::test]
async fn t16_lease_expiry_mid_post_one_post_stale_evidence(pool: PgPool) {
    let x = Arc::new(x(pool).await);
    let e = x.w.approve(17, "queued", "prechecking").await;
    let jid = x.enqueue(e).await;
    let a = x.claim().await;
    let gate = Arc::new(Notify::new());
    {
        let mut s = x.ns.lock().unwrap();
        s.posts.push_back(Post::CommitThenHold(gate.clone()));
        s.miss_lookup = true; // only the stale worker's evidence can find the PO
    }
    let xa = x.clone();
    let ja = job_of(&a, e);
    let worker_a = tokio::spawn(async move { xa.run(ja).await });
    x.wait_until(|s| s.stored.len() == 1).await;

    // Takeover refused while A's lease is live.
    assert_eq!(x.run(x.job(e, Uuid::now_v7())).await, Step::Done);
    assert_eq!(x.row(e).await.status, "running");

    x.expire(jid).await;
    let b = x.claim().await;
    assert_eq!(b.id, jid);
    assert_eq!(x.run(job_of(&b, e)).await, Step::Done);
    let r = x.row(e).await;
    assert_eq!(r.status, "awaiting_reconciliation");
    assert_eq!(x.outcomes(e).await, vec![Some("ambiguous".into())]);
    assert!(x.kinds(e).await.contains(&"takeover".to_string()));
    assert_eq!(x.posts(), 1, "worker B never POSTs (W1)");

    gate.notify_one();
    assert_eq!(worker_a.await.unwrap(), Step::Done);
    let stale: (Option<i16>, Value) = sqlx::query_as(
        "SELECT attempt_no, payload FROM workflow_events
         WHERE execution_id = $1 AND kind = 'stale_worker_result'",
    )
    .bind(e)
    .fetch_one(&x.w.pool)
    .await
    .unwrap();
    assert_eq!(stale.0, Some(1));
    assert_eq!(stale.1["target_id"], "100");
    assert_eq!(
        x.row(e).await.status,
        "awaiting_reconciliation",
        "fence held"
    );

    assert_eq!(x.reconcile(e, 1).await.unwrap(), Step::Done);
    assert_eq!(x.row(e).await.status, "succeeded");
    assert_eq!(x.posts(), 1);
}

// T24: 8 workers with live leases claim one execution at once → 1 owner, 1 POST.
#[sqlx::test]
async fn t24_eight_workers_one_claim_one_post(pool: PgPool) {
    let x = Arc::new(x(pool).await);
    let e = x.w.approve(17, "queued", "prechecking").await;
    let mut claimed = vec![];
    for i in 0..8 {
        let mut c = x.w.pool.acquire().await.unwrap();
        let j = NewJob {
            workspace_id: Some(x.w.k.ws),
            payload: json!({"execution_id": e}),
            dedupe_key: Some(format!("t24:{i}")),
            ..NewJob::new(WF01_EXECUTE)
        };
        jobs::enqueue(&mut c, &j).await.unwrap();
        claimed.push(x.claim().await);
    }
    let gate = Arc::new(Barrier::new(8));
    let tasks: Vec<_> = claimed
        .iter()
        .map(|c| {
            let (x, gate, j) = (x.clone(), gate.clone(), job_of(c, e));
            tokio::spawn(async move {
                gate.wait().await;
                x.run(j).await
            })
        })
        .collect();
    for t in tasks {
        assert_eq!(t.await.unwrap(), Step::Done);
    }
    assert_eq!(x.posts(), 1);
    assert_eq!(x.row(e).await.status, "succeeded");
    let kinds = x.kinds(e).await;
    assert_eq!(kinds.iter().filter(|k| *k == "claim").count(), 1);
    assert_eq!(kinds.iter().filter(|k| *k == "takeover").count(), 0);
}

// T25: the execute job is enqueued 3 times + 2 backstop ticks → 1 live job, 1 POST.
#[sqlx::test]
async fn t25_duplicate_enqueues_one_job_one_post(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    for _ in 0..3 {
        let mut tx = x.w.pool.begin().await.unwrap();
        dispatch::enqueue_execution(&mut tx, "wf01_purchase", e)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    tick::enqueue_queued_executions(&x.w.pool).await.unwrap();
    tick::enqueue_queued_executions(&x.w.pool).await.unwrap();
    assert_eq!(x.jobs_of(WF01_EXECUTE).await, 1);
    let c = x.claim().await;
    assert_eq!(c.payload["execution_id"], json!(e));
    assert_eq!(x.run(job_of(&c, e)).await, Step::Done);
    assert_eq!(x.posts(), 1);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM workflow_executions")
        .fetch_one(&x.w.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(x.row(e).await.status, "succeeded");
}

// T26: A blocks in its pre-check lookup, loses its lease; B takes over and POSTs; A resumes and
// its write-intent CAS is refused → 1 POST.
#[sqlx::test]
async fn t26_write_intent_race_forced_interleaving(pool: PgPool) {
    let x = Arc::new(x(pool).await);
    let e = x.w.approve(17, "queued", "prechecking").await;
    let jid = x.enqueue(e).await;
    let a = x.claim().await;
    let gate = Arc::new(Notify::new());
    x.ns.lock().unwrap().hold_lookup = Some(gate.clone());
    let xa = x.clone();
    let ja = job_of(&a, e);
    let worker_a = tokio::spawn(async move { xa.run(ja).await });
    x.wait_until(|s| s.seen.iter().any(|r| r.method == "GET"))
        .await;
    x.expire(jid).await;
    let b = x.claim().await;
    assert_eq!(x.run(job_of(&b, e)).await, Step::Done);
    assert_eq!(x.row(e).await.status, "succeeded");
    x.ns.lock().unwrap().miss_lookup = true; // A's lookup sees nothing and reaches the CAS
    gate.notify_one();
    assert_eq!(worker_a.await.unwrap(), Step::Done);
    assert_eq!(x.posts(), 1);
    assert_eq!(x.outcomes(e).await, vec![Some("created".into())]);
    assert_eq!(x.row(e).await.status, "succeeded");
}

async fn backdate_intent(x: &X, e: Uuid, minutes: i32) {
    let mut tx = x.w.pool.begin().await.unwrap();
    // The set-once trigger refuses this UPDATE; test-only bypass simulates elapsed time.
    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE workflow_execution_attempts
         SET write_intent_at = write_intent_at - make_interval(mins => $2) WHERE execution_id = $1",
    )
    .bind(e)
    .bind(minutes)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

// T27 + Q2 window: not-found is never proof; 10 re-checks keep it open with 0 POSTs. A grant
// allows exactly one more POST with the same externalId, only after the 15-minute window.
#[sqlx::test]
async fn t27_not_found_never_proof_grant_one_more_after_window(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    let err = fake_netsuite::ns_error(500, "UNEXPECTED_ERROR", "boom");
    x.ns.lock().unwrap().posts.push_back(Post::Reply(500, err));
    assert_eq!(x.exec(e).await, Step::Done);
    assert_eq!(x.row(e).await.status, "awaiting_reconciliation");
    assert_eq!(x.outcomes(e).await, vec![Some("ambiguous".into())]);
    for n in 1..=10 {
        let r = x.reconcile(e, n).await;
        if n < 10 {
            assert!(r.is_err(), "check {n} retries with backoff");
        } else {
            assert_eq!(r.unwrap(), Step::Done, "last check leaves it to a human");
        }
        assert_eq!(x.row(e).await.status, "awaiting_reconciliation");
    }
    let checks = x
        .kinds(e)
        .await
        .into_iter()
        .filter(|k| k == "reconcile_check")
        .count();
    assert_eq!(checks, 10);
    assert_eq!(x.posts(), 1);

    x.grant(e).await;
    // Too early: the first request may still land (facts N5 l.244, 15 min). Each tick-driven
    // run is a no-op (no write intent, no POST); the wait is logged once.
    let tick_run = async || {
        tick::enqueue_queued_executions(&x.w.pool).await.unwrap();
        let c = x.claim().await;
        assert_eq!(x.run(job_of(&c, e)).await, Step::Done);
        jobs::finish(&x.w.pool, c.id, c.token).await.unwrap();
    };
    for _ in 0..2 {
        tick_run().await;
        assert_eq!(x.posts(), 1);
        let r = x.row(e).await;
        assert_eq!((r.status.as_str(), r.remote_attempts), ("queued", 1));
    }
    let waits: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM workflow_events WHERE execution_id = $1 AND kind = 'retry_window_wait'",
    )
    .bind(e)
    .fetch_all(&x.w.pool)
    .await
    .unwrap();
    assert_eq!(waits.len(), 1, "logged once, not per tick");
    assert!(waits[0]["earliest"].is_string());
    backdate_intent(&x, e, 16).await;
    tick_run().await;
    let posts = fake_netsuite::posts(&x.ns);
    assert_eq!(posts.len(), 2);
    let eid = |p: &fake_netsuite::Seen| {
        serde_json::from_str::<Value>(&p.body).unwrap()["externalId"].clone()
    };
    assert_eq!(eid(&posts[0]), eid(&posts[1]), "same externalId");
    let granted: Option<Uuid> = sqlx::query_scalar(
        "SELECT granted_by FROM workflow_execution_attempts WHERE execution_id = $1 AND attempt_no = 2",
    )
    .bind(e)
    .fetch_one(&x.w.pool)
    .await
    .unwrap();
    assert_eq!(granted, Some(x.w.k.approver));
    assert_eq!(x.row(e).await.status, "succeeded");
    // Requester → 403: C2 HTTP test `allow_retry_grants_one_attempt_by_a_non_requester`.
}

// T28: "link existing" on a differing PO is never verified; on a matching PO it succeeds.
#[sqlx::test]
async fn t28_link_existing_mismatch_is_resolved_manual(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    x.ns.lock().unwrap().posts.push_back(Post::CommitThenHang);
    x.ns.lock().unwrap().tamper = Some(|po| po["item"]["items"][0]["rate"] = json!(149.5));
    x.exec(e).await;
    assert_eq!(x.row(e).await.status, "awaiting_reconciliation");
    let mut tx = x.w.pool.begin().await.unwrap();
    let out = link::link(&mut tx, &x.sink, x.w.k.ws, e, "100")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(out, "resolved_manual:linked_with_differences");
    let r = x.row(e).await;
    assert_eq!(r.status, "resolved_manual");
    assert!(r.target_ref.unwrap().ends_with(":purchaseOrder:100"));
    assert!(!x.kinds(e).await.contains(&"verify_match".to_string()));
    let resolution: Option<String> =
        sqlx::query_scalar("SELECT resolution FROM workflow_executions WHERE id = $1")
            .bind(e)
            .fetch_one(&x.w.pool)
            .await
            .unwrap();
    assert_eq!(resolution.as_deref(), Some("linked_with_differences"));
    // Unknown id → readable 422 on target_id.
    let mut tx = x.w.pool.begin().await.unwrap();
    assert!(
        link::link(&mut tx, &x.sink, x.w.k.ws, e, "999")
            .await
            .is_err()
    );
}

#[sqlx::test]
async fn t28_link_existing_match_succeeds(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    x.ns.lock().unwrap().posts.push_back(Post::CommitThenHang);
    x.exec(e).await;
    let mut tx = x.w.pool.begin().await.unwrap();
    let out = link::link(&mut tx, &x.sink, x.w.k.ws, e, "100")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(out, "succeeded");
    assert_eq!(x.row(e).await.status, "succeeded");
    assert!(x.kinds(e).await.contains(&"verify_match".to_string()));
    assert_eq!(x.posts(), 1);
}

// T29: source change by phase (R3-§5).
#[sqlx::test]
async fn t29_source_change_by_phase(pool: PgPool) {
    let x = x(pool).await;
    for id in 18..=20 {
        x.w.put(id, Order::draft(T0, id * 10));
    }
    x.w.poll().await.unwrap();
    // prechecking: executor cancels, invalidates, next revision exists, 0 POSTs.
    let e17 = x.w.approve(17, "queued", "prechecking").await;
    x.w.edit(17, |o| o.lines[0].price = "151.0");
    assert_eq!(x.exec(e17).await, Step::Done);
    let r = x.row(e17).await;
    assert_eq!(
        (r.status.as_str(), r.reason.as_deref()),
        ("cancelled", Some("source_changed"))
    );
    let h = x.w.history(17).await;
    assert_eq!(h[0].1, "invalidated");
    assert_eq!(h[0].2.as_deref(), Some("source_changed"));
    assert_eq!(h.len(), 2, "next revision created now");
    assert_eq!(x.posts(), 0);
    // write_intent / write_sent: executor goes to reconciliation, approval stays approved.
    let e18 = x.w.approve(18, "running", "write_intent").await;
    let e19 = x.w.approve(19, "running", "write_sent").await;
    x.w.edit(18, |o| o.lines[0].price = "151.0");
    x.w.edit(19, |o| o.lines[0].price = "151.0");
    for e in [e18, e19] {
        assert_eq!(x.exec(e).await, Step::Done);
        assert_eq!(x.row(e).await.status, "awaiting_reconciliation");
    }
    assert_eq!(x.w.history(18).await[0].1, "approved");
    assert_eq!(x.posts(), 0);
    // succeeded: incident only.
    x.w.approve(20, "succeeded", "done").await;
    x.w.edit(20, |o| o.lines[0].price = "151.0");
    x.w.sweep().await;
    let inc = x.incidents().await;
    assert!(
        inc.contains(&"source_changed_during_write".to_string()),
        "{inc:?}"
    );
    assert!(
        inc.contains(&"source_changed_after_execution".to_string()),
        "{inc:?}"
    );
    assert_eq!(x.w.history(20).await[0].1, "approved");
    assert_eq!(x.posts(), 0);
    // CHECK I2 refusing a cancel with remote_attempts > 0: executions.rs
    // `cancel_after_remote_attempt_is_refused_by_i2`.
}

// T31: NetSuite tax 1 minor unit off at tolerance 0 → variance, never plain succeeded.
#[sqlx::test]
async fn t31_tax_variance_is_reviewed_not_succeeded(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    x.ns.lock().unwrap().tax_total = "63.01".into();
    x.exec(e).await;
    assert_eq!(x.row(e).await.status, "succeeded_with_variance");
    assert_eq!(x.incidents().await, vec!["tax_variance"]);
    assert_eq!(x.posts(), 1);
    // Accept (C2 resolve body; HTTP path: `variance_accept_and_reject`).
    let m = HumanMove {
        to: "resolved_manual",
        reason: Some("variance_accepted"),
        resolution: Some("variance_accepted"),
        phase: None,
        grant: false,
    };
    wrepo::move_execution(&x.w.pool, x.w.k.ws, e, &m)
        .await
        .unwrap();
    assert_eq!(x.row(e).await.status, "resolved_manual");
}

// T32: read-back rate differs from the rate sent → awaiting + target_conflict, no PATCH/PUT.
#[sqlx::test]
async fn t32_rate_mismatch_is_target_conflict(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    x.ns.lock().unwrap().tamper = Some(|po| po["item"]["items"][0]["rate"] = json!(149.99));
    x.exec(e).await;
    let r = x.row(e).await;
    assert_eq!(r.status, "awaiting_reconciliation");
    assert_eq!(x.incidents().await, vec!["target_conflict"]);
    assert!(x.kinds(e).await.contains(&"verify_mismatch".to_string()));
    assert_eq!(x.posts(), 1);
    assert_eq!(fake_netsuite::writes_other_than_post(&x.ns), 0);
}

// Pre-checks block before any write intent.
#[sqlx::test]
async fn prechecks_block_before_write_intent(pool: PgPool) {
    let x = x(pool).await;
    for id in 18..=21 {
        x.w.put(id, Order::draft(T0, id * 10));
    }
    x.w.poll().await.unwrap();
    let blocked = async |e: Uuid| {
        assert_eq!(x.exec(e).await, Step::Done);
        let r = x.row(e).await;
        assert_eq!(r.status, "blocked");
        assert_eq!(r.remote_attempts, 0);
        r.reason.unwrap()
    };
    // Approver loses membership (D8).
    let e = x.w.approve(17, "queued", "prechecking").await;
    x.sql("UPDATE workspace_members SET active = false WHERE workspace_id = $1 AND user_id <> (SELECT created_by_user_id FROM workspaces WHERE id = $1)").await;
    assert_eq!(blocked(e).await, "approver_revoked");
    x.sql("UPDATE workspace_members SET active = true WHERE workspace_id = $1")
        .await;
    // Mapping retargeted after approval.
    let e = x.w.approve(18, "queued", "prechecking").await;
    x.sql("UPDATE integration_mappings SET target_id = '1188' WHERE workspace_id = $1 AND entity = 'vendor'").await;
    assert_eq!(blocked(e).await, "mapping_changed");
    x.sql("UPDATE integration_mappings SET target_id = '1187' WHERE workspace_id = $1 AND entity = 'vendor'").await;
    // Target credential expired.
    let e = x.w.approve(19, "queued", "prechecking").await;
    x.sql("UPDATE integration_connections SET credential_expires_at = now() - interval '1 day' WHERE workspace_id = $1 AND provider = 'netsuite'").await;
    assert_eq!(blocked(e).await, "connection_expired");
    x.sql("UPDATE integration_connections SET credential_expires_at = NULL, status = 'active' WHERE workspace_id = $1").await;
    // Pair paused.
    let e = x.w.approve(20, "queued", "prechecking").await;
    x.sql("UPDATE integration_pairs SET status = 'paused', paused_reason = 'manual' WHERE workspace_id = $1").await;
    assert_eq!(blocked(e).await, "pair_paused");
    assert_eq!(x.posts(), 0);
    assert!(x.kinds(e).await.contains(&"precheck_failed".to_string()));
}

// C6-O1: a line flips only `is_downpayment` after approval (same fingerprint) → blocked, 0 POSTs.
#[sqlx::test]
async fn o1_downpayment_flip_after_approval_blocks_before_write(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    x.w.edit(17, |o| o.lines[0].downpayment = true);
    assert_eq!(x.exec(e).await, Step::Done);
    let r = x.row(e).await;
    assert_eq!(
        (r.status.as_str(), r.reason.as_deref()),
        ("blocked", Some("downpayment_line"))
    );
    assert_eq!(r.remote_attempts, 0);
    assert_eq!(x.posts(), 0);
}

// NetSuite answers: 400 → blocked(validation_rejected); 429 → awaiting, transient, rejected.
#[sqlx::test]
async fn rejected_answers_settle_the_attempt(pool: PgPool) {
    let x = x(pool).await;
    x.w.put(18, Order::draft(T0, 180));
    x.w.poll().await.unwrap();
    let e = x.w.approve(17, "queued", "prechecking").await;
    let bad = fake_netsuite::ns_error(400, "INVALID_CONTENT", "Invalid item reference.");
    x.ns.lock().unwrap().posts.push_back(Post::Reply(400, bad));
    x.exec(e).await;
    let r = x.row(e).await;
    assert_eq!(
        (r.status.as_str(), r.reason.as_deref()),
        ("blocked", Some("validation_rejected"))
    );
    assert_eq!(x.outcomes(e).await, vec![Some("rejected".into())]);

    let e = x.w.approve(18, "queued", "prechecking").await;
    // facts l.256: 429 CONCURRENCY_LIMIT_EXCEEDED, "Request blocked."
    let busy = fake_netsuite::ns_error(
        429,
        "CONCURRENCY_LIMIT_EXCEEDED",
        "Concurrent request limit exceeded. Request blocked.",
    );
    x.ns.lock().unwrap().posts.push_back(Post::Reply(429, busy));
    x.exec(e).await;
    let r = x.row(e).await;
    assert_eq!(r.status, "awaiting_reconciliation");
    assert_eq!(r.error_class.as_deref(), Some("transient"));
    assert_eq!(x.outcomes(e).await, vec![Some("rejected".into())]);
    let ev: Value = sqlx::query_scalar(
        "SELECT evidence FROM workflow_execution_attempts WHERE execution_id = $1",
    )
    .bind(e)
    .fetch_one(&x.w.pool)
    .await
    .unwrap();
    assert_eq!(ev["http_status"], 429);
    assert_eq!(ev["error_code"], "CONCURRENCY_LIMIT_EXCEEDED");
    assert_eq!(x.posts(), 2);
}

// Transient source failure before write intent: requeue with backoff, then fail at the cap.
#[sqlx::test]
async fn transient_before_write_retries_then_fails(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    x.w.odoo
        .lock()
        .unwrap()
        .fail
        .push("purchase.order/search_read".into());
    let j = x.job(e, Uuid::now_v7());
    let r = execute::execute(&x.w.pool, &j, Ok(&x.w.client), Ok(&*x.sink)).await;
    assert!(r.is_err_and(|e| !e.is_permanent()), "rail retries");
    assert_eq!(x.row(e).await.status, "queued");
    let last = Job {
        attempt: 5,
        ..x.job(e, Uuid::now_v7())
    };
    assert_eq!(x.run(last).await, Step::Done);
    let r = x.row(e).await;
    assert_eq!(
        (r.status.as_str(), r.reason.as_deref()),
        ("failed", Some("attempts_exhausted"))
    );
    assert_eq!(x.posts(), 0);
}

// Another workspace's job cannot touch this execution.
#[sqlx::test]
async fn other_workspace_job_changes_nothing(pool: PgPool) {
    let x = x(pool).await;
    let e = x.w.approve(17, "queued", "prechecking").await;
    let j = Job {
        ws: Uuid::now_v7(),
        ..x.job(e, Uuid::now_v7())
    };
    assert_eq!(x.run(j).await, Step::Done);
    assert_eq!(x.row(e).await.status, "queued");
    assert_eq!(x.posts(), 0);
}

// Backstop: queued and orphaned running get a job; awaiting_reconciliation never does.
#[sqlx::test]
async fn backstop_enqueues_queued_and_orphans_only(pool: PgPool) {
    let x = x(pool).await;
    for id in 18..=19 {
        x.w.put(id, Order::draft(T0, id * 10));
    }
    x.w.poll().await.unwrap();
    let q = x.w.approve(17, "queued", "prechecking").await;
    let orphan = x.w.approve(18, "running", "prechecking").await;
    let waiting =
        x.w.approve(19, "awaiting_reconciliation", "write_sent")
            .await;
    assert_eq!(tick::enqueue_queued_executions(&x.w.pool).await.unwrap(), 2);
    let payloads: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM jobs WHERE kind = 'wf01_execute' ORDER BY payload->>'execution_id'",
    )
    .fetch_all(&x.w.pool)
    .await
    .unwrap();
    let mut want = vec![json!({"execution_id": q}), json!({"execution_id": orphan})];
    want.sort_by_key(|v| v["execution_id"].as_str().unwrap().to_owned());
    assert_eq!(payloads, want);
    assert!(!payloads.contains(&json!({"execution_id": waiting})));
}
