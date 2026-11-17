//! HTTP edge for /v1/workspace/{workspace}/approvals/*.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    idempotency::{Answer, Keyed},
    modules::approvals::{
        dto::{
            ApprovalRequestResource, CreatePolicyRequest, DecideRequest, PageQuery, PolicyCreated,
            PolicyResource, RequestListQuery, ResubmitRequest, Resubmitted, UpdatePolicyRequest,
        },
        service::{self, Decision},
    },
};

permissions! {
    IntegrationView => "integration.view",
    IntegrationManage => "integration.manage",
}

/// GET /approvals/requests.
#[utoipa::path(
    get, path = "/workspace/{workspace}/approvals/requests", tag = "approvals",
    operation_id = "approvals_requests_list", summary = "List approval requests",
    description = "Newest first, without `snapshot`. `scope=all` needs an integration permission (view, approve, approve_high, reconcile or manage) and is otherwise forced to `mine`. `scope=decidable`: pending requests the caller may decide (not raised by them, needed permission held). Approval status, execution status and open incidents are separate fields.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), RequestListQuery),
    responses(
        (status = 200, body = Paginated<ApprovalRequestResource>, description = "One page"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown status or scope"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<RequestListQuery>,
) -> Result<Json<Paginated<ApprovalRequestResource>>, AppError> {
    Ok(Json(service::list(&state, &access, query).await?))
}

/// GET /approvals/requests/{request}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/approvals/requests/{request}", tag = "approvals",
    operation_id = "approvals_requests_get", summary = "Get an approval request",
    description = "Full request with `snapshot`. Integration roles see any request; other members only their own.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("request" = Uuid, Path, description = "Request id")),
    responses(
        (status = 200, body = Envelope<ApprovalRequestResource>, description = "The request"),
        (status = 404, body = MessageBody, description = "Unknown, foreign or not visible to the caller"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<ApprovalRequestResource>, AppError> {
    Ok(Data(service::get(&state, &access, &id).await?))
}

const DECIDE_DESC: &str = "Not the requester (owner included: 403 `four_eyes_required`). Needs the request's `required_permission`; `integration.approve_high` also covers `integration.approve`. Compares `fingerprint` with the current revision: changed → 409 `approval_stale`; no longer pending → 409 `approval_already_decided`. Approve queues one execution in the same transaction.";

async fn decide(
    state: AppState,
    access: WorkspaceAccess,
    id: String,
    decision: Decision,
    keyed: Keyed<DecideRequest>,
) -> Result<Answer, AppError> {
    service::decide(&state, &access, &id, decision, keyed.input, keyed.key).await
}

/// POST /approvals/requests/{request}/approve.
#[utoipa::path(
    post, path = "/workspace/{workspace}/approvals/requests/{request}/approve", tag = "approvals",
    operation_id = "approvals_requests_approve", summary = "Approve a request",
    description = DECIDE_DESC,
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"), ("request" = Uuid, Path, description = "Request id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters; a retry replays the first answer."),
    ),
    request_body = DecideRequest,
    responses(
        (status = 200, body = Envelope<ApprovalRequestResource>, description = "Approved; `execution` is queued"),
        (status = 403, body = MessageBody, description = "`four_eyes_required`, or permission missing"),
        (status = 404, body = MessageBody, description = "Unknown or foreign request"),
        (status = 409, body = MessageBody, description = "`approval_already_decided` or `approval_stale`"),
        (status = 422, body = ValidationErrorBody, description = "Bad fingerprint or Idempotency-Key"),
    ),
    security(("bearer" = []))
)]
pub async fn approve(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
    keyed: Keyed<DecideRequest>,
) -> Result<Answer, AppError> {
    decide(state, access, id, Decision::Approve, keyed).await
}

/// POST /approvals/requests/{request}/reject.
#[utoipa::path(
    post, path = "/workspace/{workspace}/approvals/requests/{request}/reject", tag = "approvals",
    operation_id = "approvals_requests_reject", summary = "Reject a request",
    description = DECIDE_DESC,
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"), ("request" = Uuid, Path, description = "Request id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters; a retry replays the first answer."),
    ),
    request_body = DecideRequest,
    responses(
        (status = 200, body = Envelope<ApprovalRequestResource>, description = "Rejected; no execution"),
        (status = 403, body = MessageBody, description = "`four_eyes_required`, or permission missing"),
        (status = 404, body = MessageBody, description = "Unknown or foreign request"),
        (status = 409, body = MessageBody, description = "`approval_already_decided` or `approval_stale`"),
        (status = 422, body = ValidationErrorBody, description = "Comment missing, bad fingerprint"),
    ),
    security(("bearer" = []))
)]
pub async fn reject(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
    keyed: Keyed<DecideRequest>,
) -> Result<Answer, AppError> {
    decide(state, access, id, Decision::Reject, keyed).await
}

/// POST /approvals/requests/{request}/return.
#[utoipa::path(
    post, path = "/workspace/{workspace}/approvals/requests/{request}/return", tag = "approvals",
    operation_id = "approvals_requests_return", summary = "Return a request to its requester",
    description = DECIDE_DESC,
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"), ("request" = Uuid, Path, description = "Request id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters; a retry replays the first answer."),
    ),
    request_body = DecideRequest,
    responses(
        (status = 200, body = Envelope<ApprovalRequestResource>, description = "Returned; no execution"),
        (status = 403, body = MessageBody, description = "`four_eyes_required`, or permission missing"),
        (status = 404, body = MessageBody, description = "Unknown or foreign request"),
        (status = 409, body = MessageBody, description = "`approval_already_decided` or `approval_stale`"),
        (status = 422, body = ValidationErrorBody, description = "Comment missing, bad fingerprint"),
    ),
    security(("bearer" = []))
)]
pub async fn return_request(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
    keyed: Keyed<DecideRequest>,
) -> Result<Answer, AppError> {
    decide(state, access, id, Decision::Return, keyed).await
}

/// POST /approvals/requests/{request}/resubmit.
#[utoipa::path(
    post, path = "/workspace/{workspace}/approvals/requests/{request}/resubmit", tag = "approvals",
    operation_id = "approvals_requests_resubmit", summary = "Resubmit a rejected or returned request",
    description = "The requester or `integration.manage`. Creates the next revision with the same fingerprint, `pending`, audited (`resubmitted_by`, event `resubmitted`).",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"), ("request" = Uuid, Path, description = "Request id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters; a retry replays the first answer."),
    ),
    request_body = ResubmitRequest,
    responses(
        (status = 201, body = Envelope<Resubmitted>, description = "New revision"),
        (status = 403, body = MessageBody, description = "Not the requester and no `integration.manage`"),
        (status = 404, body = MessageBody, description = "Unknown or foreign request"),
        (status = 409, body = MessageBody, description = "`approval_not_resubmittable`"),
        (status = 422, body = ValidationErrorBody, description = "Comment too long, bad Idempotency-Key"),
    ),
    security(("bearer" = []))
)]
pub async fn resubmit(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
    Keyed { input, key }: Keyed<ResubmitRequest>,
) -> Result<Answer, AppError> {
    service::resubmit(&state, &access, &id, input, key).await
}

/// GET /approvals/policies.
#[utoipa::path(
    get, path = "/workspace/{workspace}/approvals/policies", tag = "approvals",
    operation_id = "approvals_policies_list", summary = "List approval policies",
    params(("workspace" = Uuid, Path, description = "Workspace id"), PageQuery),
    responses(
        (status = 200, body = Paginated<PolicyResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "Missing `integration.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn policies(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<PolicyResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    Ok(Json(
        service::policies(&state, access.workspace_id, page).await?,
    ))
}

/// POST /approvals/policies.
#[utoipa::path(
    post, path = "/workspace/{workspace}/approvals/policies", tag = "approvals",
    operation_id = "approvals_policies_create", summary = "Create an approval policy",
    description = "One active policy per workflow and currency. Untaxed amounts below `threshold_minor` need `integration.approve`; from it up `integration.approve_high`.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreatePolicyRequest,
    responses(
        (status = 201, body = Envelope<PolicyCreated>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing `integration.manage`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid fields, or an active policy already exists"),
    ),
    security(("bearer" = []))
)]
pub async fn create_policy(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    ValidatedJson(input): ValidatedJson<CreatePolicyRequest>,
) -> Result<(StatusCode, Data<PolicyCreated>), AppError> {
    let policy_id = service::create_policy(&state, &access, input).await?;
    Ok((StatusCode::CREATED, Data(PolicyCreated { policy_id })))
}

/// POST /approvals/policies/{policy}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/approvals/policies/{policy}", tag = "approvals",
    operation_id = "approvals_policies_update", summary = "Update an approval policy",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("policy" = Uuid, Path, description = "Policy id")),
    request_body = UpdatePolicyRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Missing `integration.manage`"),
        (status = 404, body = MessageBody, description = "Unknown or foreign policy"),
        (status = 422, body = ValidationErrorBody, description = "Invalid fields, or activating would make a second active policy"),
    ),
    security(("bearer" = []))
)]
pub async fn update_policy(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<UpdatePolicyRequest>,
) -> Result<Message, AppError> {
    service::update_policy(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}
