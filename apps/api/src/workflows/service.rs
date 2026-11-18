//! Human actions on executions and incidents (contracts §3.2, §3.5, §5.5, §10): retry after
//! fix, resolve (allow_retry, close_no_po, variance, link seam), incident resolve. Each one is
//! a single transaction: row lock, state check, move, resolution row, event, stored answer.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::Rules,
        list::{Page, Paginated},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Actor, Answer, Key},
    modules::approvals::dto::Person,
    workflows::{
        dispatch,
        dto::{
            AttemptView, EventView, ExecutionListQuery, ExecutionResource, IncidentListQuery,
            IncidentResource, ResolutionView, ResolveInput,
        },
        events::{self, Actor as EventActor, EventRef},
        repo::{self, ExecutionRow, HumanMove, IncidentRow, NewResolution},
        wf01_purchase::link,
    },
};

const STATUSES: [&str; 9] = [
    "queued",
    "running",
    "blocked",
    "awaiting_reconciliation",
    "cancelled",
    "succeeded",
    "succeeded_with_variance",
    "resolved_manual",
    "failed",
];
/// Outcomes after which one more attempt may be granted (§3.4).
const RETRYABLE: [&str; 3] = ["ambiguous", "not_sent", "rejected"];

fn reason(status: StatusCode, code: &'static str, message: &str) -> AppError {
    AppError::Reason {
        status,
        code,
        message: message.into(),
    }
}

fn changed() -> AppError {
    reason(
        StatusCode::CONFLICT,
        "execution_state_changed",
        "This run changed state. Reload and try again.",
    )
}

fn four_eyes() -> AppError {
    reason(
        StatusCode::FORBIDDEN,
        "four_eyes_required",
        "You cannot resolve a request you raised.",
    )
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn grantable(e: &ExecutionRow) -> bool {
    matches!(e.status.as_str(), "awaiting_reconciliation" | "blocked")
        && e.latest_outcome
            .as_deref()
            .is_some_and(|o| RETRYABLE.contains(&o))
}

/// Human actions open on a row (UI shows only these).
fn allowed(e: &ExecutionRow) -> Vec<&'static str> {
    let mut out = vec![];
    match e.status.as_str() {
        "awaiting_reconciliation" => {
            out.push("link_existing");
            if grantable(e) {
                out.push("allow_retry");
            }
            out.push("close_no_po");
        }
        "succeeded_with_variance" => out.extend(["accept_variance", "reject_variance"]),
        "blocked" => {
            if grantable(e) {
                out.push("allow_retry");
            }
            out.push("retry");
        }
        _ => {}
    }
    out
}

fn resource(e: ExecutionRow) -> ExecutionResource {
    ExecutionResource {
        allowed_actions: allowed(&e),
        verified: matches!(e.status.as_str(), "succeeded" | "succeeded_with_variance"),
        id: e.id,
        approval_request_id: e.approval_request_id,
        workflow_key: e.workflow_key,
        correlation_id: e.correlation_id,
        status: e.status,
        phase: e.phase,
        reason: e.reason,
        resolution: e.resolution,
        error_class: e.error_class,
        error_message: e.error_detail,
        target_ref: e.target_ref,
        remote_attempts: e.remote_attempts,
        manual_retry_grants: e.manual_retry_grants,
        created_at: e.created_at,
        updated_at: e.updated_at,
        attempts: None,
        events: None,
        resolutions: None,
    }
}

/// Readable timeline line per event kind (§3.7); never a raw code.
fn summary(kind: &str) -> &'static str {
    match kind {
        "decision" => "Approval decided",
        "claim" | "takeover" => "Run started",
        "precheck_failed" => "Checks before sending failed",
        "write_intent" => "Ready to create the order",
        "api_request" => "Order sent",
        "api_result" | "stale_worker_result" => "Answer received",
        "verify_match" => "Order checked: it matches",
        "verify_mismatch" => "Order checked: it differs",
        "reconcile_check" => "Looked for the order",
        "incident_opened" => "Problem flagged",
        "resolution" => "Resolved by a person",
        "invalidated" => "Request no longer valid",
        "resubmitted" => "Request sent again",
        _ => "Workflow step recorded",
    }
}

/// GET …/executions.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    q: ExecutionListQuery,
) -> Result<Paginated<ExecutionResource>, AppError> {
    if let Some(s) = &q.status {
        let mut r = Rules::default();
        r.check(
            "status",
            STATUSES.contains(&s.as_str()),
            "The selected status is invalid.",
        );
        r.into_result()?;
    }
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let rows = repo::executions(
        &state.db,
        ws,
        None,
        q.workflow_key.as_deref(),
        q.status.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    Ok(Paginated::new(
        rows.into_iter().map(resource).collect(),
        total,
        page,
    ))
}

async fn one(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<ExecutionRow, AppError> {
    repo::executions(&mut *conn, ws, Some(id), None, None, 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET …/executions/{id} with attempts, events and resolutions.
pub async fn get(state: &AppState, ws: Uuid, id: &str) -> Result<ExecutionResource, AppError> {
    let id = parse(id)?;
    let mut conn = state.db.acquire().await?;
    let row = one(&mut conn, ws, id).await?;
    let request = row.approval_request_id;
    let mut res = resource(row);
    res.attempts = Some(
        repo::attempts(&mut *conn, ws, id)
            .await?
            .into_iter()
            .map(|a| AttemptView {
                attempt_no: a.attempt_no,
                granted_by: a.granted_by,
                grant_reason: a.grant_reason,
                write_intent_at: a.write_intent_at,
                sent_at: a.sent_at,
                outcome: a.outcome,
            })
            .collect(),
    );
    res.events = Some(
        repo::events(&mut *conn, ws, id, request)
            .await?
            .into_iter()
            .map(|e| EventView {
                summary: summary(&e.kind).to_owned(),
                kind: e.kind,
                actor_kind: e.actor_kind,
                created_at: e.created_at,
            })
            .collect(),
    );
    res.resolutions = Some(
        repo::resolutions(&mut *conn, ws, id)
            .await?
            .into_iter()
            .map(|r| ResolutionView {
                action: r.action,
                actor: Person {
                    user_id: r.actor_user_id,
                    name: r.actor_name,
                },
                reason: r.reason,
                outcome: r.outcome,
                created_at: r.created_at,
            })
            .collect(),
    );
    Ok(res)
}

/// Lock, then read: 404 outside the workspace.
async fn locked(tx: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<ExecutionRow, AppError> {
    if !repo::lock_execution(&mut *tx, ws, id).await? {
        return Err(AppError::NotFound);
    }
    one(tx, ws, id).await
}

/// Resolution row (+ one per incident it closed) + `resolution` event + stored answer.
#[allow(clippy::too_many_arguments)]
async fn finish(
    mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
    access: &WorkspaceAccess,
    id: Uuid,
    action: &str,
    reason_text: &str,
    target_id: Option<&str>,
    outcome: &str,
    closed_incidents: &[Uuid],
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let ws = access.workspace_id;
    let res = NewResolution {
        execution_id: Some(id),
        incident_id: None,
        action,
        actor: access.user_id,
        reason: reason_text,
        target_id,
        outcome,
    };
    repo::insert_resolution(&mut *tx, ws, &res).await?;
    for &inc in closed_incidents {
        let r = NewResolution {
            incident_id: Some(inc),
            action: "resolve_incident",
            outcome: "resolved",
            target_id: None,
            ..res
        };
        repo::insert_resolution(&mut *tx, ws, &r).await?;
    }
    let at = EventRef {
        execution_id: Some(id),
        ..Default::default()
    };
    events::record(
        &mut *tx,
        ws,
        at,
        "resolution",
        json!({ "action": action, "outcome": outcome }),
        EventActor::User(access.user_id),
    )
    .await?;
    let row = one(&mut tx, ws, id).await?;
    let body = serde_json::to_vec(&json!({ "data": resource(row) }))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let answer = Answer { status: 200, body };
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST …/retry (`retry_after_fix`): blocked → queued; all pre-checks rerun. After a settled
/// attempt with no unused grant → 409 `retry_needs_grant` (§10).
pub async fn retry(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    reason_text: String,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(id)?;
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let e = locked(&mut tx, ws, id).await?;
    if e.status != "blocked" {
        return Err(changed());
    }
    // Cap reached: remote_attempts == 1 + grants.
    if e.has_attempt && e.remote_attempts > e.manual_retry_grants {
        return Err(reason(
            StatusCode::CONFLICT,
            "retry_needs_grant",
            "An order was already attempted. A reconciler must allow one more attempt first.",
        ));
    }
    let m = HumanMove {
        to: "queued",
        reason: None,
        resolution: None,
        phase: Some("prechecking"),
        grant: false,
    };
    repo::move_execution(&mut *tx, ws, id, &m).await?;
    dispatch::enqueue_execution(&mut tx, &e.workflow_key, id).await?;
    finish(
        tx,
        access,
        id,
        "retry_after_fix",
        &reason_text,
        None,
        "queued",
        &[],
        key,
    )
    .await
}

/// POST …/resolve: reconcile permission (route), not the request's requester.
pub async fn resolve(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    input: ResolveInput,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(id)?;
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let e = locked(&mut tx, ws, id).await?;
    if e.requester_user_id == Some(access.user_id) {
        return Err(four_eyes());
    }
    let action = input.action.as_str();
    let (m, outcome) = match action {
        "allow_retry" if grantable(&e) => (
            Some(HumanMove {
                to: "queued",
                reason: None,
                resolution: None,
                phase: Some("prechecking"),
                grant: true,
            }),
            "queued",
        ),
        "close_no_po" if e.status == "awaiting_reconciliation" => (
            Some(HumanMove {
                to: "resolved_manual",
                reason: Some("closed_no_po"),
                resolution: Some("closed_no_po"),
                phase: Some("done"),
                grant: false,
            }),
            "resolved_manual:closed_no_po",
        ),
        "accept_variance" if e.status == "succeeded_with_variance" => (
            Some(HumanMove {
                to: "resolved_manual",
                reason: Some("variance_accepted"),
                resolution: Some("variance_accepted"),
                phase: None,
                grant: false,
            }),
            "resolved_manual:variance_accepted",
        ),
        "reject_variance" if e.status == "succeeded_with_variance" => {
            (None, "succeeded_with_variance")
        }
        "link_existing" if e.status == "awaiting_reconciliation" => {
            let target = input.target_id.as_deref().unwrap_or_default();
            let outcome = link::link_existing(state, &mut tx, ws, id, target).await?;
            (None, outcome)
        }
        _ => return Err(changed()),
    };
    if let Some(m) = &m {
        repo::move_execution(&mut *tx, ws, id, m).await?;
    }
    if action == "allow_retry" {
        dispatch::enqueue_execution(&mut tx, &e.workflow_key, id).await?;
    }
    let mut closed = vec![];
    if action == "accept_variance" {
        for inc in repo::open_incidents(&mut *tx, ws, id, "tax_variance").await? {
            repo::resolve_incident(&mut *tx, ws, inc, access.user_id, &input.reason).await?;
            closed.push(inc);
        }
    }
    let target = input.target_id.as_deref();
    finish(
        tx,
        access,
        id,
        action,
        &input.reason,
        target,
        outcome,
        &closed,
        key,
    )
    .await
}

fn incident(r: IncidentRow) -> IncidentResource {
    IncidentResource {
        id: r.id,
        kind: r.kind,
        subject_ref: r.subject_ref,
        status: r.status,
        pair_id: r.pair_id,
        approval_request_id: r.approval_request_id,
        execution_id: r.execution_id,
        detail: r.detail,
        note: r.note,
        resolved_by: r.resolved_by,
        created_at: r.created_at,
    }
}

/// GET …/incidents.
pub async fn incidents(
    state: &AppState,
    ws: Uuid,
    q: IncidentListQuery,
) -> Result<Paginated<IncidentResource>, AppError> {
    if let Some(s) = &q.status {
        let mut r = Rules::default();
        r.check(
            "status",
            ["open", "resolved"].contains(&s.as_str()),
            "The selected status is invalid.",
        );
        r.into_result()?;
    }
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let rows = repo::incidents(
        &state.db,
        ws,
        None,
        q.status.as_deref(),
        q.kind.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    Ok(Paginated::new(
        rows.into_iter().map(incident).collect(),
        total,
        page,
    ))
}

/// POST …/incidents/{id}/resolve: reconcile, not the linked request's requester, once.
pub async fn resolve_incident(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    note: String,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(id)?;
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let row = repo::incidents(&mut *tx, ws, Some(id), None, None, 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    if row.requester_user_id == Some(access.user_id) {
        return Err(four_eyes());
    }
    if !repo::resolve_incident(&mut *tx, ws, id, access.user_id, &note).await? {
        return Err(reason(
            StatusCode::CONFLICT,
            "incident_already_resolved",
            "This incident is already resolved.",
        ));
    }
    let r = NewResolution {
        execution_id: row.execution_id,
        incident_id: Some(id),
        action: "resolve_incident",
        actor: access.user_id,
        reason: &note,
        target_id: None,
        outcome: "resolved",
    };
    repo::insert_resolution(&mut *tx, ws, &r).await?;
    if row.execution_id.is_some() || row.approval_request_id.is_some() {
        let at = EventRef {
            execution_id: row.execution_id,
            approval_request_id: row.approval_request_id,
            ..Default::default()
        };
        events::record(
            &mut *tx,
            ws,
            at,
            "resolution",
            json!({ "action": "resolve_incident", "incident_id": id }),
            EventActor::User(access.user_id),
        )
        .await?;
    }
    let row = repo::incidents(&mut *tx, ws, Some(id), None, None, 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    let body = serde_json::to_vec(&json!({ "data": incident(row) }))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let answer = Answer { status: 200, body };
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}
