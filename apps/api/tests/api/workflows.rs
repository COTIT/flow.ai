//! WF-01 executions and incidents routes (contracts §5.5, §10): list/get, retry_after_fix,
//! human resolutions with four-eyes, incident resolve, cross-workspace 404.

use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::common::{app, member_with, send, signup};

const FP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Ada owns `acme`; Rita raised the request; Rex reconciles; Mo manages; Vi views.
struct W {
    app: Router,
    pool: PgPool,
    ws: String,
    rita: String,
    rex: String,
    mo: String,
    vi: String,
    req: String,
    exec: String,
}

async fn world(pool: PgPool) -> W {
    let app = app(pool.clone());
    let (_owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let rita = member_with(
        &app,
        &pool,
        &ws,
        "rita@example.com",
        &["integration.reconcile"],
    )
    .await;
    let rex = member_with(
        &app,
        &pool,
        &ws,
        "rex@example.com",
        &["integration.reconcile"],
    )
    .await;
    let mo = member_with(&app, &pool, &ws, "mo@example.com", &["integration.manage"]).await;
    let vi = member_with(&app, &pool, &ws, "vi@example.com", &["integration.view"]).await;
    let (req, exec): (String, String) = sqlx::query_as(
        "WITH u AS (SELECT id FROM users WHERE email = 'ada@example.com'),
              r0 AS (SELECT id FROM users WHERE email = 'rita@example.com'),
              s AS (INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
                    SELECT $1::uuid, 'odoo', 'o', '{}', u.id FROM u RETURNING id),
              t AS (INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
                    SELECT $1::uuid, 'netsuite', 'n', '{}', u.id FROM u RETURNING id),
              p AS (INSERT INTO integration_pairs (workspace_id, workflow_key, source_connection_id,
                        target_connection_id, config, created_by)
                    SELECT $1::uuid, 'wf01_purchase', s.id, t.id, '{}', u.id FROM s, t, u RETURNING id),
              r AS (INSERT INTO approval_requests (workspace_id, workflow_key, pair_id, subject_ref, revision,
                        fingerprint, snapshot, amount_minor, currency_ref, currency_decimals,
                        required_permission, requester_user_id, status, decided_by, decided_at)
                    SELECT $1::uuid, 'wf01_purchase', p.id, 'odoo:x:purchase.order:17', 1, $2, '{}', 100,
                        'EUR', 2, 'integration.approve', r0.id, 'approved', u.id, now()
                    FROM p, r0, u RETURNING id),
              e AS (INSERT INTO workflow_executions (workspace_id, approval_request_id, workflow_key,
                        correlation_id, status)
                    SELECT $1::uuid, r.id, 'wf01_purchase', 'vz-0f1e2d3c4b5a69788796a5b4c3d2e1f0', 'queued'
                    FROM r RETURNING id)
         SELECT r.id::text, e.id::text FROM r, e",
    )
    .bind(&ws)
    .bind(FP)
    .fetch_one(&pool)
    .await
    .unwrap();
    W {
        app,
        pool,
        ws,
        rita,
        rex,
        mo,
        vi,
        req,
        exec,
    }
}

impl W {
    fn uri(&self, rest: &str) -> String {
        format!("/v1/workspace/{}/workflows/{rest}", self.ws)
    }

    async fn sql(&self, sql: &'static str) {
        sqlx::query(sql)
            .bind(&self.exec)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// Execution after one sent attempt with `outcome`, now in `status`.
    async fn after_attempt(&self, status: &str, outcome: &str) {
        self.sql(
            "INSERT INTO workflow_execution_attempts (workspace_id, execution_id, attempt_no, sent_at)
             SELECT workspace_id, id, 1, now() FROM workflow_executions WHERE id = $1::uuid",
        )
        .await;
        sqlx::query(
            "UPDATE workflow_execution_attempts SET outcome = $2 WHERE execution_id = $1::uuid",
        )
        .bind(&self.exec)
        .bind(outcome)
        .execute(&self.pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE workflow_executions SET status = $2, phase = 'write_sent', remote_attempts = 1,
                 reason = CASE WHEN $2 = 'blocked' THEN 'validation_rejected' END
             WHERE id = $1::uuid",
        )
        .bind(&self.exec)
        .bind(status)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn post(&self, who: &str, rest: &str, body: Value) -> (StatusCode, Value) {
        send(
            &self.app,
            Method::POST,
            &self.uri(rest),
            Some(body),
            Some(who),
        )
        .await
    }

    async fn resolve(&self, who: &str, body: Value) -> (StatusCode, Value) {
        self.post(who, &format!("executions/{}/resolve", self.exec), body)
            .await
    }

    async fn status(&self) -> (String, i16, i16, Option<String>) {
        sqlx::query_as(
            "SELECT status, manual_retry_grants, remote_attempts, resolution FROM workflow_executions
             WHERE id = $1::uuid",
        )
        .bind(&self.exec)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn resolutions(&self) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT action FROM workflow_resolutions WHERE execution_id = $1::uuid ORDER BY id",
        )
        .bind(&self.exec)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

#[sqlx::test]
async fn list_and_detail_show_attempts_events_and_actions(pool: PgPool) {
    let w = world(pool).await;
    w.after_attempt("awaiting_reconciliation", "ambiguous")
        .await;
    let (s, b) = send(
        &w.app,
        Method::GET,
        &w.uri("executions?status=awaiting_reconciliation"),
        None,
        Some(&w.vi),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["meta"]["total"], 1);
    let row = &b["data"][0];
    assert_eq!(row["approval_request_id"], w.req.as_str());
    assert_eq!(row["verified"], false);
    assert_eq!(
        row["allowed_actions"],
        json!(["link_existing", "allow_retry", "close_no_po"])
    );
    let (s, b) = send(
        &w.app,
        Method::GET,
        &w.uri(&format!("executions/{}", w.exec)),
        None,
        Some(&w.vi),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["attempts"][0]["outcome"], "ambiguous");
    assert!(b["data"]["events"].is_array());
    assert!(b["data"]["resolutions"].is_array());
    let (s, _) = send(&w.app, Method::GET, &w.uri("executions"), None, Some(&w.mo)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "manage alone is not view");
}

// T27 (manual grant) + four-eyes + audit.
#[sqlx::test]
async fn allow_retry_grants_one_attempt_by_a_non_requester(pool: PgPool) {
    let w = world(pool).await;
    w.after_attempt("awaiting_reconciliation", "ambiguous")
        .await;
    let body = json!({"action": "allow_retry", "reason": "Checked NetSuite: no order"});
    let (s, b) = w.resolve(&w.rita, body.clone()).await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::FORBIDDEN, json!("four_eyes_required"))
    );
    let (s, _) = w.resolve(&w.vi, body.clone()).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "view cannot reconcile");
    let (s, b) = w.resolve(&w.rex, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "queued");
    assert_eq!(b["data"]["phase"], "prechecking");
    assert_eq!(w.status().await, ("queued".into(), 1, 1, None));
    assert_eq!(w.resolutions().await, vec!["allow_retry"]);
    let (s, b) = w.resolve(&w.rex, body).await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("execution_state_changed"))
    );
}

#[sqlx::test]
async fn allow_retry_needs_a_settled_attempt(pool: PgPool) {
    let w = world(pool).await;
    // Attempt still in flight (no outcome): never a second POST.
    w.sql(
        "INSERT INTO workflow_execution_attempts (workspace_id, execution_id, attempt_no)
         SELECT workspace_id, id, 1 FROM workflow_executions WHERE id = $1::uuid",
    )
    .await;
    w.sql("UPDATE workflow_executions SET status = 'awaiting_reconciliation', remote_attempts = 1, phase = 'write_intent' WHERE id = $1::uuid").await;
    let (s, b) = w
        .resolve(&w.rex, json!({"action": "allow_retry", "reason": "r"}))
        .await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("execution_state_changed"))
    );
}

#[sqlx::test]
async fn close_no_po_and_link_seam(pool: PgPool) {
    let w = world(pool).await;
    w.after_attempt("awaiting_reconciliation", "ambiguous")
        .await;
    let (s, b) = w
        .resolve(
            &w.rex,
            json!({"action": "link_existing", "reason": "Found"}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["target_id"].is_array(), "{b}");
    let (s, b) = w
        .resolve(
            &w.rex,
            json!({"action": "link_existing", "reason": "Found", "target_id": "8812"}),
        )
        .await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::SERVICE_UNAVAILABLE, json!("link_unavailable"))
    );
    let (s, b) = w
        .resolve(&w.rex, json!({"action": "nope", "reason": "x"}))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["action"].is_array());
    let (s, b) = w
        .resolve(
            &w.rex,
            json!({"action": "close_no_po", "reason": "Not in NetSuite"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "resolved_manual");
    assert_eq!(b["data"]["resolution"], "closed_no_po");
    assert_eq!(b["data"]["verified"], false);
    assert_eq!(w.status().await.3.as_deref(), Some("closed_no_po"));
}

#[sqlx::test]
async fn variance_accept_and_reject(pool: PgPool) {
    let w = world(pool).await;
    w.after_attempt("awaiting_reconciliation", "created").await;
    w.sql("UPDATE workflow_executions SET status = 'succeeded_with_variance', phase = 'done', target_ref = 'netsuite:c:purchaseOrder:1' WHERE id = $1::uuid").await;
    w.sql(
        "INSERT INTO workflow_incidents (workspace_id, kind, subject_ref, execution_id, status, detail)
         SELECT workspace_id, 'tax_variance', 'odoo:x:purchase.order:17', id, 'open', '{}'
         FROM workflow_executions WHERE id = $1::uuid",
    )
    .await;
    let (s, b) = w
        .resolve(
            &w.rex,
            json!({"action": "reject_variance", "reason": "Ask vendor"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "succeeded_with_variance");
    let (s, b) = w
        .resolve(
            &w.rex,
            json!({"action": "accept_variance", "reason": "1 cent"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["resolution"], "variance_accepted");
    let open: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workflow_incidents WHERE status = 'open'")
            .fetch_one(&w.pool)
            .await
            .unwrap();
    assert_eq!(open, 0, "accepting closes the tax_variance incident");
    assert_eq!(
        w.resolutions().await,
        vec!["reject_variance", "accept_variance", "resolve_incident"]
    );
    let (s, _) = w
        .resolve(&w.rex, json!({"action": "close_no_po", "reason": "x"}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

// §10: retry_after_fix after a definite rejection needs a grant.
#[sqlx::test]
async fn retry_after_fix_rules(pool: PgPool) {
    let w = world(pool).await;
    let retry = format!("executions/{}/retry", w.exec);
    let (s, b) = w
        .post(&w.mo, &retry, json!({"reason": "Vendor mapped"}))
        .await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("execution_state_changed")),
        "queued is not blocked"
    );
    w.after_attempt("blocked", "rejected").await;
    let (s, _) = w.post(&w.rex, &retry, json!({"reason": "x"})).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "needs manage");
    let (s, b) = w
        .post(&w.mo, &retry, json!({"reason": "Vendor mapped"}))
        .await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("retry_needs_grant"))
    );
    let (s, b) = w
        .resolve(
            &w.rex,
            json!({"action": "allow_retry", "reason": "Fixed vendor"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "queued");
    // Blocked before any attempt: retry just requeues.
    w.sql("UPDATE workflow_executions SET status = 'blocked', reason = 'mapping_stale' WHERE id = $1::uuid").await;
    let (s, b) = w
        .post(&w.mo, &retry, json!({"reason": "Mapping fixed"}))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "queued");
    assert_eq!(b["data"]["reason"], Value::Null);
    assert_eq!(
        w.resolutions().await,
        vec!["allow_retry", "retry_after_fix"]
    );
    let (s, b) = w.post(&w.mo, &retry, json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["reason"].is_array());
}

#[sqlx::test]
async fn incidents_list_and_resolve_once(pool: PgPool) {
    let w = world(pool).await;
    let inc: String = sqlx::query_scalar(
        "INSERT INTO workflow_incidents (workspace_id, kind, subject_ref, approval_request_id, execution_id, status, detail)
         SELECT workspace_id, 'duplicate_obligation_risk', 'odoo:x:purchase.order:17', approval_request_id, id, 'open',
             '{\"source_state\":\"purchase\"}'
         FROM workflow_executions WHERE id = $1::uuid RETURNING id::text",
    )
    .bind(&w.exec)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let (s, b) = send(
        &w.app,
        Method::GET,
        &w.uri("incidents?status=open"),
        None,
        Some(&w.vi),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"][0]["kind"], "duplicate_obligation_risk");
    assert_eq!(b["data"][0]["detail"]["source_state"], "purchase");
    let path = format!("incidents/{inc}/resolve");
    let (s, b) = w.post(&w.rita, &path, json!({"note": "done"})).await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::FORBIDDEN, json!("four_eyes_required"))
    );
    let (s, b) = w.post(&w.rex, &path, json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["note"].is_array());
    let (s, b) = w
        .post(&w.rex, &path, json!({"note": "Odoo PO cancelled by hand"}))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "resolved");
    assert_eq!(b["data"]["note"], "Odoo PO cancelled by hand");
    let (s, b) = w.post(&w.rex, &path, json!({"note": "again"})).await;
    assert_eq!(
        (s, b["code"].clone()),
        (StatusCode::CONFLICT, json!("incident_already_resolved"))
    );
}

// PLAN test 21 for executions and incidents.
#[sqlx::test]
async fn other_workspace_gets_404(pool: PgPool) {
    let w = world(pool).await;
    let (eve, eve_ws) = signup(&w.app, "eve@example.com", "evil").await;
    let own = format!("/v1/workspace/{eve_ws}/workflows/executions/{}", w.exec);
    for (m, uri, body) in [
        (Method::GET, own.clone(), None),
        (
            Method::POST,
            format!("{own}/resolve"),
            Some(json!({"action": "close_no_po", "reason": "x"})),
        ),
        (
            Method::POST,
            format!("{own}/retry"),
            Some(json!({"reason": "x"})),
        ),
        (Method::GET, w.uri(&format!("executions/{}", w.exec)), None),
        (Method::GET, w.uri("incidents"), None),
    ] {
        let (s, _) = send(&w.app, m, &uri, body, Some(&eve)).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{uri}");
    }
    let (s, _) = send(
        &w.app,
        Method::GET,
        &w.uri("executions/not-a-uuid"),
        None,
        Some(&w.vi),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = w
        .post(
            &w.rex,
            "incidents/00000000-0000-0000-0000-000000000000/resolve",
            json!({"note": "n"}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
