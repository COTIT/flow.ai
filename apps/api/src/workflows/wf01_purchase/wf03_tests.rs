//! WF-03 recovery (contracts §11.3, PLAN-WF02-07 §4.5) over WF-01 writes, against the
//! in-process doc-copied fake Odoo and fake NetSuite. "Verified against the documented
//! contract", not live.

use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    execute::{self, Job},
    fake_netsuite::{self, Post, Shared as Ns},
    fake_odoo::Order,
    tests::{T0, W, world},
};
use crate::{
    integrations::{
        capability::{CapabilityCheck, ErrorClass},
        connections::repo as conns,
        providers::SinkClient,
    },
    workflows::{
        recovery::{
            health,
            sweep::{self, Checked},
        },
        repo::{self as wrepo, HumanMove, NewResolution},
    },
};

struct X {
    w: W,
    sink: SinkClient,
    ns: Ns,
}

async fn x(pool: PgPool) -> X {
    let w = world(pool).await;
    w.put(17, Order::draft(T0, 55));
    w.poll().await.unwrap();
    let (sink, ns) = fake_netsuite::start().await;
    X { w, sink, ns }
}

#[derive(Debug, sqlx::FromRow)]
struct Exp {
    status: String,
    checks: i16,
    incident_id: Option<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
struct Inc {
    kind: String,
    category: Option<String>,
    status: String,
    resolution_kind: Option<String>,
    verified: bool,
    diagnosis: Option<Value>,
}

impl X {
    async fn exec(&self, e: Uuid) {
        let j = Job {
            ws: self.w.k.ws,
            execution_id: e,
            token: Uuid::now_v7(),
            attempt: 1,
            max_attempts: 5,
        };
        execute::execute(&self.w.pool, &j, Ok(&self.w.client), Ok(&self.sink))
            .await
            .unwrap();
    }

    /// RFQ 17 approved and sent once with `post` as NetSuite's answer.
    async fn sent(&self, post: Option<Post>) -> Uuid {
        let e = self.w.approve(17, "queued", "prechecking").await;
        if let Some(p) = post {
            self.ns.lock().unwrap().posts.push_back(p);
        }
        self.exec(e).await;
        e
    }

    fn ambiguous() -> Option<Post> {
        let err = fake_netsuite::ns_error(500, "UNEXPECTED_ERROR", "boom");
        Some(Post::Reply(500, err))
    }

    async fn exp(&self, e: Uuid) -> Exp {
        sqlx::query_as(
            "SELECT status, checks, incident_id FROM exchange_expectations WHERE execution_id = $1",
        )
        .bind(e)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }

    async fn incidents(&self) -> Vec<Inc> {
        sqlx::query_as(
            "SELECT kind, category, status, resolution_kind, verified_at IS NOT NULL AS verified,
                    diagnosis
             FROM workflow_incidents WHERE workspace_id = $1 ORDER BY id",
        )
        .bind(self.w.k.ws)
        .fetch_all(&self.w.pool)
        .await
        .unwrap()
    }

    /// Make every expectation due now.
    async fn due(&self) {
        sqlx::query("UPDATE exchange_expectations SET due_at = now() - interval '1 minute'")
            .execute(&self.w.pool)
            .await
            .unwrap();
    }

    async fn sweep(&self) -> Vec<Checked> {
        sweep::sweep_with(&self.w.pool, &self.sink).await.unwrap()
    }

    fn posts(&self) -> usize {
        fake_netsuite::posts(&self.ns).len()
    }

    fn lookups(&self) -> usize {
        self.ns
            .lock()
            .unwrap()
            .seen
            .iter()
            .filter(|s| s.method == "GET")
            .count()
    }
}

#[sqlx::test]
async fn wf03_expectation_met_on_verified_readback(pool: PgPool) {
    let x = x(pool).await;
    let e = x.sent(None).await;
    let exp = x.exp(e).await;
    assert_eq!(exp.status, "met");
    assert!(x.incidents().await.is_empty());
    x.due().await;
    assert!(
        x.sweep().await.is_empty(),
        "met rows are never checked again"
    );
}

#[sqlx::test]
async fn wf03_late_expectation_opens_one_incident(pool: PgPool) {
    let x = x(pool).await;
    let e = x.sent(X::ambiguous()).await;
    assert_eq!(x.exp(e).await.status, "pending");
    assert!(x.sweep().await.is_empty(), "not due before 15 minutes");
    x.due().await;
    for _ in 0..3 {
        assert_eq!(x.sweep().await, vec![Checked::Late]);
    }
    let exp = x.exp(e).await;
    assert_eq!((exp.status.as_str(), exp.checks), ("late", 3));
    let inc = x.incidents().await;
    assert_eq!(inc.len(), 1, "{inc:?}");
    assert_eq!(inc[0].kind, "exchange_late");
    let d = inc[0].diagnosis.as_ref().unwrap();
    assert!(d["sentence"].as_str().unwrap().len() > 10);
    assert_eq!(d["last_attempt"]["outcome"], "ambiguous");
}

#[sqlx::test]
async fn wf03_ambiguous_write_never_auto_resent(pool: PgPool) {
    let x = x(pool).await;
    x.sent(X::ambiguous()).await;
    x.due().await;
    x.sweep().await;
    x.sweep().await;
    assert_eq!(x.posts(), 1);
    let inc = x.incidents().await;
    assert_eq!(inc[0].category.as_deref(), Some("unknown_outcome"));
}

#[sqlx::test]
async fn wf03_late_then_met_auto_verifies(pool: PgPool) {
    let x = x(pool).await;
    // The POST commits in NetSuite but the answer never comes back.
    let e = x.sent(Some(Post::CommitThenHang)).await;
    x.ns.lock().unwrap().miss_lookup = true;
    x.due().await;
    assert_eq!(x.sweep().await, vec![Checked::Late]);
    x.ns.lock().unwrap().miss_lookup = false;
    assert_eq!(x.sweep().await, vec![Checked::Met]);
    assert_eq!(x.exp(e).await.status, "met");
    let inc = x.incidents().await;
    assert_eq!(inc.len(), 1);
    assert_eq!(inc[0].status, "resolved");
    assert_eq!(inc[0].resolution_kind.as_deref(), Some("reconciled"));
    assert!(inc[0].verified);
    let reconcile: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'wf01_reconcile' AND payload->>'execution_id' = $1",
    )
    .bind(e.to_string())
    .fetch_one(&x.w.pool)
    .await
    .unwrap();
    assert_eq!(reconcile, 1, "WF-01 settles its own run");
    assert_eq!(x.posts(), 1);
}

#[sqlx::test]
async fn wf03_permanent_failure_not_retried(pool: PgPool) {
    let x = x(pool).await;
    let bad = fake_netsuite::ns_error(400, "INVALID_CONTENT", "Invalid item reference.");
    let e = x.sent(Some(Post::Reply(400, bad))).await;
    let exp = x.exp(e).await;
    assert_eq!(exp.status, "failed");
    x.due().await;
    let before = x.lookups();
    for _ in 0..3 {
        assert!(x.sweep().await.is_empty());
    }
    assert_eq!(x.lookups(), before, "no read-back calls");
    assert_eq!(x.posts(), 1);
    assert_eq!(x.exp(e).await.checks, 0);
    let inc = x.incidents().await;
    assert_eq!(inc.len(), 1);
    assert_eq!(inc[0].kind, "mapping_invalid");
    assert_eq!(inc[0].category.as_deref(), Some("data"));
    assert_eq!(inc[0].diagnosis.as_ref().unwrap()["code"], "validation");
}

#[sqlx::test]
async fn wf03_429_is_transient_and_backs_off(pool: PgPool) {
    let x = x(pool).await;
    let e = x.sent(Some(Post::CommitThenHang)).await;
    let busy = fake_netsuite::ns_error(
        429,
        "CONCURRENCY_LIMIT_EXCEEDED",
        "Concurrent request limit exceeded. Request blocked.",
    );
    x.ns.lock().unwrap().lookup_errors.push_back((429, busy));
    x.due().await;
    assert_eq!(x.sweep().await, vec![Checked::Busy]);
    assert_eq!(x.exp(e).await.status, "pending");
    assert!(x.incidents().await.is_empty());
    assert_eq!(x.sweep().await, vec![Checked::Met]);
    assert!(x.incidents().await.is_empty(), "no incident when later OK");
}

#[sqlx::test]
async fn wf03_manual_retry_one_post_same_eid(pool: PgPool) {
    let x = x(pool).await;
    let e = x.sent(X::ambiguous()).await;
    x.due().await;
    x.sweep().await;
    // A non-requester reconciler grants one more attempt (C2 allow_retry move).
    let mut tx = x.w.pool.begin().await.unwrap();
    let m = HumanMove {
        to: "queued",
        reason: None,
        resolution: None,
        phase: Some("prechecking"),
        grant: true,
    };
    wrepo::move_execution(&mut *tx, x.w.k.ws, e, &m)
        .await
        .unwrap();
    let r = NewResolution {
        execution_id: Some(e),
        incident_id: None,
        action: "allow_retry",
        actor: x.w.k.approver,
        reason: "Checked NetSuite: no order",
        target_id: None,
        outcome: "queued",
    };
    wrepo::insert_resolution(&mut *tx, x.w.k.ws, &r)
        .await
        .unwrap();
    // Past NetSuite's 15-minute window (test-only bypass of the set-once trigger).
    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE workflow_execution_attempts
         SET write_intent_at = write_intent_at - interval '16 minutes',
             sent_at = sent_at - interval '16 minutes' WHERE execution_id = $1",
    )
    .bind(e)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    x.exec(e).await;
    let posts = fake_netsuite::posts(&x.ns);
    assert_eq!(posts.len(), 2);
    let eid = |p: &fake_netsuite::Seen| {
        serde_json::from_str::<Value>(&p.body).unwrap()["externalId"].clone()
    };
    assert_eq!(eid(&posts[0]), eid(&posts[1]), "same externalId");
    assert_eq!(x.exp(e).await.status, "met");
    let inc = x.incidents().await;
    assert_eq!(inc[0].kind, "exchange_late");
    assert_eq!(inc[0].resolution_kind.as_deref(), Some("reconciled"));
}

#[sqlx::test]
async fn wf03_target_changed_since_send_no_write(pool: PgPool) {
    let x = x(pool).await;
    let e = x.sent(Some(Post::CommitThenHang)).await;
    x.ns.lock().unwrap().tamper = Some(|po| po["item"]["items"][0]["rate"] = json!(149.99));
    x.due().await;
    assert_eq!(x.sweep().await, vec![Checked::Failed]);
    let exp = x.exp(e).await;
    assert_eq!(exp.status, "failed");
    let inc = x.incidents().await;
    assert_eq!(inc.len(), 1);
    assert_eq!(inc[0].kind, "target_conflict");
    assert!(exp.incident_id.is_some());
    assert_eq!(fake_netsuite::writes_other_than_post(&x.ns), 0);
    assert_eq!(x.posts(), 1);
}

#[sqlx::test]
async fn wf03_check_budget_stops_at_10(pool: PgPool) {
    let x = x(pool).await;
    let e = x.sent(X::ambiguous()).await;
    x.due().await;
    let before = x.lookups();
    for _ in 0..12 {
        x.sweep().await;
    }
    assert_eq!(x.lookups() - before, 10);
    let exp = x.exp(e).await;
    assert_eq!(
        (exp.status.as_str(), exp.checks),
        ("late", 10),
        "stays visible"
    );
}

#[sqlx::test]
async fn wf03_health_auth_failure_incident(pool: PgPool) {
    let x = x(pool).await;
    let id: Uuid =
        sqlx::query_scalar("SELECT target_connection_id FROM integration_pairs WHERE id = $1")
            .bind(x.w.k.pair)
            .fetch_one(&x.w.pool)
            .await
            .unwrap();
    let row = conns::find(&x.w.pool, x.w.k.ws, id, false)
        .await
        .unwrap()
        .unwrap();
    let checks = vec![CapabilityCheck {
        key: "auth.token".into(),
        ok: false,
        error_class: Some(ErrorClass::Auth),
        detail: Some("invalid_client".into()),
    }];
    health::record(&x.w.pool, &row, &checks).await.unwrap();
    health::record(&x.w.pool, &row, &checks).await.unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM integration_connections WHERE id = $1")
            .bind(id)
            .fetch_one(&x.w.pool)
            .await
            .unwrap();
    assert_eq!(status, "unauthorized");
    let inc = x.incidents().await;
    assert_eq!(inc.len(), 1, "open once");
    assert_eq!(inc[0].kind, "auth_failure");
    assert_eq!(inc[0].category.as_deref(), Some("authorization"));
}
