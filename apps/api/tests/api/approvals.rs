//! WF-01 approval requests, decisions and policies (contracts §5.3; PLAN tests 4-8, 21, 23).

use std::sync::Arc;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::Barrier;

use crate::common::{app, member_with, send, send_raw, signup};

const FP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FP2: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const APPROVE: &[&str] = &["integration.approve"];

/// Ada owns `acme` (with a WF-01 pair); Rita requests (no approve rights), Bea and Cy approve.
struct W {
    app: Router,
    pool: PgPool,
    ws: String,
    owner: String,
    pair: String,
    rita: String,
    bea: String,
    cy: String,
}

async fn world(pool: PgPool) -> W {
    let app = app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let rita = member_with(&app, &pool, &ws, "rita@example.com", &[]).await;
    let bea = member_with(&app, &pool, &ws, "bea@example.com", APPROVE).await;
    let cy = member_with(&app, &pool, &ws, "cy@example.com", APPROVE).await;
    let pair: String = sqlx::query_scalar(
        "WITH u AS (SELECT id FROM users WHERE email = 'ada@example.com'),
              s AS (INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
                    SELECT $1::uuid, 'odoo', 'o', '{}', u.id FROM u RETURNING id),
              t AS (INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
                    SELECT $1::uuid, 'netsuite', 'n', '{}', u.id FROM u RETURNING id)
         INSERT INTO integration_pairs (workspace_id, workflow_key, source_connection_id,
             target_connection_id, config, created_by)
         SELECT $1::uuid, 'wf01_purchase', s.id, t.id, '{}', u.id FROM s, t, u RETURNING id::text",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    W {
        app,
        pool,
        ws,
        owner,
        pair,
        rita,
        bea,
        cy,
    }
}

impl W {
    /// A request raised by `requester_email` in `status`, needing `perm`. Returns its id.
    async fn request(&self, requester_email: &str, status: &str, perm: &str) -> String {
        sqlx::query_scalar(
            "INSERT INTO approval_requests (workspace_id, workflow_key, pair_id, subject_ref, revision,
                 fingerprint, snapshot, amount_minor, currency_ref, currency_decimals,
                 required_permission, requester_user_id, status, invalidated_reason)
             SELECT $1::uuid, 'wf01_purchase', $2::uuid, 'odoo:x:purchase.order:' || gen_random_uuid(), 1,
                 $3, '{\"v\":1,\"lines\":[]}', 125000, 'EUR', 2, $4, u.id, $5,
                 CASE WHEN $5 = 'invalidated' THEN 'source_changed' END
             FROM users u WHERE u.email = $6 RETURNING id::text",
        )
        .bind(&self.ws)
        .bind(&self.pair)
        .bind(FP)
        .bind(perm)
        .bind(status)
        .bind(requester_email)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    fn uri(&self, rest: &str) -> String {
        format!("/v1/workspace/{}/approvals/{rest}", self.ws)
    }

    async fn decide(&self, who: &str, id: &str, action: &str, body: Value) -> (StatusCode, Value) {
        send(
            &self.app,
            Method::POST,
            &self.uri(&format!("requests/{id}/{action}")),
            Some(body),
            Some(who),
        )
        .await
    }

    async fn get(&self, who: &str, rest: &str) -> (StatusCode, Value) {
        send(&self.app, Method::GET, &self.uri(rest), None, Some(who)).await
    }

    async fn count(&self, sql: &'static str, id: &str) -> i64 {
        sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn executions(&self, id: &str) -> i64 {
        self.count(
            "SELECT count(*) FROM workflow_executions WHERE approval_request_id = $1::uuid",
            id,
        )
        .await
    }

    async fn decisions(&self, id: &str) -> i64 {
        self.count(
            "SELECT count(*) FROM approval_decisions WHERE request_id = $1::uuid",
            id,
        )
        .await
    }
}

#[sqlx::test]
async fn approve_queues_one_execution_and_returns_the_request(pool: PgPool) {
    let w = world(pool).await;
    let id = w
        .request("rita@example.com", "pending", "integration.approve")
        .await;
    let (s, b) = w
        .decide(&w.bea, &id, "approve", json!({"fingerprint": FP}))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let d = &b["data"];
    assert_eq!(d["status"], "approved");
    assert_eq!(d["can_decide"], false);
    assert_eq!(d["decision"]["decision"], "approve");
    assert_eq!(d["execution"]["status"], "queued");
    assert_eq!(d["execution"]["phase"], "prechecking");
    assert_eq!(d["execution"]["verified"], false);
    assert_eq!(
        d["amount"],
        json!({"minor": 125000, "currency_ref": "EUR", "decimals": 2})
    );
    assert_eq!(w.executions(&id).await, 1);
    assert_eq!(w.decisions(&id).await, 1);
    let events = w
        .count(
            "SELECT count(*) FROM workflow_events WHERE approval_request_id = $1::uuid AND kind = 'decision'",
            &id,
        )
        .await;
    assert_eq!(events, 1);
}

// PLAN test 5.
#[sqlx::test]
async fn two_approvers_at_once_one_wins(pool: PgPool) {
    let w = Arc::new(world(pool).await);
    let id = w
        .request("rita@example.com", "pending", "integration.approve")
        .await;
    let gate = Arc::new(Barrier::new(2));
    let tasks: Vec<_> = [w.bea.clone(), w.cy.clone()]
        .into_iter()
        .map(|who| {
            let (w, gate, id) = (w.clone(), gate.clone(), id.clone());
            tokio::spawn(async move {
                gate.wait().await;
                w.decide(&who, &id, "approve", json!({"fingerprint": FP}))
                    .await
            })
        })
        .collect();
    let mut got = vec![];
    for t in tasks {
        got.push(t.await.unwrap());
    }
    got.sort_by_key(|(s, _)| s.as_u16());
    assert_eq!(got[0].0, StatusCode::OK);
    assert_eq!(got[1].0, StatusCode::CONFLICT);
    assert_eq!(got[1].1["code"], "approval_already_decided");
    assert_eq!(w.decisions(&id).await, 1);
    assert_eq!(w.executions(&id).await, 1);
}

// PLAN test 6.
#[sqlx::test]
async fn requester_cannot_decide_even_as_owner(pool: PgPool) {
    let w = world(pool).await;
    // Bea holds approve but raised it.
    let id = w
        .request("bea@example.com", "pending", "integration.approve")
        .await;
    let (s, b) = w
        .decide(&w.bea, &id, "approve", json!({"fingerprint": FP}))
        .await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::FORBIDDEN, json!("four_eyes_required"))
    );
    let own = w
        .request("ada@example.com", "pending", "integration.approve_high")
        .await;
    let (s, b) = w
        .decide(&w.owner, &own, "approve", json!({"fingerprint": FP}))
        .await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::FORBIDDEN, json!("four_eyes_required"))
    );
    assert_eq!(w.decisions(&own).await + w.decisions(&id).await, 0);
}

#[sqlx::test]
async fn approve_high_is_needed_above_threshold_and_satisfies_approve(pool: PgPool) {
    let w = world(pool).await;
    let high = w
        .request("rita@example.com", "pending", "integration.approve_high")
        .await;
    let (s, b) = w
        .decide(&w.bea, &high, "approve", json!({"fingerprint": FP}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(b, json!({"message": "This action is unauthorized."}));
    let (s, _) = w
        .decide(
            &w.rita,
            &high,
            "reject",
            json!({"fingerprint": FP, "comment": "no"}),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "no rights at all");
    let hi = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "hi@example.com",
        &["integration.approve_high"],
    )
    .await;
    let low = w
        .request("rita@example.com", "pending", "integration.approve")
        .await;
    let (s, _) = w
        .decide(&hi, &low, "approve", json!({"fingerprint": FP}))
        .await;
    assert_eq!(s, StatusCode::OK);
    let used: String = sqlx::query_scalar(
        "SELECT permission_used FROM approval_decisions WHERE request_id = $1::uuid",
    )
    .bind(&low)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(used, "integration.approve_high");
    let (s, _) = w
        .decide(&hi, &high, "approve", json!({"fingerprint": FP}))
        .await;
    assert_eq!(s, StatusCode::OK);
}

// PLAN test 4: stale fingerprint, and an invalidated request.
#[sqlx::test]
async fn stale_or_invalidated_request_is_refused(pool: PgPool) {
    let w = world(pool).await;
    let id = w
        .request("rita@example.com", "pending", "integration.approve")
        .await;
    let (s, b) = w
        .decide(&w.bea, &id, "approve", json!({"fingerprint": FP2}))
        .await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("approval_stale"))
    );
    let gone = w
        .request("rita@example.com", "invalidated", "integration.approve")
        .await;
    let (s, b) = w
        .decide(&w.bea, &gone, "approve", json!({"fingerprint": FP}))
        .await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("approval_already_decided"))
    );
    assert_eq!(w.executions(&id).await + w.executions(&gone).await, 0);
    let (s, b) = w
        .decide(&w.bea, &id, "approve", json!({"fingerprint": "nope"}))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["fingerprint"].is_array());
}

// PLAN test 8.
#[sqlx::test]
async fn reject_and_return_need_a_comment_and_never_queue(pool: PgPool) {
    let w = world(pool).await;
    for action in ["reject", "return"] {
        let id = w
            .request("rita@example.com", "pending", "integration.approve")
            .await;
        let (s, b) = w
            .decide(&w.bea, &id, action, json!({"fingerprint": FP}))
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
        assert_eq!(b["errors"]["comment"][0], "The comment field is required.");
        let (s, b) = w
            .decide(
                &w.bea,
                &id,
                action,
                json!({"fingerprint": FP, "comment": "Wrong vendor"}),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let want = if action == "reject" {
            "rejected"
        } else {
            "returned"
        };
        assert_eq!(b["data"]["status"], want);
        assert_eq!(b["data"]["decision"]["comment"], "Wrong vendor");
        assert!(b["data"]["execution"].is_null());
        assert_eq!(w.executions(&id).await, 0);
    }
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind LIKE 'wf01%'")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
}

#[sqlx::test]
async fn idempotency_key_replays_the_decision(pool: PgPool) {
    let w = world(pool).await;
    let id = w
        .request("rita@example.com", "pending", "integration.approve")
        .await;
    let uri = w.uri(&format!("requests/{id}/approve"));
    let body = json!({"fingerprint": FP});
    let call = || {
        send_raw(
            &w.app,
            Method::POST,
            &uri,
            Some(body.clone()),
            Some(&w.bea),
            &[("idempotency-key", "k1")],
        )
    };
    let (s1, b1) = call().await;
    let (s2, b2) = call().await;
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
    assert_eq!(b1, b2);
    assert_eq!(w.decisions(&id).await, 1);
}

// PLAN test 21.
#[sqlx::test]
async fn other_workspace_gets_404(pool: PgPool) {
    let w = world(pool).await;
    let id = w
        .request("rita@example.com", "pending", "integration.approve")
        .await;
    let (eve, eve_ws) = signup(&w.app, "eve@example.com", "evil").await;
    // Eve owns her own workspace and holds every permission there.
    let foreign = format!("/v1/workspace/{eve_ws}/approvals/requests/{id}");
    for (m, uri, body) in [
        (Method::GET, foreign.clone(), None),
        (
            Method::POST,
            format!("{foreign}/approve"),
            Some(json!({"fingerprint": FP})),
        ),
        (Method::GET, w.uri(&format!("requests/{id}")), None),
        (
            Method::POST,
            w.uri(&format!("requests/{id}/approve")),
            Some(json!({"fingerprint": FP})),
        ),
    ] {
        let (s, _) = send(&w.app, m, &uri, body, Some(&eve)).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{uri}");
    }
    let (s, _) = w
        .get(&w.bea, "requests/00000000-0000-0000-0000-000000000000")
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = w.get(&w.bea, "requests/not-a-uuid").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(w.decisions(&id).await, 0);
}

// PLAN test 23 + list scopes.
#[sqlx::test]
async fn list_shows_approval_execution_and_incident_apart(pool: PgPool) {
    let w = world(pool).await;
    let mine = w
        .request("rita@example.com", "pending", "integration.approve")
        .await;
    let other = w
        .request("bea@example.com", "pending", "integration.approve")
        .await;
    let (s, _) = w
        .decide(&w.cy, &mine, "approve", json!({"fingerprint": FP}))
        .await;
    assert_eq!(s, StatusCode::OK);
    sqlx::query(
        "INSERT INTO workflow_incidents (workspace_id, kind, subject_ref, approval_request_id, status, detail)
         SELECT workspace_id, 'duplicate_obligation_risk', subject_ref, id, 'open', '{}'
         FROM approval_requests WHERE id = $1::uuid",
    )
    .bind(&mine)
    .execute(&w.pool)
    .await
    .unwrap();
    let (s, b) = w.get(&w.bea, "requests?scope=all").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["meta"]["total"], 2);
    let row = b["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == mine.as_str())
        .unwrap();
    assert_eq!(row["status"], "approved");
    assert_eq!(row["execution"]["status"], "queued");
    assert_eq!(
        row["open_incidents"][0]["kind"],
        "duplicate_obligation_risk"
    );
    assert!(row.get("snapshot").is_none(), "list rows have no snapshot");
    // Rita has no integration rights: scope=all is forced to mine.
    let (_, b) = w.get(&w.rita, "requests?scope=all").await;
    let ids: Vec<&str> = b["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![mine.as_str()]);
    let (s, _) = w.get(&w.rita, &format!("requests/{other}")).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "not hers, no rights");
    let (s, b) = w.get(&w.rita, &format!("requests/{mine}")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["snapshot"]["v"], 1);
    // Decidable for Cy: Bea's pending one only.
    let (_, b) = w.get(&w.cy, "requests?scope=decidable").await;
    let ids: Vec<&str> = b["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![other.as_str()]);
    assert_eq!(b["data"][0]["can_decide"], true);
    let (_, b) = w.get(&w.bea, "requests?scope=decidable").await;
    assert_eq!(b["meta"]["total"], 0, "own request is not decidable");
    let (s, b) = w.get(&w.bea, "requests?status=weird").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b["errors"]["status"][0], "The selected status is invalid.");
}

#[sqlx::test]
async fn resubmit_creates_next_revision_once(pool: PgPool) {
    let w = world(pool).await;
    let id = w
        .request("rita@example.com", "pending", "integration.approve")
        .await;
    let uri = w.uri(&format!("requests/{id}/resubmit"));
    let (s, b) = send(&w.app, Method::POST, &uri, Some(json!({})), Some(&w.rita)).await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("approval_not_resubmittable"))
    );
    w.decide(
        &w.bea,
        &id,
        "reject",
        json!({"fingerprint": FP, "comment": "no"}),
    )
    .await;
    let (s, _) = send(&w.app, Method::POST, &uri, Some(json!({})), Some(&w.cy)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "not the requester, no manage");
    let (s, b) = send(
        &w.app,
        Method::POST,
        &uri,
        Some(json!({"comment": "fixed"})),
        Some(&w.rita),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let next = b["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, b) = w.get(&w.rita, &format!("requests/{next}")).await;
    assert_eq!(b["data"]["revision"], 2);
    assert_eq!(b["data"]["status"], "pending");
    assert_eq!(b["data"]["fingerprint"], FP);
    let (s, b) = send(&w.app, Method::POST, &uri, Some(json!({})), Some(&w.rita)).await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("approval_not_resubmittable"))
    );
}

#[sqlx::test]
async fn policies_crud(pool: PgPool) {
    let w = world(pool).await;
    let uri = w.uri("policies");
    let body =
        json!({"workflow_key": "wf01_purchase", "currency_ref": "EUR", "threshold_minor": 500000});
    let (s, _) = send(&w.app, Method::POST, &uri, Some(body.clone()), Some(&w.bea)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, b) = send(
        &w.app,
        Method::POST,
        &uri,
        Some(body.clone()),
        Some(&w.owner),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let pid = b["data"]["policy_id"].as_str().unwrap().to_owned();
    let (s, b) = send(&w.app, Method::POST, &uri, Some(body), Some(&w.owner)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        b["errors"]["currency_ref"][0],
        "An active policy already exists for this currency."
    );
    let (s, b) = send(
        &w.app,
        Method::POST,
        &uri,
        Some(json!({"workflow_key": "nope", "currency_ref": "", "threshold_minor": -1})),
        Some(&w.owner),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    for f in ["workflow_key", "currency_ref", "threshold_minor"] {
        assert!(b["errors"][f].is_array(), "{f}: {b}");
    }
    let (s, b) = send(
        &w.app,
        Method::POST,
        &format!("{uri}/{pid}"),
        Some(json!({"threshold_minor": 1000, "active": false})),
        Some(&w.owner),
    )
    .await;
    assert_eq!((s, b), (StatusCode::OK, json!({"message": "ok"})));
    let (s, b) = w.get(&w.owner, "policies").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"][0]["threshold_minor"], 1000);
    assert_eq!(b["data"][0]["active"], false);
    let (s, _) = w.get(&w.rita, "policies").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = send(
        &w.app,
        Method::POST,
        &format!("{uri}/00000000-0000-0000-0000-000000000000"),
        Some(json!({"active": true})),
        Some(&w.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
