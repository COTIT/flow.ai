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
    integrations::{connections, providers::SinkClient},
    modules::approvals::dto::Person,
    workflows::{
        dispatch,
        dto::{
            AttemptView, EventView, ExecutionListQuery, ExecutionResource, ExpectationView,
            IncidentDetail, IncidentListQuery, IncidentResolve, IncidentResource, ResolutionView,
            ResolveInput,
        },
        events::{self, Actor as EventActor, EventRef},
        recovery::{expectations, sweep},
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

/// Settled retryable attempt in a state a reconciler can act on.
fn settled_retryable(e: &ExecutionRow) -> bool {
    matches!(e.status.as_str(), "awaiting_reconciliation" | "blocked")
        && e.latest_outcome
            .as_deref()
            .is_some_and(|o| RETRYABLE.contains(&o))
}

/// A grant not yet spent by an attempt (cap: remote_attempts <= 1 + grants).
fn unused_grant(e: &ExecutionRow) -> bool {
    e.remote_attempts <= e.manual_retry_grants
}

fn grantable(e: &ExecutionRow) -> bool {
    settled_retryable(e) && !unused_grant(e)
}

fn already_granted() -> AppError {
    reason(
        StatusCode::CONFLICT,
        "retry_already_granted",
        "A retry is already allowed and not used yet. Run it with Retry.",
    )
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
        "expectation_met" => "Order confirmed in the other system",
        "expectation_late" => "Order not seen in time",
        "expectation_failed" => "Order refused by the other system",
        "incident_verified" => "Problem closed after a check",
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
    dispatch::enqueue_execution(&mut tx, ws, &e.workflow_key, id).await?;
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
        "allow_retry" if settled_retryable(&e) && unused_grant(&e) => {
            return Err(already_granted());
        }
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
        dispatch::enqueue_execution(&mut tx, ws, &e.workflow_key, id).await?;
    }
    let mut closed = vec![];
    if action == "accept_variance" {
        for inc in repo::open_incidents(&mut *tx, ws, id, "tax_variance").await? {
            repo::resolve_incident(
                &mut *tx,
                ws,
                inc,
                access.user_id,
                &input.reason,
                "manual_closed",
            )
            .await?;
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
    let diagnosis = r.diagnosis.map(|mut d| {
        if let Some(m) = d.as_object_mut() {
            m.remove("code");
        }
        d
    });
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
        category: r.category,
        diagnosis,
        verified_at: r.verified_at,
        resolution_kind: r.resolution_kind,
    }
}

const CATEGORIES: [&str; 6] = [
    "transient",
    "authorization",
    "data",
    "conflict",
    "missing",
    "unknown_outcome",
];

/// GET …/incidents.
pub async fn incidents(
    state: &AppState,
    ws: Uuid,
    q: IncidentListQuery,
) -> Result<Paginated<IncidentResource>, AppError> {
    let mut r = Rules::default();
    if let Some(s) = &q.status {
        r.check(
            "status",
            ["open", "resolved"].contains(&s.as_str()),
            "The selected status is invalid.",
        );
    }
    if let Some(c) = &q.category {
        r.check(
            "category",
            CATEGORIES.contains(&c.as_str()),
            "The selected category is invalid.",
        );
    }
    r.into_result()?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let rows = repo::incidents(
        &state.db,
        ws,
        None,
        q.status.as_deref(),
        q.kind.as_deref(),
        q.category.as_deref(),
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

async fn incident_row(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<IncidentRow, AppError> {
    repo::incidents(&mut *conn, ws, Some(id), None, None, None, 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// Incident + its expectation + the execution's timeline.
async fn detail(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<IncidentDetail, AppError> {
    let row = incident_row(conn, ws, id).await?;
    let exp = expectations::find(&mut *conn, ws, row.execution_id, Some(id)).await?;
    let timeline = match (row.execution_id, row.approval_request_id) {
        (Some(e), Some(r)) => repo::events(&mut *conn, ws, e, r)
            .await?
            .into_iter()
            .map(|e| EventView {
                summary: summary(&e.kind).to_owned(),
                kind: e.kind,
                actor_kind: e.actor_kind,
                created_at: e.created_at,
            })
            .collect(),
        _ => vec![],
    };
    Ok(IncidentDetail {
        incident: incident(row),
        expectation: exp.map(|x| ExpectationView {
            status: x.status,
            due_at: x.due_at,
            checks: x.checks,
            last_checked_at: x.last_checked_at,
            target_kind: x.target_kind,
            target_external_id: x.target_external_id,
        }),
        timeline,
    })
}

/// GET …/incidents/{id}.
pub async fn get_incident(
    state: &AppState,
    ws: Uuid,
    id: &str,
) -> Result<IncidentDetail, AppError> {
    let id = parse(id)?;
    let mut conn = state.db.acquire().await?;
    detail(&mut conn, ws, id).await
}

fn already_resolved() -> AppError {
    reason(
        StatusCode::CONFLICT,
        "incident_already_resolved",
        "This incident is already resolved.",
    )
}

/// POST …/incidents/{id}/verify: one read-back now (does not spend the sweep budget).
pub async fn verify_incident(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
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
    let row = incident_row(&mut tx, ws, id).await?;
    if row.status != "open" {
        return Err(already_resolved());
    }
    let exp = expectations::find(&state.db, ws, row.execution_id, Some(id))
        .await?
        .filter(|x| x.status == "pending" || x.status == "late")
        .ok_or_else(|| {
            reason(
                StatusCode::UNPROCESSABLE_ENTITY,
                "incident_not_verifiable",
                "This incident has no record to check against.",
            )
        })?;
    let e = exp.execution_id.ok_or(AppError::NotFound)?;
    let target = sqlx::query_scalar!(
        "SELECT p.target_connection_id FROM workflow_executions e
         JOIN approval_requests q ON q.workspace_id = e.workspace_id AND q.id = e.approval_request_id
         JOIN integration_pairs p ON p.workspace_id = q.workspace_id AND p.id = q.pair_id
         WHERE e.workspace_id = $1 AND e.id = $2",
        ws,
        e,
    )
    .fetch_one(&state.db)
    .await?;
    let conn = connections::load(&state.db, ws, target).await?;
    let sink = SinkClient::for_connection(&connections::service::ctx(state), &conn);
    sweep::check(&state.db, &exp, sink.as_ref().map_err(Clone::clone), false).await?;
    let body = serde_json::to_vec(&json!({ "data": detail(&mut tx, ws, id).await? }))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let answer = Answer { status: 200, body };
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST …/incidents/{id}/resolve: reconcile, not the linked request's requester, once.
pub async fn resolve_incident(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    input: IncidentResolve,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(id)?;
    let ws = access.workspace_id;
    let note = input.note;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let row = incident_row(&mut tx, ws, id).await?;
    if row.requester_user_id == Some(access.user_id) {
        return Err(four_eyes());
    }
    if row.status != "open" {
        return Err(already_resolved());
    }
    if input.resolution_kind == "reconciled" && row.verified_at.is_none() {
        return Err(reason(
            StatusCode::CONFLICT,
            "incident_not_verified",
            "This incident has no verified read-back yet. Run Verify first, or close it by hand with a reason.",
        ));
    }
    if !repo::resolve_incident(
        &mut *tx,
        ws,
        id,
        access.user_id,
        &note,
        &input.resolution_kind,
    )
    .await?
    {
        return Err(already_resolved());
    }
    expectations::close_for_incident(&mut *tx, ws, id).await?;
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
    let row = incident_row(&mut tx, ws, id).await?;
    let body = serde_json::to_vec(&json!({ "data": incident(row) }))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let answer = Answer { status: 200, body };
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}
