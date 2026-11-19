//! SQL behind the workflow routes. Every query filters by workspace.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

pub struct ExecutionRow {
    pub id: Uuid,
    pub approval_request_id: Uuid,
    pub workflow_key: String,
    pub correlation_id: String,
    pub status: String,
    pub phase: String,
    pub reason: Option<String>,
    pub resolution: Option<String>,
    pub error_class: Option<String>,
    pub error_detail: Option<String>,
    pub target_ref: Option<String>,
    pub remote_attempts: i16,
    pub manual_retry_grants: i16,
    /// Outcome of the newest attempt; None when no attempt or still unknown.
    pub latest_outcome: Option<String>,
    pub has_attempt: bool,
    pub requester_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub total: i64,
}

/// Executions newest first; `id` set → that one (locked `FOR UPDATE` when `lock`).
#[allow(clippy::too_many_arguments)]
pub async fn executions(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    workflow_key: Option<&str>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ExecutionRow>, sqlx::Error> {
    sqlx::query_as!(
        ExecutionRow,
        r#"SELECT e.id, e.approval_request_id, e.workflow_key, e.correlation_id, e.status, e.phase,
                  e.reason, e.resolution, e.error_class, e.error_detail, e.target_ref,
                  e.remote_attempts, e.manual_retry_grants,
                  (SELECT a.outcome FROM workflow_execution_attempts a WHERE a.execution_id = e.id
                   ORDER BY a.attempt_no DESC LIMIT 1) AS "latest_outcome?",
                  EXISTS (SELECT 1 FROM workflow_execution_attempts a WHERE a.execution_id = e.id) AS "has_attempt!",
                  r.requester_user_id, e.created_at, e.updated_at, count(*) OVER () AS "total!"
           FROM workflow_executions e
           JOIN approval_requests r ON r.workspace_id = e.workspace_id AND r.id = e.approval_request_id
           WHERE e.workspace_id = $1
             AND ($2::uuid IS NULL OR e.id = $2)
             AND ($3::text IS NULL OR e.workflow_key = $3)
             AND ($4::text IS NULL OR e.status = $4)
           ORDER BY e.id DESC LIMIT $5 OFFSET $6"#,
        ws,
        id,
        workflow_key,
        status,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// Row lock before a human action; false when not in the workspace.
pub async fn lock_execution(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar!(
        "SELECT id FROM workflow_executions WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        ws,
        id,
    )
    .fetch_optional(db)
    .await?
    .is_some())
}

/// A human move of the execution. Clears reason/error unless given; None keeps the rest.
pub struct HumanMove<'a> {
    pub to: &'a str,
    pub reason: Option<&'a str>,
    pub resolution: Option<&'a str>,
    pub phase: Option<&'a str>,
    pub grant: bool,
}

pub async fn move_execution(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    m: &HumanMove<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE workflow_executions
         SET status = $3, reason = $4, resolution = $5, phase = coalesce($6, phase),
             manual_retry_grants = manual_retry_grants + CASE WHEN $7 THEN 1 ELSE 0 END,
             error_class = NULL, error_detail = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        m.to,
        m.reason,
        m.resolution,
        m.phase,
        m.grant,
    )
    .execute(db)
    .await?;
    Ok(())
}

pub struct NewResolution<'a> {
    pub execution_id: Option<Uuid>,
    pub incident_id: Option<Uuid>,
    pub action: &'a str,
    pub actor: Uuid,
    pub reason: &'a str,
    pub target_id: Option<&'a str>,
    pub outcome: &'a str,
}

pub async fn insert_resolution(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    r: &NewResolution<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO workflow_resolutions (workspace_id, execution_id, incident_id, action,
             actor_user_id, reason, target_id, outcome)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        ws,
        r.execution_id,
        r.incident_id,
        r.action,
        r.actor,
        r.reason,
        r.target_id,
        r.outcome,
    )
    .execute(db)
    .await?;
    Ok(())
}

pub struct AttemptRow {
    pub attempt_no: i16,
    pub granted_by: Option<Uuid>,
    pub grant_reason: Option<String>,
    pub write_intent_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub outcome: Option<String>,
}

pub async fn attempts(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Vec<AttemptRow>, sqlx::Error> {
    sqlx::query_as!(
        AttemptRow,
        "SELECT attempt_no, granted_by, grant_reason, write_intent_at, sent_at, outcome
         FROM workflow_execution_attempts WHERE workspace_id = $1 AND execution_id = $2
         ORDER BY attempt_no",
        ws,
        id,
    )
    .fetch_all(db)
    .await
}

pub struct EventRow {
    pub kind: String,
    pub actor_kind: String,
    pub created_at: DateTime<Utc>,
}

/// Newest 200 events of the execution or its request.
pub async fn events(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    request: Uuid,
) -> Result<Vec<EventRow>, sqlx::Error> {
    // shortcut: last 200 only, page when a run's timeline grows past that.
    sqlx::query_as!(
        EventRow,
        "SELECT kind, actor_kind, created_at FROM (
             SELECT id, kind, actor_kind, created_at FROM workflow_events
             WHERE workspace_id = $1 AND (execution_id = $2 OR approval_request_id = $3)
             ORDER BY id DESC LIMIT 200) t
         ORDER BY id",
        ws,
        id,
        request,
    )
    .fetch_all(db)
    .await
}

pub struct ResolutionRow {
    pub action: String,
    pub actor_user_id: Uuid,
    pub actor_name: String,
    pub reason: String,
    pub outcome: String,
    pub created_at: DateTime<Utc>,
}

pub async fn resolutions(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Vec<ResolutionRow>, sqlx::Error> {
    sqlx::query_as!(
        ResolutionRow,
        r#"SELECT w.action, w.actor_user_id, u.first_name || ' ' || u.last_name AS "actor_name!",
                  w.reason, w.outcome, w.created_at
           FROM workflow_resolutions w JOIN users u ON u.id = w.actor_user_id
           WHERE w.workspace_id = $1 AND w.execution_id = $2 ORDER BY w.id"#,
        ws,
        id,
    )
    .fetch_all(db)
    .await
}

pub struct IncidentRow {
    pub id: Uuid,
    pub kind: String,
    pub subject_ref: String,
    pub status: String,
    pub pair_id: Option<Uuid>,
    pub approval_request_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
    pub detail: Value,
    pub note: Option<String>,
    pub resolved_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// Requester of the linked request (direct or through the execution).
    pub requester_user_id: Option<Uuid>,
    pub total: i64,
}

pub async fn incidents(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    status: Option<&str>,
    kind: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<IncidentRow>, sqlx::Error> {
    sqlx::query_as!(
        IncidentRow,
        r#"SELECT i.id, i.kind, i.subject_ref, i.status, i.pair_id, i.approval_request_id,
                  i.execution_id, i.detail, i.note, i.resolved_by, i.created_at,
                  r.requester_user_id AS "requester_user_id?", count(*) OVER () AS "total!"
           FROM workflow_incidents i
           LEFT JOIN workflow_executions e ON e.id = i.execution_id
           LEFT JOIN approval_requests r ON r.id = coalesce(i.approval_request_id, e.approval_request_id)
           WHERE i.workspace_id = $1
             AND ($2::uuid IS NULL OR i.id = $2)
             AND ($3::text IS NULL OR i.status = $3)
             AND ($4::text IS NULL OR i.kind = $4)
           ORDER BY i.id DESC LIMIT $5 OFFSET $6"#,
        ws,
        id,
        status,
        kind,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// `open` → `resolved`; false when already resolved or not in the workspace.
pub async fn resolve_incident(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    by: Uuid,
    note: &str,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE workflow_incidents SET status = 'resolved', resolved_by = $3, resolved_at = now(),
             note = $4, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status = 'open'",
        ws,
        id,
        by,
        note,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Open incidents of `kind` on the execution (ids).
pub async fn open_incidents(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    execution: Uuid,
    kind: &str,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM workflow_incidents
         WHERE workspace_id = $1 AND execution_id = $2 AND kind = $3 AND status = 'open'",
        ws,
        execution,
        kind,
    )
    .fetch_all(db)
    .await
}
