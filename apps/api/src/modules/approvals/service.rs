//! Approval rules: visibility scopes, decide (four-eyes, permission, fingerprint CAS, queue
//! execution), resubmit, policies (contracts §3.1, §5.3).

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::Rules,
        list::{Page, Paginated},
        workspace::{NOT_ALLOWED, WorkspaceAccess},
    },
    idempotency::{self, Actor, Answer, Key},
    modules::approvals::{
        dto::{
            Amount, ApprovalRequestResource, BlockedReason, DecideInput, DecisionView,
            ExecutionSummary, IncidentRef, NewPolicy, Person, PolicyPatch, PolicyResource,
            RequestListQuery, STATUSES,
        },
        repo::{self, Filter, NewDecision, RequestRow},
    },
    workflows::{
        dispatch,
        events::{self, Actor as EventActor, EventRef},
    },
};

const APPROVE: &str = "integration.approve";
const APPROVE_HIGH: &str = "integration.approve_high";
/// Any of these lets a member see every request (`scope=all`).
const VIEWERS: [&str; 5] = [
    "integration.view",
    APPROVE,
    APPROVE_HIGH,
    "integration.reconcile",
    "integration.manage",
];

/// approve / reject / return.
#[derive(Debug, Clone, Copy)]
pub enum Decision {
    Approve,
    Reject,
    Return,
}

impl Decision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Return => "return",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Reject => "rejected",
            Self::Return => "returned",
        }
    }
}

fn reason(status: StatusCode, code: &'static str, message: &str) -> AppError {
    AppError::Reason {
        status,
        code,
        message: message.into(),
    }
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Permissions the caller can decide with: `approve_high` also covers `approve` (O-2).
fn decide_perms(access: &WorkspaceAccess) -> Vec<String> {
    let mut perms = vec![];
    if access.allows(APPROVE) || access.allows(APPROVE_HIGH) {
        perms.push(APPROVE.to_owned());
    }
    if access.allows(APPROVE_HIGH) {
        perms.push(APPROVE_HIGH.to_owned());
    }
    perms
}

fn sees_all(access: &WorkspaceAccess) -> bool {
    VIEWERS.iter().any(|p| access.allows(p))
}

/// §3.1 checks 1-2 on a pending row.
fn can_decide(access: &WorkspaceAccess, row: &RequestRow) -> bool {
    row.status == "pending"
        && row.requester_user_id != Some(access.user_id)
        && row
            .required_permission
            .as_ref()
            .is_some_and(|p| decide_perms(access).contains(p))
}

fn resource(access: &WorkspaceAccess, row: RequestRow) -> ApprovalRequestResource {
    let can_decide = can_decide(access, &row);
    let requester = row.requester_user_id.map(|user_id| Person {
        user_id,
        name: row.requester_name.clone().unwrap_or_default(),
    });
    let decision = match (row.decision, row.decided_by, row.decision_at) {
        (Some(decision), Some(user_id), Some(created_at)) => Some(DecisionView {
            decision,
            actor: Person {
                user_id,
                name: row.decided_by_name.unwrap_or_default(),
            },
            comment: row.decision_comment,
            created_at,
        }),
        _ => None,
    };
    let execution = match (
        row.execution_id,
        row.execution_status,
        row.execution_phase,
        row.execution_updated_at,
    ) {
        (Some(id), Some(status), Some(phase), Some(updated_at)) => Some(ExecutionSummary {
            id,
            verified: matches!(status.as_str(), "succeeded" | "succeeded_with_variance"),
            status,
            phase,
            reason: row.execution_reason,
            resolution: row.execution_resolution,
            error_class: row.execution_error_class,
            error_message: row.execution_error_detail,
            target_ref: row.execution_target_ref,
            updated_at,
        }),
        _ => None,
    };
    ApprovalRequestResource {
        id: row.id,
        workflow_key: row.workflow_key,
        pair_id: row.pair_id,
        subject_ref: row.subject_ref,
        revision: row.revision,
        fingerprint: row.fingerprint,
        status: row.status,
        blocked_reasons: row
            .blocked_reasons
            .iter()
            .map(|r| BlockedReason::parse(r))
            .collect(),
        invalidated_reason: row.invalidated_reason,
        amount: Amount {
            minor: row.amount_minor,
            currency_ref: row.currency_ref,
            decimals: row.currency_decimals,
        },
        required_permission: row.required_permission,
        requester,
        decision,
        can_decide,
        execution,
        open_incidents: serde_json::from_value::<Vec<IncidentRef>>(row.open_incidents)
            .unwrap_or_default(),
        snapshot: row.snapshot,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// GET /approvals/requests.
pub async fn list(
    state: &AppState,
    access: &WorkspaceAccess,
    q: RequestListQuery,
) -> Result<Paginated<ApprovalRequestResource>, AppError> {
    let mut r = Rules::default();
    if let Some(s) = &q.status {
        r.check(
            "status",
            STATUSES.contains(&s.as_str()),
            "The selected status is invalid.",
        );
    }
    if let Some(s) = &q.scope {
        r.check(
            "scope",
            ["all", "mine", "decidable"].contains(&s.as_str()),
            "The selected scope is invalid.",
        );
    }
    r.into_result()?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let scope = match q.scope.as_deref() {
        Some("decidable") => "decidable",
        Some("mine") => "mine",
        _ if sees_all(access) => "all",
        _ => "mine",
    };
    let filter = Filter {
        id: None,
        workflow_key: q.workflow_key.as_deref(),
        status: q.status.as_deref(),
        requester: (scope == "mine").then_some(access.user_id),
        decidable_perms: (scope == "decidable").then(|| decide_perms(access)),
        caller: access.user_id,
        limit: page.size,
        offset: page.offset(),
    };
    let rows = repo::list(&state.db, access.workspace_id, &filter).await?;
    let total = rows.first().map_or(0, |r| r.total);
    let data = rows.into_iter().map(|row| resource(access, row)).collect();
    Ok(Paginated::new(data, total, page))
}

async fn load(
    conn: &mut sqlx::PgConnection,
    access: &WorkspaceAccess,
    id: Uuid,
) -> Result<ApprovalRequestResource, AppError> {
    let filter = Filter {
        id: Some(id),
        workflow_key: None,
        status: None,
        requester: None,
        decidable_perms: None,
        caller: access.user_id,
        limit: 1,
        offset: 0,
    };
    let row = repo::list(&mut *conn, access.workspace_id, &filter)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    Ok(resource(access, row))
}

/// GET /approvals/requests/{id}: visible to integration roles, else only to its requester.
pub async fn get(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
) -> Result<ApprovalRequestResource, AppError> {
    let id = parse(id)?;
    let mut conn = state.db.acquire().await?;
    let res = load(&mut conn, access, id).await?;
    if !sees_all(access) && res.requester.as_ref().map(|p| p.user_id) != Some(access.user_id) {
        return Err(AppError::NotFound);
    }
    Ok(res)
}

/// POST …/approve|reject|return. One transaction: CAS, decision row, event, and on approve
/// the `queued` execution + its job.
pub async fn decide(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    decision: Decision,
    input: DecideInput,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(id)?;
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(answer);
    }
    let facts = repo::decide_facts(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if facts.requester_user_id == Some(access.user_id) {
        return Err(reason(
            StatusCode::FORBIDDEN,
            "four_eyes_required",
            "You cannot decide a request you raised.",
        ));
    }
    let needed = facts.required_permission.as_deref().unwrap_or(APPROVE);
    let held = decide_perms(access);
    if !held.iter().any(|p| p == needed) {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let permission_used = if access.allows(needed) {
        needed
    } else {
        APPROVE_HIGH
    };
    if !matches!(decision, Decision::Approve) && input.comment.is_none() {
        let mut r = Rules::default();
        r.add("comment", "The comment field is required.");
        r.into_result()?;
    }
    if !repo::decide_cas(
        &mut *tx,
        ws,
        id,
        decision.status(),
        access.user_id,
        &input.fingerprint,
    )
    .await?
    {
        let now = repo::decide_facts(&mut *tx, ws, id)
            .await?
            .ok_or(AppError::NotFound)?;
        return Err(if now.status != "pending" {
            reason(
                StatusCode::CONFLICT,
                "approval_already_decided",
                "This request was already decided.",
            )
        } else {
            reason(
                StatusCode::CONFLICT,
                "approval_stale",
                "This request changed after you opened it. Reload to see the new version.",
            )
        });
    }
    // Pending implies a requester (blocked `no_requester` otherwise).
    let requester = facts
        .requester_user_id
        .ok_or_else(|| AppError::Internal("pending request without requester".into()))?;
    repo::insert_decision(
        &mut *tx,
        ws,
        &NewDecision {
            request_id: id,
            requester,
            actor: access.user_id,
            decision: decision.as_str(),
            comment: input.comment.as_deref(),
            fingerprint: &input.fingerprint,
            permission_used,
        },
    )
    .await?;
    let mut at = EventRef {
        approval_request_id: Some(id),
        ..Default::default()
    };
    if matches!(decision, Decision::Approve) {
        let execution = repo::insert_execution(&mut *tx, ws, id, &facts.workflow_key).await?;
        dispatch::enqueue_execution(&mut tx, ws, &facts.workflow_key, execution).await?;
        at.execution_id = Some(execution);
    }
    let payload = json!({ "decision": decision.as_str(), "permission_used": permission_used });
    events::record(
        &mut *tx,
        ws,
        at,
        "decision",
        payload,
        EventActor::User(access.user_id),
    )
    .await?;
    let res = load(&mut tx, access, id).await?;
    let answer = Answer {
        status: 200,
        body: serde_json::to_vec(&json!({ "data": res }))
            .map_err(|e| AppError::Internal(e.to_string()))?,
    };
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST …/resubmit: requester or `integration.manage`; latest revision, rejected or returned.
pub async fn resubmit(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    comment: Option<String>,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(id)?;
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(answer);
    }
    let facts = repo::resubmit_facts(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if facts.requester_user_id != Some(access.user_id) && !access.allows("integration.manage") {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let not = || {
        reason(
            StatusCode::CONFLICT,
            "approval_not_resubmittable",
            "Only the latest rejected or returned request can be resubmitted.",
        )
    };
    if !facts.is_latest || !matches!(facts.status.as_str(), "rejected" | "returned") {
        return Err(not());
    }
    let new = repo::insert_resubmission(&mut *tx, ws, id, access.user_id)
        .await?
        .ok_or_else(not)?;
    let at = EventRef {
        approval_request_id: Some(new),
        ..Default::default()
    };
    let payload: Value = json!({ "previous_id": id, "comment": comment });
    events::record(
        &mut *tx,
        ws,
        at,
        "resubmitted",
        payload,
        EventActor::User(access.user_id),
    )
    .await?;
    let answer = Answer::created(&json!({ "data": { "approval_request_id": new } }));
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// GET /approvals/policies.
pub async fn policies(
    state: &AppState,
    ws: Uuid,
    page: Page,
) -> Result<Paginated<PolicyResource>, AppError> {
    let rows = repo::policies(&state.db, ws, page.size, page.offset()).await?;
    let total = rows.first().map_or(0, |r| r.total);
    let data = rows
        .into_iter()
        .map(|r| PolicyResource {
            id: r.id,
            workflow_key: r.workflow_key,
            currency_ref: r.currency_ref,
            threshold_minor: r.threshold_minor,
            active: r.active,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok(Paginated::new(data, total, page))
}

fn duplicate_policy() -> AppError {
    let mut r = Rules::default();
    r.add(
        "currency_ref",
        "An active policy already exists for this currency.",
    );
    r.into_result()
        .err()
        .unwrap_or(AppError::Internal("rules".into()))
}

/// POST /approvals/policies.
pub async fn create_policy(
    state: &AppState,
    access: &WorkspaceAccess,
    p: NewPolicy,
) -> Result<Uuid, AppError> {
    repo::insert_policy(
        &state.db,
        access.workspace_id,
        &p.workflow_key,
        &p.currency_ref,
        p.threshold_minor,
        access.user_id,
    )
    .await?
    .ok_or_else(duplicate_policy)
}

/// POST /approvals/policies/{id}.
pub async fn update_policy(
    state: &AppState,
    ws: Uuid,
    id: &str,
    p: PolicyPatch,
) -> Result<(), AppError> {
    let id = parse(id)?;
    match repo::update_policy(&state.db, ws, id, p.threshold_minor, p.active).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(AppError::NotFound),
        Err(e)
            if e.as_database_error()
                .is_some_and(|d| d.is_unique_violation()) =>
        {
            Err(duplicate_policy())
        }
        Err(e) => Err(e.into()),
    }
}
