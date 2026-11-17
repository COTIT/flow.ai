//! Approvals + workflow tables (WF-01 C2): CHECKs, four-eyes FK, live partial uniques,
//! append-only and set-once triggers, composite workspace FKs, exercised in raw SQL.

use sqlx::PgPool;
use uuid::Uuid;

/// Workspace with a requester, an approver, two connections and a WF-01 pair.
pub struct Fx {
    pub ws: Uuid,
    pub requester: Uuid,
    pub approver: Uuid,
    pub pair: Uuid,
    pub conn: Uuid,
}

pub async fn user(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, first_name, last_name, password_hash)
         VALUES ($1, 'U', 'X', 'h') RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn fx(pool: &PgPool, name: &str) -> Fx {
    let requester = user(pool, &format!("req-{name}@x.io")).await;
    let approver = user(pool, &format!("apr-{name}@x.io")).await;
    let ws: Uuid = sqlx::query_scalar(
        "INSERT INTO workspaces (name, created_by_user_id) VALUES ($2, $1) RETURNING id",
    )
    .bind(requester)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap();
    let conn: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
         VALUES ($1, 'odoo', 'o', '{}', $2) RETURNING id",
    )
    .bind(ws)
    .bind(requester)
    .fetch_one(pool)
    .await
    .unwrap();
    let target: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
         VALUES ($1, 'netsuite', 'n', '{}', $2) RETURNING id",
    )
    .bind(ws)
    .bind(requester)
    .fetch_one(pool)
    .await
    .unwrap();
    let pair: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_pairs (workspace_id, workflow_key, source_connection_id,
             target_connection_id, config, created_by)
         VALUES ($1, 'wf01_purchase', $2, $3, '{}', $4) RETURNING id",
    )
    .bind(ws)
    .bind(conn)
    .bind(target)
    .bind(requester)
    .fetch_one(pool)
    .await
    .unwrap();
    Fx {
        ws,
        requester,
        approver,
        pair,
        conn,
    }
}

pub const FP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Insert a request row; `status` pending/approved/blocked etc.
async fn request(
    pool: &PgPool,
    f: &Fx,
    ws: Uuid,
    subject: &str,
    rev: i32,
    status: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO approval_requests (workspace_id, workflow_key, pair_id, subject_ref, revision,
             fingerprint, snapshot, amount_minor, currency_ref, currency_decimals,
             required_permission, requester_user_id, status, blocked_reasons,
             invalidated_reason, decided_by, decided_at)
         VALUES ($1, 'wf01_purchase', $2, $3, $4, $5, '{}', 100, 'EUR', 2,
             CASE WHEN $6 = 'blocked' THEN NULL ELSE 'integration.approve' END, $7, $6,
             CASE WHEN $6 = 'blocked' THEN '{no_requester}'::text[] ELSE '{}' END,
             CASE WHEN $6 = 'invalidated' THEN 'source_changed' END,
             CASE WHEN $6 IN ('approved','rejected','returned') THEN $8::uuid END,
             CASE WHEN $6 IN ('approved','rejected','returned') THEN now() END)
         RETURNING id",
    )
    .bind(ws)
    .bind(f.pair)
    .bind(subject)
    .bind(rev)
    .bind(FP)
    .bind(status)
    .bind(f.requester)
    .bind(f.approver)
    .fetch_one(pool)
    .await
}

async fn execution(pool: &PgPool, f: &Fx, req: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO workflow_executions (workspace_id, approval_request_id, workflow_key,
             correlation_id, status)
         VALUES ($1, $2, 'wf01_purchase', 'vz-' || replace(gen_random_uuid()::text, '-', ''), 'queued')
         RETURNING id",
    )
    .bind(f.ws)
    .bind(req)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// SQLSTATE of a failed statement.
fn code<T: std::fmt::Debug>(r: Result<T, sqlx::Error>) -> String {
    let e = r.expect_err("statement should fail");
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_else(|| panic!("not a database error: {e}"))
}

const CHECK: &str = "23514";
const UNIQUE: &str = "23505";
const FK: &str = "23503";
const RESTRICT: &str = "23001";

#[sqlx::test]
async fn one_live_request_per_subject_and_revisions_unique(pool: PgPool) {
    let f = fx(&pool, "a").await;
    let r1 = request(&pool, &f, f.ws, "odoo:x:purchase.order:1", 1, "pending")
        .await
        .unwrap();
    assert_eq!(
        code(request(&pool, &f, f.ws, "odoo:x:purchase.order:1", 2, "blocked").await),
        UNIQUE
    );
    sqlx::query(
        "UPDATE approval_requests SET status = 'invalidated', invalidated_reason = 'source_changed'
         WHERE id = $1",
    )
    .bind(r1)
    .execute(&pool)
    .await
    .unwrap();
    request(&pool, &f, f.ws, "odoo:x:purchase.order:1", 2, "pending")
        .await
        .unwrap();
    assert_eq!(
        code(request(&pool, &f, f.ws, "odoo:x:purchase.order:1", 2, "rejected").await),
        UNIQUE
    );
}

#[sqlx::test]
async fn request_checks_hold(pool: PgPool) {
    let f = fx(&pool, "b").await;
    let r = request(&pool, &f, f.ws, "s:1", 1, "pending").await.unwrap();
    // decided_by = requester refused (four eyes).
    let self_decide = sqlx::query(
        "UPDATE approval_requests SET status = 'approved', decided_by = requester_user_id,
             decided_at = now() WHERE id = $1",
    )
    .bind(r)
    .execute(&pool)
    .await;
    assert_eq!(code(self_decide), CHECK);
    // approved without decided_by refused.
    let undecided = sqlx::query("UPDATE approval_requests SET status = 'approved' WHERE id = $1")
        .bind(r)
        .execute(&pool)
        .await;
    assert_eq!(code(undecided), CHECK);
    // blocked with no reasons refused.
    let blocked = sqlx::query(
        "UPDATE approval_requests SET status = 'blocked', required_permission = NULL WHERE id = $1",
    )
    .bind(r)
    .execute(&pool)
    .await;
    assert_eq!(code(blocked), CHECK);
    // fingerprint must be 64 hex.
    let fp = sqlx::query("UPDATE approval_requests SET fingerprint = 'XYZ' WHERE id = $1")
        .bind(r)
        .execute(&pool)
        .await;
    assert_eq!(code(fp), CHECK);
}

#[sqlx::test]
async fn decisions_are_four_eyes_once_and_append_only(pool: PgPool) {
    let f = fx(&pool, "c").await;
    let r = request(&pool, &f, f.ws, "s:1", 1, "approved")
        .await
        .unwrap();
    let ins = |actor: Uuid, requester: Uuid| {
        sqlx::query(
            "INSERT INTO approval_decisions (workspace_id, request_id, requester_user_id,
                 actor_user_id, decision, fingerprint_seen, permission_used)
             VALUES ($1, $2, $3, $4, 'approve', $5, 'integration.approve')",
        )
        .bind(f.ws)
        .bind(r)
        .bind(requester)
        .bind(actor)
        .bind(FP)
        .execute(&pool)
    };
    assert_eq!(code(ins(f.requester, f.requester).await), CHECK);
    // Lying about the requester breaks the composite FK.
    assert_eq!(code(ins(f.requester, f.approver).await), FK);
    ins(f.approver, f.requester).await.unwrap();
    assert_eq!(code(ins(f.approver, f.requester).await), UNIQUE);
    let upd = sqlx::query("UPDATE approval_decisions SET comment = 'x'")
        .execute(&pool)
        .await;
    assert_eq!(code(upd), RESTRICT);
    let del = sqlx::query("DELETE FROM approval_decisions")
        .execute(&pool)
        .await;
    assert_eq!(code(del), RESTRICT);
    // reject/return need a comment.
    let r2 = request(&pool, &f, f.ws, "s:2", 1, "rejected")
        .await
        .unwrap();
    let no_comment = sqlx::query(
        "INSERT INTO approval_decisions (workspace_id, request_id, requester_user_id,
             actor_user_id, decision, fingerprint_seen, permission_used)
         VALUES ($1, $2, $3, $4, 'reject', $5, 'integration.approve')",
    )
    .bind(f.ws)
    .bind(r2)
    .bind(f.requester)
    .bind(f.approver)
    .bind(FP)
    .execute(&pool)
    .await;
    assert_eq!(code(no_comment), CHECK);
}

#[sqlx::test]
async fn execution_checks_cancel_i2_and_attempt_cap(pool: PgPool) {
    let f = fx(&pool, "d").await;
    let r = request(&pool, &f, f.ws, "s:1", 1, "approved")
        .await
        .unwrap();
    let e = execution(&pool, &f, r).await;
    let set = |sql: &'static str| sqlx::query(sql).bind(e).execute(&pool);
    // Cap: remote_attempts <= 1 + manual_retry_grants.
    assert_eq!(
        code(set("UPDATE workflow_executions SET remote_attempts = 2 WHERE id = $1").await),
        CHECK
    );
    set("UPDATE workflow_executions SET remote_attempts = 1 WHERE id = $1")
        .await
        .unwrap();
    // I2: cancelled only before any remote attempt.
    assert_eq!(
        code(
            set(
                "UPDATE workflow_executions SET status = 'cancelled', reason = 'source_changed'
                 WHERE id = $1"
            )
            .await
        ),
        CHECK
    );
    // succeeded needs target_ref.
    assert_eq!(
        code(set("UPDATE workflow_executions SET status = 'succeeded' WHERE id = $1").await),
        CHECK
    );
    // resolved_manual needs resolution; blocked needs reason.
    assert_eq!(
        code(
            set("UPDATE workflow_executions SET status = 'resolved_manual', reason = 'x' WHERE id = $1")
                .await
        ),
        CHECK
    );
    assert_eq!(
        code(set("UPDATE workflow_executions SET status = 'blocked' WHERE id = $1").await),
        CHECK
    );
    // A second execution for the same request refused.
    let dup = sqlx::query(
        "INSERT INTO workflow_executions (workspace_id, approval_request_id, workflow_key,
             correlation_id, status)
         VALUES ($1, $2, 'wf01_purchase', 'vz-00000000000000000000000000000000', 'queued')",
    )
    .bind(f.ws)
    .bind(r)
    .execute(&pool)
    .await;
    assert_eq!(code(dup), UNIQUE);
}

#[sqlx::test]
async fn attempts_are_set_once(pool: PgPool) {
    let f = fx(&pool, "e").await;
    let r = request(&pool, &f, f.ws, "s:1", 1, "approved")
        .await
        .unwrap();
    let e = execution(&pool, &f, r).await;
    let ins = |no: i16, granted: Option<Uuid>| {
        sqlx::query(
            "INSERT INTO workflow_execution_attempts (workspace_id, execution_id, attempt_no,
                 granted_by, grant_reason)
             VALUES ($1, $2, $3, $4, CASE WHEN $4 IS NULL THEN NULL ELSE 'why' END)",
        )
        .bind(f.ws)
        .bind(e)
        .bind(no)
        .bind(granted)
        .execute(&pool)
    };
    // Attempt 1 has no grant; later ones need one.
    assert_eq!(code(ins(1, Some(f.approver)).await), CHECK);
    assert_eq!(code(ins(2, None).await), CHECK);
    ins(1, None).await.unwrap();
    let q = |sql: &'static str| sqlx::query(sql).bind(e).execute(&pool);
    q("UPDATE workflow_execution_attempts SET sent_at = now() WHERE execution_id = $1")
        .await
        .unwrap();
    q(
        "UPDATE workflow_execution_attempts SET outcome = 'ambiguous', evidence = '{}'
       WHERE execution_id = $1",
    )
    .await
    .unwrap();
    for sql in [
        "UPDATE workflow_execution_attempts SET sent_at = now() WHERE execution_id = $1",
        "UPDATE workflow_execution_attempts SET outcome = 'created' WHERE execution_id = $1",
        "UPDATE workflow_execution_attempts SET attempt_no = 3 WHERE execution_id = $1",
        "DELETE FROM workflow_execution_attempts WHERE execution_id = $1",
    ] {
        assert_eq!(code(q(sql).await), RESTRICT, "{sql}");
    }
}

#[sqlx::test]
async fn events_and_resolutions_are_append_only(pool: PgPool) {
    let f = fx(&pool, "f").await;
    let r = request(&pool, &f, f.ws, "s:1", 1, "approved")
        .await
        .unwrap();
    let e = execution(&pool, &f, r).await;
    sqlx::query(
        "INSERT INTO workflow_events (workspace_id, execution_id, kind, payload, actor_kind)
         VALUES ($1, $2, 'claim', '{}', 'system')",
    )
    .bind(f.ws)
    .bind(e)
    .execute(&pool)
    .await
    .unwrap();
    // user actor needs actor_user_id.
    let bad = sqlx::query(
        "INSERT INTO workflow_events (workspace_id, execution_id, kind, payload, actor_kind)
         VALUES ($1, $2, 'claim', '{}', 'user')",
    )
    .bind(f.ws)
    .bind(e)
    .execute(&pool)
    .await;
    assert_eq!(code(bad), CHECK);
    sqlx::query(
        "INSERT INTO workflow_resolutions (workspace_id, execution_id, action, actor_user_id,
             reason, outcome)
         VALUES ($1, $2, 'close_no_po', $3, 'r', 'resolved_manual:closed_no_po')",
    )
    .bind(f.ws)
    .bind(e)
    .bind(f.approver)
    .execute(&pool)
    .await
    .unwrap();
    for sql in [
        "UPDATE workflow_events SET kind = 'x'",
        "DELETE FROM workflow_events",
        "UPDATE workflow_resolutions SET reason = 'x'",
        "DELETE FROM workflow_resolutions",
    ] {
        assert_eq!(
            code(sqlx::query(sql).execute(&pool).await),
            RESTRICT,
            "{sql}"
        );
    }
}

#[sqlx::test]
async fn incidents_one_open_per_subject(pool: PgPool) {
    let f = fx(&pool, "g").await;
    let open = || {
        sqlx::query(
            "INSERT INTO workflow_incidents (workspace_id, kind, subject_ref, connection_id,
                 status, detail)
             VALUES ($1, 'connection_unhealthy', 's:1', $2, 'open', '{}') RETURNING id",
        )
        .bind(f.ws)
        .bind(f.conn)
        .fetch_one(&pool)
    };
    let i: Uuid = sqlx::Row::get(&open().await.unwrap(), 0);
    assert_eq!(code(open().await), UNIQUE);
    let no_by = sqlx::query("UPDATE workflow_incidents SET status = 'resolved' WHERE id = $1")
        .bind(i)
        .execute(&pool)
        .await;
    assert_eq!(code(no_by), CHECK);
    sqlx::query(
        "UPDATE workflow_incidents SET status = 'resolved', resolved_by = $2, resolved_at = now(),
             note = 'n' WHERE id = $1",
    )
    .bind(i)
    .bind(f.approver)
    .execute(&pool)
    .await
    .unwrap();
    open().await.unwrap();
}

#[sqlx::test]
async fn cross_workspace_references_refused(pool: PgPool) {
    let a = fx(&pool, "h1").await;
    let b = fx(&pool, "h2").await;
    // Request in b pointing at a's pair.
    assert_eq!(
        code(request(&pool, &a, b.ws, "s:1", 1, "pending").await),
        FK
    );
    let r = request(&pool, &a, a.ws, "s:1", 1, "approved")
        .await
        .unwrap();
    // Execution in b pointing at a's request.
    let e = sqlx::query(
        "INSERT INTO workflow_executions (workspace_id, approval_request_id, workflow_key,
             correlation_id, status)
         VALUES ($1, $2, 'wf01_purchase', 'vz-00000000000000000000000000000001', 'queued')",
    )
    .bind(b.ws)
    .bind(r)
    .execute(&pool)
    .await;
    assert_eq!(code(e), FK);
    // Event in b pointing at a's request.
    let ev = sqlx::query(
        "INSERT INTO workflow_events (workspace_id, approval_request_id, kind, payload, actor_kind)
         VALUES ($1, $2, 'decision', '{}', 'system')",
    )
    .bind(b.ws)
    .bind(r)
    .execute(&pool)
    .await;
    assert_eq!(code(ev), FK);
}
