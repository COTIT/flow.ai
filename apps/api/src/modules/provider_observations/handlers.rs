//! HTTP edge for /v1/workspace/{workspace}/accounting/exceptions: reads need
//! `accounting.read`; retry/resolve need `accounting.post`, take the A envelope plus a required
//! Idempotency-Key and replay permanently by `client_ref`.

use axum::extract::{Path, Query, State};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Command},
    modules::{
        accounting::dto::Action,
        provider_observations::{
            dto::{
                ExceptionResource, ListQuery, ResolvePayload, ResolveReview, ResolveReviewBody,
                RetryPayload,
            },
            service,
        },
    },
};

permissions! {
    Read => "accounting.read",
    Post => "accounting.post",
}

fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// accounting_exceptions_list: Provider observations the book could not post or must recover.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/exceptions", tag = "accounting",
    operation_id = "accounting_exceptions_list",
    summary = "List accounting exceptions",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Envelope<Vec<ExceptionResource>>, description = "Newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn accounting_exceptions_list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<ListQuery>,
) -> Result<Data<Vec<ExceptionResource>>, AppError> {
    let all = q.state.as_deref() == Some("all");
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    Ok(Data(
        service::list(&state, access.workspace_id, !all, q.before, limit).await?,
    ))
}

/// accounting_exceptions_get: One exception with its observation.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/exceptions/{exception}", tag = "accounting",
    operation_id = "accounting_exceptions_get",
    summary = "Read an accounting exception",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("exception" = Uuid, Path, description = "Exception id")),
    responses(
        (status = 200, body = Envelope<ExceptionResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or exception, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn accounting_exceptions_get(
    State(state): State<AppState>,
    access: Can<Read>,
    Path((_, exception)): Path<(String, String)>,
) -> Result<Data<ExceptionResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, id(&exception)?).await?,
    ))
}

/// accounting_exceptions_retry: Rerun the posting of a pending observation (no provider call).
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/exceptions/{exception}/retry", tag = "accounting",
    operation_id = "accounting_exceptions_retry",
    summary = "Retry posting a provider observation",
    request_body = Action<RetryPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("exception" = Uuid, Path, description = "Exception id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<ExceptionResource>, description = "Posted"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or exception, or not a member"),
        (status = 409, body = MessageBody, description = "`exception_state` (resolved or a discrepancy) or `stale_document`"),
        (status = 422, body = ValidationErrorBody, description = "Still can't post (reason in message; attempt counted)"),
    ),
    security(("bearer" = []))
)]
pub async fn accounting_exceptions_retry(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, exception)): Path<(String, String)>,
    cmd: Command<Action<RetryPayload>>,
) -> Result<Answer, AppError> {
    service::retry(&state, &access, id(&exception)?, cmd).await
}

/// accounting_exceptions_resolve_review: Preview resolving a discrepancy.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/exceptions/{exception}/resolve/review", tag = "accounting",
    operation_id = "accounting_exceptions_resolve_review",
    summary = "Preview resolving an accounting exception",
    request_body = ResolveReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("exception" = Uuid, Path, description = "Exception id")),
    responses(
        (status = 200, body = Envelope<ResolveReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or exception, or not a member"),
        (status = 409, body = MessageBody, description = "`exception_state`: already resolved"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown customer, or a pending posting (retry it)"),
    ),
    security(("bearer" = []))
)]
pub async fn accounting_exceptions_resolve_review(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, exception)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ResolveReviewBody>,
) -> Result<Data<ResolveReview>, AppError> {
    Ok(Data(
        service::resolve_review(&state, access.workspace_id, id(&exception)?, &input).await?,
    ))
}

/// accounting_exceptions_resolve: Assign a recovery to a customer receivable or write it off.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/exceptions/{exception}/resolve", tag = "accounting",
    operation_id = "accounting_exceptions_resolve",
    summary = "Resolve an accounting exception",
    request_body = Action<ResolvePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("exception" = Uuid, Path, description = "Exception id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<ExceptionResource>, description = "Resolved"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or exception, or not a member"),
        (status = 409, body = MessageBody, description = "`exception_state` or `stale_document` (version or review changed)"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown customer, a pending posting, or `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn accounting_exceptions_resolve(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, exception)): Path<(String, String)>,
    cmd: Command<Action<ResolvePayload>>,
) -> Result<Answer, AppError> {
    service::resolve(&state, &access, id(&exception)?, cmd).await
}
