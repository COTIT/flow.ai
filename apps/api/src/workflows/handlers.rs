//! HTTP edge for /v1/workspace/{workspace}/workflows/*.

use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        list::Paginated,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Keyed},
    workflows::{
        dto::{
            ExecutionListQuery, ExecutionResource, IncidentDetail, IncidentListQuery,
            IncidentResolveRequest, IncidentResource, ResolveRequest, RetryRequest,
        },
        service,
    },
};

permissions! {
    IntegrationView => "integration.view",
    IntegrationManage => "integration.manage",
    IntegrationReconcile => "integration.reconcile",
}

/// GET /workflows/executions.
#[utoipa::path(
    get, path = "/workspace/{workspace}/workflows/executions", tag = "workflows",
    operation_id = "workflows_executions_list", summary = "List workflow runs",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ExecutionListQuery),
    responses(
        (status = 200, body = Paginated<ExecutionResource>, description = "One page, newest first"),
        (status = 403, body = MessageBody, description = "Missing `integration.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown status"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Query(q): Query<ExecutionListQuery>,
) -> Result<Json<Paginated<ExecutionResource>>, AppError> {
    Ok(Json(service::list(&state, access.workspace_id, q).await?))
}

/// GET /workflows/executions/{execution}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/workflows/executions/{execution}", tag = "workflows",
    operation_id = "workflows_executions_get", summary = "Get a workflow run",
    description = "Adds `attempts`, `events` (readable `summary`) and `resolutions`.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("execution" = Uuid, Path, description = "Run id")),
    responses(
        (status = 200, body = Envelope<ExecutionResource>, description = "The run"),
        (status = 403, body = MessageBody, description = "Missing `integration.view`"),
        (status = 404, body = MessageBody, description = "Unknown or foreign run"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<ExecutionResource>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /workflows/executions/{execution}/retry.
#[utoipa::path(
    post, path = "/workspace/{workspace}/workflows/executions/{execution}/retry", tag = "workflows",
    operation_id = "workflows_executions_retry", summary = "Retry a blocked run after a fix",
    description = "`blocked` → `queued`; every check runs again. After a settled attempt with no unused grant: 409 `retry_needs_grant` (a reconciler must `allow_retry`).",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"), ("execution" = Uuid, Path, description = "Run id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters; a retry replays the first answer."),
    ),
    request_body = RetryRequest,
    responses(
        (status = 200, body = Envelope<ExecutionResource>, description = "Queued again"),
        (status = 403, body = MessageBody, description = "Missing `integration.manage`"),
        (status = 404, body = MessageBody, description = "Unknown or foreign run"),
        (status = 409, body = MessageBody, description = "`execution_state_changed` or `retry_needs_grant`"),
        (status = 422, body = ValidationErrorBody, description = "Reason missing"),
    ),
    security(("bearer" = []))
)]
pub async fn retry(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, id)): Path<(String, String)>,
    Keyed { input, key }: Keyed<RetryRequest>,
) -> Result<Answer, AppError> {
    service::retry(&state, &access, &id, input, key).await
}

/// POST /workflows/executions/{execution}/resolve.
#[utoipa::path(
    post, path = "/workspace/{workspace}/workflows/executions/{execution}/resolve", tag = "workflows",
    operation_id = "workflows_executions_resolve", summary = "Resolve a run by hand",
    description = "Not the request's requester. `allow_retry` grants one more attempt (latest outcome ambiguous, not_sent or rejected); `close_no_po` closes a run awaiting reconciliation; `accept_variance` / `reject_variance` review a tax variance; `link_existing` reads the order from NetSuite and verifies it: match → `succeeded`, differences → `resolved_manual` `linked_with_differences` (422 when the id can't be read, 503 when NetSuite can't be reached). Audited in `resolutions`.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"), ("execution" = Uuid, Path, description = "Run id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters; a retry replays the first answer."),
    ),
    request_body = ResolveRequest,
    responses(
        (status = 200, body = Envelope<ExecutionResource>, description = "Resolved"),
        (status = 403, body = MessageBody, description = "`four_eyes_required`, or missing `integration.reconcile`"),
        (status = 404, body = MessageBody, description = "Unknown or foreign run"),
        (status = 409, body = MessageBody, description = "`execution_state_changed`, or `retry_already_granted` (`allow_retry` while a grant is unused)"),
        (status = 422, body = ValidationErrorBody, description = "Unknown action, reason missing, `target_id` missing for link"),
    ),
    security(("bearer" = []))
)]
pub async fn resolve(
    State(state): State<AppState>,
    access: Can<IntegrationReconcile>,
    Path((_, id)): Path<(String, String)>,
    Keyed { input, key }: Keyed<ResolveRequest>,
) -> Result<Answer, AppError> {
    service::resolve(&state, &access, &id, input, key).await
}

/// GET /workflows/incidents.
#[utoipa::path(
    get, path = "/workspace/{workspace}/workflows/incidents", tag = "workflows",
    operation_id = "workflows_incidents_list", summary = "List workflow incidents",
    params(("workspace" = Uuid, Path, description = "Workspace id"), IncidentListQuery),
    responses(
        (status = 200, body = Paginated<IncidentResource>, description = "One page, newest first"),
        (status = 403, body = MessageBody, description = "Missing `integration.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown status or category"),
    ),
    security(("bearer" = []))
)]
pub async fn incidents(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Query(q): Query<IncidentListQuery>,
) -> Result<Json<Paginated<IncidentResource>>, AppError> {
    Ok(Json(
        service::incidents(&state, access.workspace_id, q).await?,
    ))
}

/// GET /workflows/incidents/{incident}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/workflows/incidents/{incident}", tag = "workflows",
    operation_id = "workflows_incidents_get", summary = "Get a workflow incident",
    description = "Adds the watched `expectation` and the run's `timeline` (readable `summary`).",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("incident" = Uuid, Path, description = "Incident id")),
    responses(
        (status = 200, body = Envelope<IncidentDetail>, description = "The incident"),
        (status = 403, body = MessageBody, description = "Missing `integration.view`"),
        (status = 404, body = MessageBody, description = "Unknown or foreign incident"),
    ),
    security(("bearer" = []))
)]
pub async fn get_incident(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<IncidentDetail>, AppError> {
    Ok(Data(
        service::get_incident(&state, access.workspace_id, &id).await?,
    ))
}

/// POST /workflows/incidents/{incident}/verify.
#[utoipa::path(
    post, path = "/workspace/{workspace}/workflows/incidents/{incident}/verify", tag = "workflows",
    operation_id = "workflows_incidents_verify", summary = "Check the other system now",
    description = "One read-back of the watched record. A match closes the incident as `reconciled` with `verified_at`. Never writes to the other system.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"), ("incident" = Uuid, Path, description = "Incident id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters; a retry replays the first answer."),
    ),
    responses(
        (status = 200, body = Envelope<IncidentDetail>, description = "Checked"),
        (status = 403, body = MessageBody, description = "Missing `integration.reconcile`"),
        (status = 404, body = MessageBody, description = "Unknown or foreign incident"),
        (status = 409, body = MessageBody, description = "`incident_already_resolved`"),
        (status = 422, body = MessageBody, description = "`incident_not_verifiable`"),
    ),
    security(("bearer" = []))
)]
pub async fn verify_incident(
    State(state): State<AppState>,
    access: Can<IntegrationReconcile>,
    Path((_, id)): Path<(String, String)>,
    crate::idempotency::KeyOnly(key): crate::idempotency::KeyOnly,
) -> Result<Answer, AppError> {
    service::verify_incident(&state, &access, &id, key).await
}

/// POST /workflows/incidents/{incident}/resolve.
#[utoipa::path(
    post, path = "/workspace/{workspace}/workflows/incidents/{incident}/resolve", tag = "workflows",
    operation_id = "workflows_incidents_resolve", summary = "Resolve an incident",
    description = "Not the requester of the linked request. Once; no reopen. `resolution_kind` `manual_closed` (default) or `reconciled` (needs `verified_at`).",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"), ("incident" = Uuid, Path, description = "Incident id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters; a retry replays the first answer."),
    ),
    request_body = IncidentResolveRequest,
    responses(
        (status = 200, body = Envelope<IncidentResource>, description = "Resolved"),
        (status = 403, body = MessageBody, description = "`four_eyes_required`, or missing `integration.reconcile`"),
        (status = 404, body = MessageBody, description = "Unknown or foreign incident"),
        (status = 409, body = MessageBody, description = "`incident_already_resolved`, or `incident_not_verified` (`reconciled` before a verified read-back)"),
        (status = 422, body = ValidationErrorBody, description = "Note missing, unknown resolution kind"),
    ),
    security(("bearer" = []))
)]
pub async fn resolve_incident(
    State(state): State<AppState>,
    access: Can<IntegrationReconcile>,
    Path((_, id)): Path<(String, String)>,
    Keyed { input, key }: Keyed<IncidentResolveRequest>,
) -> Result<Answer, AppError> {
    service::resolve_incident(&state, &access, &id, input, key).await
}
