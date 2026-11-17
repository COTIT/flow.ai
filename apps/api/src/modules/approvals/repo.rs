//! SQL for approval requests, decisions and policies. Every query filters by workspace.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// One request with its decision, execution summary and open incidents (one query).
pub struct RequestRow {
    pub id: Uuid,
    pub workflow_key: String,
    pub pair_id: Uuid,
    pub subject_ref: String,
    pub revision: i32,
    pub fingerprint: String,
    pub status: String,
    pub blocked_reasons: Vec<String>,
    pub invalidated_reason: Option<String>,
    pub amount_minor: i64,
    pub currency_ref: String,
    pub currency_decimals: i16,
    pub required_permission: Option<String>,
    pub requester_user_id: Option<Uuid>,
    pub requester_name: Option<String>,
    pub decision: Option<String>,
    pub decided_by: Option<Uuid>,
    pub decided_by_name: Option<String>,
    pub decision_comment: Option<String>,
    pub decision_at: Option<DateTime<Utc>>,
    pub execution_id: Option<Uuid>,
    pub execution_status: Option<String>,
    pub execution_phase: Option<String>,
    pub execution_reason: Option<String>,
    pub execution_resolution: Option<String>,
    pub execution_error_class: Option<String>,
    pub execution_error_detail: Option<String>,
    pub execution_target_ref: Option<String>,
    pub execution_updated_at: Option<DateTime<Utc>>,
    pub open_incidents: Value,
    pub snapshot: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub total: i64,
}

/// Filters for `list`; `id` set means one row (detail, with snapshot).
pub struct Filter<'a> {
    pub id: Option<Uuid>,
    pub workflow_key: Option<&'a str>,
    pub status: Option<&'a str>,
    /// Only requests raised by this user (`scope=mine`).
    pub requester: Option<Uuid>,
    /// `scope=decidable`: pending, not raised by `caller`, needing one of these permissions.
    pub decidable_perms: Option<Vec<String>>,
    pub caller: Uuid,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    f: &Filter<'_>,
) -> Result<Vec<RequestRow>, sqlx::Error> {
    sqlx::query_as!(
        RequestRow,
        r#"SELECT r.id, r.workflow_key, r.pair_id, r.subject_ref, r.revision, r.fingerprint,
                  r.status, r.blocked_reasons, r.invalidated_reason, r.amount_minor,
                  r.currency_ref, r.currency_decimals, r.required_permission, r.requester_user_id,
                  ru.first_name || ' ' || ru.last_name AS "requester_name?",
                  d.decision AS "decision?", d.actor_user_id AS "decided_by?",
                  du.first_name || ' ' || du.last_name AS "decided_by_name?",
                  d.comment AS "decision_comment?", d.created_at AS "decision_at?",
                  e.id AS "execution_id?", e.status AS "execution_status?",
                  e.phase AS "execution_phase?", e.reason AS "execution_reason?",
                  e.resolution AS "execution_resolution?",
                  e.error_class AS "execution_error_class?",
                  e.error_detail AS "execution_error_detail?",
                  e.target_ref AS "execution_target_ref?",
                  e.updated_at AS "execution_updated_at?",
                  coalesce((SELECT jsonb_agg(jsonb_build_object('id', i.id, 'kind', i.kind) ORDER BY i.id)
                            FROM workflow_incidents i
                            WHERE i.approval_request_id = r.id AND i.status = 'open'), '[]') AS "open_incidents!",
                  CASE WHEN $2::uuid IS NOT NULL THEN r.snapshot END AS "snapshot?",
                  r.created_at, r.updated_at, count(*) OVER () AS "total!"
           FROM approval_requests r
           LEFT JOIN users ru ON ru.id = r.requester_user_id
           LEFT JOIN approval_decisions d ON d.request_id = r.id
           LEFT JOIN users du ON du.id = d.actor_user_id
           LEFT JOIN workflow_executions e ON e.approval_request_id = r.id
           WHERE r.workspace_id = $1
             AND ($2::uuid IS NULL OR r.id = $2)
             AND ($3::text IS NULL OR r.workflow_key = $3)
             AND ($4::text IS NULL OR r.status = $4)
             AND ($5::uuid IS NULL OR r.requester_user_id = $5)
             AND ($6::text[] IS NULL OR (r.status = 'pending'
                  AND r.requester_user_id IS DISTINCT FROM $7
                  AND r.required_permission = ANY($6)))
           ORDER BY r.id DESC
           LIMIT $8 OFFSET $9"#,
        ws,
        f.id,
        f.workflow_key,
        f.status,
        f.requester,
        f.decidable_perms.as_deref(),
        f.caller,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await
}

/// Status, requester and needed permission of one request (decide checks).
pub struct DecideFacts {
    pub status: String,
    pub requester_user_id: Option<Uuid>,
    pub required_permission: Option<String>,
    pub workflow_key: String,
}

pub async fn decide_facts(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<DecideFacts>, sqlx::Error> {
    sqlx::query_as!(
        DecideFacts,
        "SELECT status, requester_user_id, required_permission, workflow_key
         FROM approval_requests WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Decide CAS (contracts §3.1). True when this call moved `pending` → `to`.
pub async fn decide_cas(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    to: &str,
    actor: Uuid,
    fingerprint: &str,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE approval_requests
         SET status = $3, decided_by = $4, decided_at = now(), updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status = 'pending' AND fingerprint = $5",
        ws,
        id,
        to,
        actor,
        fingerprint,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

pub struct NewDecision<'a> {
    pub request_id: Uuid,
    pub requester: Uuid,
    pub actor: Uuid,
    pub decision: &'a str,
    pub comment: Option<&'a str>,
    pub fingerprint: &'a str,
    pub permission_used: &'a str,
}

pub async fn insert_decision(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    d: &NewDecision<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO approval_decisions (workspace_id, request_id, requester_user_id, actor_user_id,
             decision, comment, fingerprint_seen, permission_used)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        ws,
        d.request_id,
        d.requester,
        d.actor,
        d.decision,
        d.comment,
        d.fingerprint,
        d.permission_used,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// New `queued` execution with a fresh correlation id. Returns its id.
pub async fn insert_execution(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    request_id: Uuid,
    workflow_key: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO workflow_executions (workspace_id, approval_request_id, workflow_key,
             correlation_id, status)
         VALUES ($1, $2, $3, 'vz-' || replace(gen_random_uuid()::text, '-', ''), 'queued')
         RETURNING id",
        ws,
        request_id,
        workflow_key,
    )
    .fetch_one(db)
    .await
}

/// Latest-revision facts for resubmit.
pub struct ResubmitFacts {
    pub status: String,
    pub requester_user_id: Option<Uuid>,
    pub is_latest: bool,
}

pub async fn resubmit_facts(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<ResubmitFacts>, sqlx::Error> {
    sqlx::query_as!(
        ResubmitFacts,
        r#"SELECT r.status, r.requester_user_id,
                  NOT EXISTS (SELECT 1 FROM approval_requests n
                              WHERE n.workspace_id = r.workspace_id AND n.workflow_key = r.workflow_key
                                AND n.subject_ref = r.subject_ref AND n.revision > r.revision) AS "is_latest!"
           FROM approval_requests r WHERE r.workspace_id = $1 AND r.id = $2"#,
        ws,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Copy of `id` as the next revision: same fingerprint, snapshot and amount, `pending`.
/// None when a live row for the subject exists (unique index) — caller maps to 409.
pub async fn insert_resubmission(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    by: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO approval_requests (workspace_id, workflow_key, pair_id, subject_ref, revision,
             fingerprint, snapshot, amount_minor, currency_ref, currency_decimals,
             required_permission, requester_user_id, status, resubmitted_by)
         SELECT workspace_id, workflow_key, pair_id, subject_ref, revision + 1, fingerprint,
             snapshot, amount_minor, currency_ref, currency_decimals, required_permission,
             requester_user_id, 'pending', $3
         FROM approval_requests WHERE workspace_id = $1 AND id = $2
         ON CONFLICT DO NOTHING
         RETURNING id",
        ws,
        id,
        by,
    )
    .fetch_optional(db)
    .await
}

pub struct PolicyRow {
    pub id: Uuid,
    pub workflow_key: String,
    pub currency_ref: String,
    pub threshold_minor: i64,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub total: i64,
}

pub async fn policies(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<PolicyRow>, sqlx::Error> {
    sqlx::query_as!(
        PolicyRow,
        r#"SELECT id, workflow_key, currency_ref, threshold_minor, active, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM approval_policies WHERE workspace_id = $1
           ORDER BY workflow_key, currency_ref, id DESC LIMIT $2 OFFSET $3"#,
        ws,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// None when an active policy for the key and currency exists.
pub async fn insert_policy(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    workflow_key: &str,
    currency_ref: &str,
    threshold_minor: i64,
    by: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO approval_policies (workspace_id, workflow_key, currency_ref, threshold_minor,
             created_by)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (workspace_id, workflow_key, currency_ref) WHERE active DO NOTHING
         RETURNING id",
        ws,
        workflow_key,
        currency_ref,
        threshold_minor,
        by,
    )
    .fetch_optional(db)
    .await
}

/// Patch; false when the policy is not in the workspace.
pub async fn update_policy(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    threshold_minor: Option<i64>,
    active: Option<bool>,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE approval_policies
         SET threshold_minor = coalesce($3, threshold_minor), active = coalesce($4, active),
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        threshold_minor,
        active,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}
