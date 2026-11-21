//! WF-03 recovery schema (contracts §11.3): widened incidents, close rules, expectations.

use sqlx::PgPool;
use uuid::Uuid;

use super::approvals_schema::fx;

const CHECK: &str = "23514";

fn code(r: Result<impl Sized, sqlx::Error>) -> String {
    match r {
        Ok(_) => "ok".into(),
        Err(e) => e
            .as_database_error()
            .and_then(|d| d.code())
            .map(|c| c.into_owned())
            .unwrap_or_default(),
    }
}

async fn incident(
    pool: &PgPool,
    ws: Uuid,
    kind: &str,
    category: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO workflow_incidents (workspace_id, kind, subject_ref, status, detail, category)
         VALUES ($1, $2, 's:1', 'open', '{}', $3) RETURNING id",
    )
    .bind(ws)
    .bind(kind)
    .bind(category)
    .fetch_one(pool)
    .await
}

#[sqlx::test]
async fn wf03_reconciled_close_without_verify_rejected(pool: PgPool) {
    let f = fx(&pool, "wf03a").await;
    let i = incident(&pool, f.ws, "exchange_late", Some("missing"))
        .await
        .unwrap();
    let close = |verified: bool| {
        sqlx::query(
            "UPDATE workflow_incidents SET status = 'resolved', resolved_at = now(),
                 resolution_kind = 'reconciled',
                 verified_at = CASE WHEN $2 THEN now() END WHERE id = $1",
        )
        .bind(i)
        .bind(verified)
        .execute(&pool)
    };
    assert_eq!(code(close(false).await), CHECK);
    // system close: no user, no note, verified read-back
    assert_eq!(code(close(true).await), "ok");
}

#[sqlx::test]
async fn wf03_manual_close_needs_user_and_note(pool: PgPool) {
    let f = fx(&pool, "wf03b").await;
    let i = incident(&pool, f.ws, "exchange_failed", Some("data"))
        .await
        .unwrap();
    let no_user = sqlx::query(
        "UPDATE workflow_incidents SET status = 'resolved', resolved_at = now(),
             resolution_kind = 'manual_closed', note = 'n' WHERE id = $1",
    )
    .bind(i)
    .execute(&pool)
    .await;
    assert_eq!(code(no_user), CHECK);
    let no_kind = sqlx::query(
        "UPDATE workflow_incidents SET status = 'resolved', resolved_at = now() WHERE id = $1",
    )
    .bind(i)
    .execute(&pool)
    .await;
    assert_eq!(code(no_kind), CHECK);
    // WF-01 style resolve (user + note, no kind) defaults to manual_closed (R-17)
    sqlx::query(
        "UPDATE workflow_incidents SET status = 'resolved', resolved_by = $2, resolved_at = now(),
             note = 'n' WHERE id = $1",
    )
    .bind(i)
    .bind(f.approver)
    .execute(&pool)
    .await
    .unwrap();
    let kind: Option<String> =
        sqlx::query_scalar("SELECT resolution_kind FROM workflow_incidents WHERE id = $1")
            .bind(i)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind.as_deref(), Some("manual_closed"));
}

#[sqlx::test]
async fn wf03_new_kinds_need_category(pool: PgPool) {
    let f = fx(&pool, "wf03c").await;
    assert_eq!(
        code(incident(&pool, f.ws, "exchange_late", None).await),
        CHECK
    );
    assert_eq!(
        code(incident(&pool, f.ws, "nope", Some("data")).await),
        CHECK
    );
    assert_eq!(
        code(incident(&pool, f.ws, "auth_failure", Some("bad")).await),
        CHECK
    );
    // WF-01 kinds keep a NULL category
    assert_eq!(
        code(incident(&pool, f.ws, "target_conflict", None).await),
        "ok"
    );
    for (k, c) in [
        ("exchange_failed", "data"),
        ("auth_failure", "authorization"),
        ("permission_denied", "authorization"),
        ("rate_limited", "transient"),
        ("mapping_invalid", "data"),
    ] {
        assert_eq!(code(incident(&pool, f.ws, k, Some(c)).await), "ok", "{k}");
    }
}

async fn expectation(pool: &PgPool, ws: Uuid, eid: &str, checks: i16) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO exchange_expectations (workspace_id, workflow_key, correlation_id, source_ref,
             target_kind, target_external_id, due_at, checks)
         VALUES ($1, 'wf01_purchase', $2, 'odoo:x:purchase.order:1', 'purchaseOrder', $2,
             now() + interval '15 minutes', $3) RETURNING id",
    )
    .bind(ws)
    .bind(eid)
    .bind(checks)
    .fetch_one(pool)
    .await
}

#[sqlx::test]
async fn wf03_expectation_rules(pool: PgPool) {
    let f = fx(&pool, "wf03d").await;
    let eid = format!("vz-{}", "a".repeat(32));
    expectation(&pool, f.ws, &eid, 0).await.unwrap();
    assert_eq!(code(expectation(&pool, f.ws, &eid, 0).await), "23505");
    assert_eq!(code(expectation(&pool, f.ws, "vz-bad", 0).await), CHECK);
    let eid2 = format!("vz-{}", "b".repeat(32));
    assert_eq!(code(expectation(&pool, f.ws, &eid2, 11).await), CHECK);
    // another workspace's incident cannot be linked
    let other = fx(&pool, "wf03e").await;
    let foreign = incident(&pool, other.ws, "exchange_late", Some("missing"))
        .await
        .unwrap();
    let r =
        sqlx::query("UPDATE exchange_expectations SET incident_id = $2 WHERE workspace_id = $1")
            .bind(f.ws)
            .bind(foreign)
            .execute(&pool)
            .await;
    assert_eq!(code(r), "23503");
}
