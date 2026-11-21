//! `exchange_expectations` state machine (contracts §11.3): register at write intent, then
//! `met` / `late` / `failed` / `closed`, each with its incident and event.

use serde_json::{Value, json};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use super::category::{Classified, sentence};
use crate::{
    error::AppError,
    workflows::{
        events::{self, Actor, EventRef},
        incidents::{self, IncidentRefs},
    },
};

/// A Flow-run write about to happen.
pub struct New<'a> {
    pub workflow_key: &'a str,
    pub correlation_id: &'a str,
    pub execution_id: Option<Uuid>,
    pub source_ref: &'a str,
    pub target_kind: &'a str,
    pub target_external_id: &'a str,
}

/// One expectation row.
#[derive(Debug, Clone)]
pub struct Row {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub execution_id: Option<Uuid>,
    pub source_ref: String,
    pub target_kind: String,
    pub target_external_id: String,
    pub due_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub checks: i16,
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Write intent (same tx as `begin_attempt`): due in 15 minutes (D-32). A granted retry of
/// the same correlation id is watched again and keeps its incident, so a later match closes
/// it `reconciled`; a `met` row stays met.
pub async fn register(tx: &mut PgConnection, ws: Uuid, n: &New<'_>) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO exchange_expectations (workspace_id, workflow_key, correlation_id,
             execution_id, source_ref, target_kind, target_external_id, due_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now() + interval '15 minutes')
         ON CONFLICT (workspace_id, correlation_id) DO UPDATE
         SET due_at = EXCLUDED.due_at, checks = 0, last_checked_at = NULL,
             status = 'pending', updated_at = now()
         WHERE exchange_expectations.status <> 'met'",
        ws,
        n.workflow_key,
        n.correlation_id,
        n.execution_id,
        n.source_ref,
        n.target_kind,
        n.target_external_id,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The expectation of an execution, or of an incident.
pub async fn find(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    execution: Option<Uuid>,
    incident: Option<Uuid>,
) -> Result<Option<Row>, AppError> {
    Ok(sqlx::query_as!(
        Row,
        "SELECT id, workspace_id, execution_id, source_ref, target_kind, target_external_id,
                due_at, status, checks, last_checked_at
         FROM exchange_expectations
         WHERE workspace_id = $1
           AND (execution_id = $2 OR incident_id = $3)
         ORDER BY id DESC LIMIT 1",
        ws,
        execution,
        incident,
    )
    .fetch_optional(db)
    .await?)
}

/// Rows due for a check (system scan; every later query carries the row's workspace).
pub async fn due(db: impl PgExecutor<'_>, limit: i64) -> Result<Vec<Row>, AppError> {
    Ok(sqlx::query_as!(
        Row,
        "SELECT id, workspace_id, execution_id, source_ref, target_kind, target_external_id,
                due_at, status, checks, last_checked_at
         FROM exchange_expectations
         WHERE status IN ('pending', 'late') AND due_at <= now() AND checks < 10
         ORDER BY due_at LIMIT $1",
        limit,
    )
    .fetch_all(db)
    .await?)
}

/// One provider call spent (budget ≤ 10). `spend` false for a person's Verify (G).
pub async fn checked(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    spend: bool,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE exchange_expectations
         SET checks = least(checks + CASE WHEN $3 THEN 1 ELSE 0 END, 10),
             last_checked_at = now(), updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        spend,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Incident refs of an expectation: from its execution's request and pair (decision B).
async fn refs(tx: &mut PgConnection, r: &Row) -> Result<IncidentRefs, AppError> {
    let Some(e) = r.execution_id else {
        return Ok(IncidentRefs::default());
    };
    let x = sqlx::query!(
        "SELECT e.approval_request_id, q.pair_id, p.target_connection_id
         FROM workflow_executions e
         JOIN approval_requests q ON q.workspace_id = e.workspace_id AND q.id = e.approval_request_id
         JOIN integration_pairs p ON p.workspace_id = q.workspace_id AND p.id = q.pair_id
         WHERE e.workspace_id = $1 AND e.id = $2",
        r.workspace_id,
        e,
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok(IncidentRefs {
        pair_id: Some(x.pair_id),
        connection_id: Some(x.target_connection_id),
        approval_request_id: Some(x.approval_request_id),
        execution_id: Some(e),
    })
}

/// `diagnosis` jsonb (§11.3): code hidden, sentence shown, records, last attempt.
pub async fn diagnosis(
    tx: &mut PgConnection,
    r: &Row,
    code: &str,
    kind: &str,
    category: &str,
    target_id: Option<&str>,
) -> Result<Value, AppError> {
    let last = sqlx::query!(
        r#"SELECT attempt_no, outcome, (evidence->>'http_status')::int AS "http_status?"
           FROM workflow_execution_attempts
           WHERE workspace_id = $1 AND execution_id = $2 ORDER BY attempt_no DESC LIMIT 1"#,
        r.workspace_id,
        r.execution_id,
    )
    .fetch_optional(&mut *tx)
    .await?
    .map(
        |a| json!({"attempt_no": a.attempt_no, "outcome": a.outcome, "http_status": a.http_status}),
    );
    Ok(json!({
        "code": code,
        "sentence": sentence(kind, category),
        "records": [{"subject_ref": r.source_ref, "target_id": target_id}],
        "last_attempt": last,
    }))
}

async fn event(tx: &mut PgConnection, r: &Row, kind: &str, payload: Value) -> Result<(), AppError> {
    if let Some(e) = r.execution_id {
        let at = EventRef {
            execution_id: Some(e),
            ..EventRef::default()
        };
        events::record(&mut *tx, r.workspace_id, at, kind, payload, Actor::System).await?;
    }
    Ok(())
}

/// `pending|late → met`; a linked open incident closes `reconciled` with `verified_at`.
pub async fn met(
    tx: &mut PgConnection,
    r: &Row,
    target_id: Option<&str>,
) -> Result<bool, AppError> {
    let moved = sqlx::query_scalar!(
        "UPDATE exchange_expectations SET status = 'met', updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status IN ('pending', 'late')
         RETURNING incident_id",
        r.workspace_id,
        r.id,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(incident) = moved else {
        return Ok(false);
    };
    event(tx, r, "expectation_met", json!({"target_id": target_id})).await?;
    if let Some(i) = incident {
        let closed = sqlx::query!(
            "UPDATE workflow_incidents SET status = 'resolved', resolved_at = now(),
                 verified_at = now(), resolution_kind = 'reconciled', updated_at = now()
             WHERE workspace_id = $1 AND id = $2 AND status = 'open'",
            r.workspace_id,
            i,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if closed == 1 {
            event(tx, r, "incident_verified", json!({"incident_id": i})).await?;
        }
    }
    Ok(true)
}

/// Mark the expectation of an execution `met` (writer's verify), if there is one.
pub async fn met_for_execution(
    tx: &mut PgConnection,
    ws: Uuid,
    execution: Uuid,
    target_id: &str,
) -> Result<(), AppError> {
    if let Some(r) = find(&mut *tx, ws, Some(execution), None).await? {
        met(tx, &r, Some(target_id)).await?;
    }
    Ok(())
}

/// `pending → late` + one `exchange_late` incident (open once per key).
pub async fn late(tx: &mut PgConnection, r: &Row, category: &str) -> Result<(), AppError> {
    let d = diagnosis(tx, r, "not_found", "exchange_late", category, None).await?;
    let refs = refs(tx, r).await?;
    let (inc, created) = incidents::open_classified(
        tx,
        r.workspace_id,
        "exchange_late",
        &r.source_ref,
        refs,
        json!({}),
        Some((category, d)),
    )
    .await?;
    sqlx::query!(
        "UPDATE exchange_expectations SET status = 'late', incident_id = $3, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status IN ('pending', 'late')",
        r.workspace_id,
        r.id,
        inc,
    )
    .execute(&mut *tx)
    .await?;
    if created {
        event(tx, r, "expectation_late", json!({"incident_id": inc})).await?;
    }
    Ok(())
}

/// `pending|late → failed` + incident of the classified kind, or link an existing one
/// (`incident` = e.g. WF-01's `target_conflict`).
pub async fn fail(
    tx: &mut PgConnection,
    r: &Row,
    c: Classified,
    code: &str,
    incident: Option<Uuid>,
) -> Result<(), AppError> {
    let inc = match incident {
        Some(i) => i,
        None => {
            let d = diagnosis(tx, r, code, c.kind, c.category, None).await?;
            let refs = refs(tx, r).await?;
            incidents::open_classified(
                tx,
                r.workspace_id,
                c.kind,
                &r.source_ref,
                refs,
                json!({}),
                Some((c.category, d)),
            )
            .await?
            .0
        }
    };
    let moved = sqlx::query!(
        "UPDATE exchange_expectations SET status = 'failed', incident_id = $3, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status IN ('pending', 'late')",
        r.workspace_id,
        r.id,
        inc,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if moved == 1 {
        event(
            tx,
            r,
            "expectation_failed",
            json!({"incident_id": inc, "category": c.category}),
        )
        .await?;
    }
    Ok(())
}

/// Writer side: fail the expectation of an execution after a permanent rejection.
pub async fn fail_for_execution(
    tx: &mut PgConnection,
    ws: Uuid,
    execution: Uuid,
    c: Classified,
    code: &str,
) -> Result<(), AppError> {
    if let Some(r) = find(&mut *tx, ws, Some(execution), None).await? {
        fail(tx, &r, c, code, None).await?;
    }
    Ok(())
}

/// Incident resolved by a person: its `failed|late` expectation → `closed`.
pub async fn close_for_incident(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    incident: Uuid,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE exchange_expectations SET status = 'closed', updated_at = now()
         WHERE workspace_id = $1 AND incident_id = $2 AND status IN ('failed', 'late')",
        ws,
        incident,
    )
    .execute(db)
    .await?;
    Ok(())
}
