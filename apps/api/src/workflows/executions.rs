//! Execution state writes (contracts §3.2-§3.4, §4.6). Ownership: `claim` / `takeover` set
//! `owner_token`; every later write on an owned row (`queued`/`running`) is fenced on it.
//! Unowned states (`blocked`, `awaiting_reconciliation`, …) are moved by reconcile or humans.
// shortcut: C4a/C4b are the callers of this contract API (§4.6); drop the allow when they land.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::{
    error::AppError,
    workflows::events::{self, Actor, EventRef},
};

/// `workflow_executions.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Running,
    Blocked,
    AwaitingReconciliation,
    Cancelled,
    Succeeded,
    SucceededWithVariance,
    ResolvedManual,
    Failed,
}

impl Status {
    /// DB text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Blocked => "blocked",
            Self::AwaitingReconciliation => "awaiting_reconciliation",
            Self::Cancelled => "cancelled",
            Self::Succeeded => "succeeded",
            Self::SucceededWithVariance => "succeeded_with_variance",
            Self::ResolvedManual => "resolved_manual",
            Self::Failed => "failed",
        }
    }
}

/// Columns a transition sets with the status. `None` leaves target_ref / phase as they are;
/// reason and error fields are always overwritten (cleared when `None`).
#[derive(Debug, Clone, Default)]
pub struct Patch {
    pub reason: Option<String>,
    pub error_class: Option<String>,
    pub error_detail: Option<String>,
    pub target_ref: Option<String>,
    pub phase: Option<String>,
}

/// `queued` → `running` for one winner. False when already claimed or gone.
pub async fn claim(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    token: Uuid,
) -> Result<bool, AppError> {
    let n = sqlx::query!(
        "UPDATE workflow_executions SET status = 'running', owner_token = $3, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status = 'queued'",
        ws,
        id,
        token,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Take ownership of a `queued`/`running` row whose owner job no longer holds a live lease
/// (P-2, contracts §3.2). False while the old lease is live.
pub async fn takeover(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    token: Uuid,
) -> Result<bool, AppError> {
    let n = sqlx::query!(
        "UPDATE workflow_executions SET owner_token = $3, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status IN ('queued', 'running')
           AND NOT EXISTS (SELECT 1 FROM jobs j WHERE j.workspace_id = $1
                           AND j.locked_by = workflow_executions.owner_token
                           AND j.status = 'running' AND j.locked_until > now())",
        ws,
        id,
        token,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Move status `from` → `to` with `patch`. Fenced on `token` while the row is owned
/// (`queued`/`running`). False when the row moved, or another owner holds it.
/// `succeeded*` go through `finish_succeeded` (I3).
pub async fn transition(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    token: Uuid,
    from: &[Status],
    to: Status,
    patch: Patch,
) -> Result<bool, AppError> {
    if matches!(to, Status::Succeeded | Status::SucceededWithVariance) {
        return Err(AppError::Internal(
            "succeeded needs finish_succeeded (verify_match event)".into(),
        ));
    }
    let from: Vec<&str> = from.iter().map(|s| s.as_str()).collect();
    let n = sqlx::query!(
        "UPDATE workflow_executions
         SET status = $5, reason = $6, error_class = $7, error_detail = $8,
             target_ref = coalesce($9, target_ref), phase = coalesce($10, phase), updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status = ANY($4)
           AND (status NOT IN ('queued', 'running') OR owner_token = $3)",
        ws,
        id,
        token,
        &from as &[&str],
        to.as_str(),
        patch.reason,
        patch.error_class,
        patch.error_detail,
        patch.target_ref,
        patch.phase,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Write intent (P-1): in the caller's transaction, lock the owned `running` row in phase
/// `prechecking`, check the attempt cap and that every earlier attempt has an outcome, insert
/// the attempt row and set `remote_attempts` + phase `write_intent`. `Some(attempt_no)` means
/// commit, then POST once; `None` means never POST. Attempts after the first need `granted`
/// (see `grant_for`).
pub async fn begin_attempt(
    tx: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    token: Uuid,
    granted: Option<(Uuid, String)>,
) -> Result<Option<i16>, AppError> {
    let Some(row) = sqlx::query!(
        "SELECT remote_attempts, manual_retry_grants, phase FROM workflow_executions
         WHERE workspace_id = $1 AND id = $2 AND owner_token = $3 AND status = 'running'
         FOR UPDATE",
        ws,
        id,
        token,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };
    let attempt_no = row.remote_attempts + 1;
    if row.phase != "prechecking" || attempt_no > 1 + row.manual_retry_grants {
        return Ok(None);
    }
    let unresolved = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM workflow_execution_attempts
                          WHERE workspace_id = $1 AND execution_id = $2
                            AND outcome IS NULL) AS "x!""#,
        ws,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    let (granted_by, grant_reason) = match (attempt_no, granted) {
        (1, _) => (None, None),
        (_, Some((by, why))) => (Some(by), Some(why)),
        (_, None) => return Ok(None),
    };
    if unresolved {
        return Ok(None);
    }
    sqlx::query!(
        "INSERT INTO workflow_execution_attempts (workspace_id, execution_id, attempt_no,
             granted_by, grant_reason)
         VALUES ($1, $2, $3, $4, $5)",
        ws,
        id,
        attempt_no,
        granted_by,
        grant_reason,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE workflow_executions SET remote_attempts = $3, phase = 'write_intent',
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        attempt_no,
    )
    .execute(&mut *tx)
    .await?;
    Ok(Some(attempt_no))
}

/// Grant n-1 for attempt n (actor, reason): the (n-1)-th `allow_retry`, never a newer one.
pub async fn grant_for(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    attempt_no: i16,
) -> Result<Option<(Uuid, String)>, AppError> {
    if attempt_no < 2 {
        return Ok(None);
    }
    let row = sqlx::query!(
        "SELECT actor_user_id, reason FROM workflow_resolutions
         WHERE workspace_id = $1 AND execution_id = $2 AND action = 'allow_retry'
         ORDER BY created_at, id OFFSET $3 LIMIT 1",
        ws,
        id,
        i64::from(attempt_no) - 2,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.actor_user_id, r.reason)))
}

/// Set an attempt's `sent_at` / `outcome` / `evidence`, each only if still NULL. Not fenced:
/// a stale worker may report. True when the attempt exists.
pub async fn record_attempt_result(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    attempt_no: i16,
    sent_at: Option<DateTime<Utc>>,
    outcome: Option<&str>,
    evidence: Option<Value>,
) -> Result<bool, AppError> {
    let n = sqlx::query!(
        "UPDATE workflow_execution_attempts
         SET sent_at = coalesce(sent_at, $4), outcome = coalesce(outcome, $5),
             evidence = coalesce(evidence, $6)
         WHERE workspace_id = $1 AND execution_id = $2 AND attempt_no = $3",
        ws,
        id,
        attempt_no,
        sent_at,
        outcome,
        evidence,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// `running` (fenced) or `awaiting_reconciliation` → `succeeded` / `succeeded_with_variance`,
/// phase `done`, plus the `verify_match` event in the same transaction (I3). False when the
/// row moved or is owned by another token; then nothing is written.
pub async fn finish_succeeded(
    tx: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    token: Uuid,
    target_ref: &str,
    variance: bool,
) -> Result<bool, AppError> {
    let to = if variance {
        Status::SucceededWithVariance
    } else {
        Status::Succeeded
    };
    let n = sqlx::query!(
        "UPDATE workflow_executions
         SET status = $4, phase = 'done', target_ref = $5, reason = NULL, error_class = NULL,
             error_detail = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2
           AND ((status = 'running' AND owner_token = $3) OR status = 'awaiting_reconciliation')",
        ws,
        id,
        token,
        to.as_str(),
        target_ref,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(false);
    }
    let at = EventRef {
        execution_id: Some(id),
        ..Default::default()
    };
    let payload = json!({ "target_ref": target_ref, "variance": variance });
    events::record(&mut *tx, ws, at, "verify_match", payload, Actor::System).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sqlx::PgPool;
    use tokio::sync::Barrier;

    use super::*;
    use crate::workflows::testkit::{self, Kit};

    async fn running(pool: &PgPool) -> (Kit, Uuid, Uuid) {
        let k = testkit::kit(pool, "w").await;
        let e = testkit::execution(pool, &k).await;
        let tok = Uuid::now_v7();
        assert!(claim(pool, k.ws, e, tok).await.unwrap());
        (k, e, tok)
    }

    async fn lease(pool: &PgPool, ws: Uuid, token: Uuid, live: bool) {
        sqlx::query(
            "INSERT INTO jobs (workspace_id, kind, status, locked_by, locked_until)
             VALUES ($1, 'wf01_execute', 'running', $2,
                     now() + CASE WHEN $3 THEN interval '5 minutes' ELSE interval '-1 second' END)",
        )
        .bind(ws)
        .bind(token)
        .bind(live)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn attempts(pool: &PgPool, e: Uuid) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM workflow_execution_attempts WHERE execution_id = $1",
        )
        .bind(e)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn eight_concurrent_claims_one_winner(pool: PgPool) {
        let k = testkit::kit(&pool, "w").await;
        let e = testkit::execution(&pool, &k).await;
        let gate = Arc::new(Barrier::new(8));
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let (pool, gate) = (pool.clone(), gate.clone());
                tokio::spawn(async move {
                    let tok = Uuid::now_v7();
                    gate.wait().await;
                    claim(&pool, k.ws, e, tok).await.unwrap().then_some(tok)
                })
            })
            .collect();
        let mut winners = vec![];
        for t in tasks {
            winners.extend(t.await.unwrap());
        }
        assert_eq!(winners.len(), 1);
        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT owner_token FROM workflow_executions WHERE id = $1")
                .bind(e)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(owner, Some(winners[0]));
    }

    #[sqlx::test]
    async fn write_intent_race_gives_one_attempt(pool: PgPool) {
        let (k, e, tok) = running(&pool).await;
        let gate = Arc::new(Barrier::new(2));
        let tasks: Vec<_> = (0..2)
            .map(|_| {
                let (pool, gate) = (pool.clone(), gate.clone());
                tokio::spawn(async move {
                    let mut tx = pool.begin().await.unwrap();
                    gate.wait().await;
                    let got = begin_attempt(&mut tx, k.ws, e, tok, None).await.unwrap();
                    tx.commit().await.unwrap();
                    got
                })
            })
            .collect();
        let mut got = vec![];
        for t in tasks {
            got.push(t.await.unwrap());
        }
        got.sort();
        assert_eq!(got, vec![None, Some(1)]);
        assert_eq!(attempts(&pool, e).await, 1);
    }

    #[sqlx::test]
    async fn begin_attempt_guards(pool: PgPool) {
        let (k, e, tok) = running(&pool).await;
        // Wrong token: no POST.
        let mut c = pool.acquire().await.unwrap();
        assert_eq!(
            begin_attempt(&mut c, k.ws, e, Uuid::now_v7(), None)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            begin_attempt(&mut c, k.ws, e, tok, None).await.unwrap(),
            Some(1)
        );
        // Phase is write_intent now (W1): never a second POST.
        assert_eq!(
            begin_attempt(&mut c, k.ws, e, tok, None).await.unwrap(),
            None
        );
        // Even with a grant and phase reset, the cap and the open outcome hold.
        sqlx::query("UPDATE workflow_executions SET phase = 'prechecking' WHERE id = $1")
            .bind(e)
            .execute(&pool)
            .await
            .unwrap();
        let grant = Some((k.approver, "why".to_string()));
        assert_eq!(
            begin_attempt(&mut c, k.ws, e, tok, grant.clone())
                .await
                .unwrap(),
            None
        );
        sqlx::query("UPDATE workflow_executions SET manual_retry_grants = 1 WHERE id = $1")
            .bind(e)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            begin_attempt(&mut c, k.ws, e, tok, grant.clone())
                .await
                .unwrap(),
            None,
            "attempt 1 has no outcome yet"
        );
        assert!(
            record_attempt_result(&pool, k.ws, e, 1, None, Some("ambiguous"), None)
                .await
                .unwrap()
        );
        assert_eq!(
            begin_attempt(&mut c, k.ws, e, tok, None).await.unwrap(),
            None,
            "needs grant"
        );
        assert_eq!(
            begin_attempt(&mut c, k.ws, e, tok, grant).await.unwrap(),
            Some(2)
        );
        assert_eq!(attempts(&pool, e).await, 2);
    }

    #[sqlx::test]
    async fn takeover_waits_for_lease_then_fences_old_owner(pool: PgPool) {
        let (k, e, old) = running(&pool).await;
        lease(&pool, k.ws, old, true).await;
        let new = Uuid::now_v7();
        assert!(!takeover(&pool, k.ws, e, new).await.unwrap(), "lease live");
        sqlx::query(
            "UPDATE jobs SET locked_until = now() - interval '1 second' WHERE locked_by = $1",
        )
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            takeover(&pool, k.ws, e, new).await.unwrap(),
            "lease expired"
        );
        // Stale owner's fenced write is refused; the new owner's goes through.
        let to_blocked = |tok| {
            transition(
                &pool,
                k.ws,
                e,
                tok,
                &[Status::Running],
                Status::Blocked,
                Patch {
                    reason: Some("pair_paused".into()),
                    ..Default::default()
                },
            )
        };
        assert!(!to_blocked(old).await.unwrap());
        let mut c = pool.acquire().await.unwrap();
        assert_eq!(
            begin_attempt(&mut c, k.ws, e, old, None).await.unwrap(),
            None
        );
        // Stale worker may still leave evidence.
        let at = EventRef {
            execution_id: Some(e),
            attempt_no: Some(1),
            ..Default::default()
        };
        events::record(
            &pool,
            k.ws,
            at,
            "stale_worker_result",
            json!({"http_status": 201}),
            Actor::System,
        )
        .await
        .unwrap();
        assert!(to_blocked(new).await.unwrap());
        // Unknown workspace: nothing moves.
        assert!(!takeover(&pool, Uuid::now_v7(), e, new).await.unwrap());
    }

    #[sqlx::test]
    async fn takeover_ignores_other_workspace_leases(pool: PgPool) {
        let (k, e, old) = running(&pool).await;
        let other = testkit::kit(&pool, "o").await;
        // A live job in another workspace with the same token does not count.
        lease(&pool, other.ws, old, true).await;
        assert!(takeover(&pool, k.ws, e, Uuid::now_v7()).await.unwrap());
    }

    #[sqlx::test]
    async fn finish_succeeded_writes_verify_match(pool: PgPool) {
        let (k, e, tok) = running(&pool).await;
        let refuse = transition(
            &pool,
            k.ws,
            e,
            tok,
            &[Status::Running],
            Status::Succeeded,
            Patch::default(),
        )
        .await;
        assert!(refuse.is_err());
        let mut tx = pool.begin().await.unwrap();
        assert!(
            !finish_succeeded(
                &mut tx,
                k.ws,
                e,
                Uuid::now_v7(),
                "netsuite:x:purchaseOrder:1",
                false
            )
            .await
            .unwrap()
        );
        assert!(
            finish_succeeded(&mut tx, k.ws, e, tok, "netsuite:x:purchaseOrder:1", false)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        let (status, kinds): (String, Vec<String>) = sqlx::query_as(
            "SELECT e.status, array(SELECT kind FROM workflow_events WHERE execution_id = e.id)
             FROM workflow_executions e WHERE id = $1",
        )
        .bind(e)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "succeeded");
        assert_eq!(kinds, vec!["verify_match"]);
    }

    #[sqlx::test]
    async fn cancel_after_remote_attempt_is_refused_by_i2(pool: PgPool) {
        let (k, e, tok) = running(&pool).await;
        let mut c = pool.acquire().await.unwrap();
        begin_attempt(&mut c, k.ws, e, tok, None).await.unwrap();
        let r = transition(
            &pool,
            k.ws,
            e,
            tok,
            &[Status::Running],
            Status::Cancelled,
            Patch {
                reason: Some("source_changed".into()),
                ..Default::default()
            },
        )
        .await;
        assert!(r.is_err(), "CHECK I2 must refuse");
    }
}
