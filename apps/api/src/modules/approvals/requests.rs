//! Request writes for WF-01 intake (C4a, contracts §3.1, §4.6): new revision with policy,
//! requester and eligible-approver checks done in SQL; unblock, re-block, invalidate.
// shortcut: C4a/C4b are the callers of this contract API (§4.6); drop the allow when they land.
#![allow(dead_code)]

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::AppError;

/// What intake knows about one source request revision.
#[derive(Debug, Clone)]
pub struct NewRevision {
    pub workspace_id: Uuid,
    pub workflow_key: String,
    pub pair_id: Uuid,
    pub subject_ref: String,
    pub fingerprint: String,
    pub snapshot: Value,
    pub amount_minor: i64,
    pub currency_ref: String,
    pub currency_decimals: i16,
    pub requester_user_id: Option<Uuid>,
    /// Intake's own codes (`unmapped:*`, `amount_not_exact`, …). This fn adds `no_policy:*`,
    /// `no_requester` and `no_eligible_approver`.
    pub blocked_reasons: Vec<String>,
}

/// The stored row after `upsert_revision`.
#[derive(Debug, Clone)]
pub struct RequestRow {
    pub id: Uuid,
    pub revision: i32,
    pub status: String,
    pub required_permission: Option<String>,
    pub blocked_reasons: Vec<String>,
}

/// Required permission from the active policy (D4, in SQL): below threshold → approve, else
/// approve_high; None without a policy.
async fn required_permission(
    tx: &mut PgConnection,
    ws: Uuid,
    workflow_key: &str,
    currency_ref: &str,
    amount_minor: i64,
) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT CASE WHEN $4 < threshold_minor THEN 'integration.approve'
                       ELSE 'integration.approve_high' END AS "p!"
           FROM approval_policies
           WHERE workspace_id = $1 AND workflow_key = $2 AND currency_ref = $3 AND active"#,
        ws,
        workflow_key,
        currency_ref,
        amount_minor,
    )
    .fetch_optional(&mut *tx)
    .await?)
}

/// True when an active member other than `requester` may decide with `perm`
/// (owner, `workspace_admin`, the permission, or approve_high for approve).
pub async fn eligible_approver_exists(
    tx: &mut PgConnection,
    ws: Uuid,
    perm: &str,
    requester: Option<Uuid>,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
             WHERE m.workspace_id = $1 AND m.active AND w.deleted_at IS NULL
               AND m.user_id IS DISTINCT FROM $3
               AND (w.created_by_user_id = m.user_id OR EXISTS (
                    SELECT 1 FROM team_members tm
                    JOIN teams t ON t.id = tm.team_id AND t.workspace_id = $1 AND t.deleted_at IS NULL
                    JOIN team_permissions p ON p.team_id = t.id
                    WHERE tm.user_id = m.user_id
                      AND (p.permission IN ('workspace_admin', $2)
                           OR ($2 = 'integration.approve' AND p.permission = 'integration.approve_high'))))
           ) AS "x!""#,
        ws,
        perm,
        requester,
    )
    .fetch_one(&mut *tx)
    .await?)
}

/// Blocking codes from policy, requester and approvers, after intake's own.
async fn full_reasons(
    tx: &mut PgConnection,
    ws: Uuid,
    rev: &NewRevision,
) -> Result<(Option<String>, Vec<String>), AppError> {
    let perm = required_permission(
        tx,
        ws,
        &rev.workflow_key,
        &rev.currency_ref,
        rev.amount_minor,
    )
    .await?;
    let mut reasons = rev.blocked_reasons.clone();
    match &perm {
        None => reasons.push(format!("no_policy:{}", rev.currency_ref)),
        Some(p) if !eligible_approver_exists(tx, ws, p, rev.requester_user_id).await? => {
            reasons.push("no_eligible_approver".into())
        }
        Some(_) => {}
    }
    if rev.requester_user_id.is_none() {
        reasons.push("no_requester".into());
    }
    Ok((perm, reasons))
}

/// Create the next revision for a subject (`max + 1`), `blocked` with reasons or `pending`.
/// A live row with the same fingerprint is returned as is (repeat poll). A live row with
/// another fingerprint must be invalidated first (unique `approval_requests_one_live`).
pub async fn upsert_revision(
    tx: &mut PgConnection,
    rev: NewRevision,
) -> Result<RequestRow, AppError> {
    let ws = rev.workspace_id;
    // Serialise intake per subject so `max + 1` holds.
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended($1 || ':' || $2 || ':' || $3, 0))",
        ws.to_string(),
        rev.workflow_key,
        rev.subject_ref,
    )
    .execute(&mut *tx)
    .await?;
    if let Some(live) = sqlx::query_as!(
        RequestRow,
        "SELECT id, revision, status, required_permission, blocked_reasons FROM approval_requests
         WHERE workspace_id = $1 AND workflow_key = $2 AND subject_ref = $3 AND fingerprint = $4
           AND status IN ('blocked', 'pending', 'approved')",
        ws,
        rev.workflow_key,
        rev.subject_ref,
        rev.fingerprint,
    )
    .fetch_optional(&mut *tx)
    .await?
    {
        return Ok(live);
    }
    let (perm, reasons) = full_reasons(tx, ws, &rev).await?;
    let status = if reasons.is_empty() {
        "pending"
    } else {
        "blocked"
    };
    let row = sqlx::query_as!(
        RequestRow,
        "INSERT INTO approval_requests (workspace_id, workflow_key, pair_id, subject_ref, revision,
             fingerprint, snapshot, amount_minor, currency_ref, currency_decimals,
             required_permission, requester_user_id, status, blocked_reasons)
         SELECT $1, $2, $3, $4,
             coalesce((SELECT max(revision) FROM approval_requests
                       WHERE workspace_id = $1 AND workflow_key = $2 AND subject_ref = $4), 0) + 1,
             $5, $6, $7, $8, $9, $10, $11, $12, $13
         RETURNING id, revision, status, required_permission, blocked_reasons",
        ws,
        rev.workflow_key,
        rev.pair_id,
        rev.subject_ref,
        rev.fingerprint,
        rev.snapshot,
        rev.amount_minor,
        rev.currency_ref,
        rev.currency_decimals,
        // A blocked row may lack a permission; keep it when a policy exists.
        perm,
        rev.requester_user_id,
        status,
        &reasons,
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok(row)
}

/// `blocked`/`pending`/`approved` without execution → `invalidated`. False when the row is not
/// live, or an execution exists (then only the executor may cancel it, §3.1).
pub async fn invalidate(
    tx: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    reason: &str,
) -> Result<bool, AppError> {
    let n = sqlx::query!(
        "UPDATE approval_requests r SET status = 'invalidated', invalidated_reason = $3,
             decided_by = NULL, decided_at = NULL, updated_at = now()
         WHERE r.workspace_id = $1 AND r.id = $2 AND r.status IN ('blocked', 'pending', 'approved')
           AND NOT EXISTS (SELECT 1 FROM workflow_executions e WHERE e.approval_request_id = r.id)",
        ws,
        id,
        reason,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// `blocked` → `pending` once intake finds no reasons; still checks policy and approvers.
/// Returns the status after the call.
pub async fn unblock(
    tx: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<String>, AppError> {
    let Some(row) = sqlx::query!(
        "SELECT workflow_key, pair_id, subject_ref, fingerprint, amount_minor, currency_ref,
             currency_decimals, requester_user_id
         FROM approval_requests WHERE workspace_id = $1 AND id = $2 AND status = 'blocked' FOR UPDATE",
        ws,
        id,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };
    let rev = NewRevision {
        workspace_id: ws,
        workflow_key: row.workflow_key,
        pair_id: row.pair_id,
        subject_ref: row.subject_ref,
        fingerprint: row.fingerprint,
        snapshot: Value::Null,
        amount_minor: row.amount_minor,
        currency_ref: row.currency_ref,
        currency_decimals: row.currency_decimals,
        requester_user_id: row.requester_user_id,
        blocked_reasons: vec![],
    };
    let (perm, reasons) = full_reasons(tx, ws, &rev).await?;
    set_reasons(tx, ws, id, perm, &reasons).await?;
    Ok(Some(
        if reasons.is_empty() {
            "pending"
        } else {
            "blocked"
        }
        .into(),
    ))
}

/// Replace a blocked row's reasons (intake's codes plus policy/requester/approver checks).
pub async fn set_blocked(
    tx: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    reasons: Vec<String>,
) -> Result<bool, AppError> {
    let Some(row) = sqlx::query!(
        "SELECT workflow_key, pair_id, subject_ref, fingerprint, amount_minor, currency_ref,
             currency_decimals, requester_user_id
         FROM approval_requests WHERE workspace_id = $1 AND id = $2 AND status IN ('blocked', 'pending')
         FOR UPDATE",
        ws,
        id,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    let rev = NewRevision {
        workspace_id: ws,
        workflow_key: row.workflow_key,
        pair_id: row.pair_id,
        subject_ref: row.subject_ref,
        fingerprint: row.fingerprint,
        snapshot: Value::Null,
        amount_minor: row.amount_minor,
        currency_ref: row.currency_ref,
        currency_decimals: row.currency_decimals,
        requester_user_id: row.requester_user_id,
        blocked_reasons: reasons,
    };
    let (perm, reasons) = full_reasons(tx, ws, &rev).await?;
    set_reasons(tx, ws, id, perm, &reasons).await?;
    Ok(true)
}

async fn set_reasons(
    tx: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    perm: Option<String>,
    reasons: &[String],
) -> Result<(), AppError> {
    let status = if reasons.is_empty() {
        "pending"
    } else {
        "blocked"
    };
    sqlx::query!(
        "UPDATE approval_requests SET status = $3, blocked_reasons = $4, required_permission = $5,
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        status,
        reasons,
        perm,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sqlx::PgPool;

    use super::*;
    use crate::workflows::testkit::{self, FP};

    fn rev(k: &testkit::Kit, amount: i64) -> NewRevision {
        NewRevision {
            workspace_id: k.ws,
            workflow_key: "wf01_purchase".into(),
            pair_id: k.pair,
            subject_ref: "odoo:c:purchase.order:17".into(),
            fingerprint: FP.into(),
            snapshot: json!({"v": 1}),
            amount_minor: amount,
            currency_ref: "EUR".into(),
            currency_decimals: 2,
            requester_user_id: Some(k.requester),
            blocked_reasons: vec![],
        }
    }

    async fn policy(pool: &PgPool, k: &testkit::Kit, threshold: i64) {
        sqlx::query(
            "INSERT INTO approval_policies (workspace_id, workflow_key, currency_ref, threshold_minor, created_by)
             VALUES ($1, 'wf01_purchase', 'EUR', $2, $3)",
        )
        .bind(k.ws)
        .bind(threshold)
        .bind(k.requester)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Approver joins the workspace in a team holding `perm`.
    async fn approver(pool: &PgPool, k: &testkit::Kit, perm: &str) {
        sqlx::query(
            "WITH m AS (INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)),
                  t AS (INSERT INTO teams (workspace_id, name, description) VALUES ($1, 'T', 'd') RETURNING id),
                  p AS (INSERT INTO team_permissions (team_id, permission) SELECT id, $3 FROM t)
             INSERT INTO team_members (team_id, user_id) SELECT id, $2 FROM t",
        )
        .bind(k.ws)
        .bind(k.approver)
        .bind(perm)
        .execute(pool)
        .await
        .unwrap();
    }

    // PLAN test 7: the requester owns the workspace, nobody else can approve.
    #[sqlx::test]
    async fn no_eligible_approver_blocks_until_one_joins(pool: PgPool) {
        let k = testkit::kit(&pool, "w").await;
        sqlx::query("INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)")
            .bind(k.ws)
            .bind(k.requester)
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let r = upsert_revision(&mut tx, rev(&k, 100)).await.unwrap();
        assert_eq!(r.status, "blocked");
        assert_eq!(r.blocked_reasons, vec!["no_policy:EUR"]);
        tx.commit().await.unwrap();
        policy(&pool, &k, 1000).await;
        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            unblock(&mut tx, k.ws, r.id).await.unwrap().as_deref(),
            Some("blocked")
        );
        let reasons: Vec<String> =
            sqlx::query_scalar("SELECT blocked_reasons FROM approval_requests WHERE id = $1")
                .bind(r.id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(
            reasons,
            vec!["no_eligible_approver"],
            "owner is the requester"
        );
        tx.commit().await.unwrap();
        approver(&pool, &k, "integration.approve_high").await;
        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            unblock(&mut tx, k.ws, r.id).await.unwrap().as_deref(),
            Some("pending")
        );
        tx.commit().await.unwrap();
    }

    #[sqlx::test]
    async fn threshold_picks_permission_and_repeat_poll_is_one_row(pool: PgPool) {
        let k = testkit::kit(&pool, "w").await;
        policy(&pool, &k, 1000).await;
        approver(&pool, &k, "integration.approve").await;
        let mut tx = pool.begin().await.unwrap();
        let low = upsert_revision(&mut tx, rev(&k, 999)).await.unwrap();
        assert_eq!(
            (low.status.as_str(), low.required_permission.as_deref()),
            ("pending", Some("integration.approve"))
        );
        let again = upsert_revision(&mut tx, rev(&k, 999)).await.unwrap();
        assert_eq!(again.id, low.id, "same fingerprint: same row");
        assert!(
            invalidate(&mut tx, k.ws, low.id, "source_changed")
                .await
                .unwrap()
        );
        let mut high = rev(&k, 1000);
        high.fingerprint = "b".repeat(64);
        let high = upsert_revision(&mut tx, high).await.unwrap();
        assert_eq!(high.revision, 2);
        // approve only: nobody holds approve_high besides the owner, who is the requester.
        assert_eq!(
            high.required_permission.as_deref(),
            Some("integration.approve_high")
        );
        assert_eq!(high.blocked_reasons, vec!["no_eligible_approver"]);
        let mut none = rev(&k, 5);
        none.subject_ref = "odoo:c:purchase.order:18".into();
        none.requester_user_id = None;
        none.blocked_reasons = vec!["unmapped:vendor:43".into()];
        let none = upsert_revision(&mut tx, none).await.unwrap();
        assert_eq!(
            none.blocked_reasons,
            vec!["unmapped:vendor:43", "no_requester"]
        );
        assert!(set_blocked(&mut tx, k.ws, none.id, vec![]).await.unwrap());
    }
}
