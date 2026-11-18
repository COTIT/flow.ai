//! HTTP edge for /v1/workspace/{workspace}/customer-deposits. Every route needs
//! `accounting.post`. Writes take the C/A envelope plus a required Idempotency-Key and replay
//! permanently by `client_ref`.

use axum::extract::{Path, State};
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
        accounting::dto::{Action, Create},
        customer_deposits::{
            dto::{
                ApplyPayload, ApplyReviewBody, DepositPayload, DepositResource, DepositReview,
                DepositReviewBody,
            },
            service,
        },
    },
};

permissions! {
    Post => "accounting.post",
}

/// customer_deposits_receive_review: Preview receiving a deposit (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/customer-deposits/review", tag = "customer_deposits",
    operation_id = "customer_deposits_receive_review",
    summary = "Preview receiving a customer deposit",
    request_body = DepositReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<DepositReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or money place, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, date other than the book's day, or `accounting_not_configured` / `unsupported_active_erp_operation` (setup book, foreign currency)"),
    ),
    security(("bearer" = []))
)]
pub async fn customer_deposits_receive_review(
    State(state): State<AppState>,
    access: Can<Post>,
    ValidatedJson(input): ValidatedJson<DepositReviewBody>,
) -> Result<Data<DepositReview>, AppError> {
    Ok(Data(
        service::receive_review(&state, access.workspace_id, &input).await?,
    ))
}

/// customer_deposits_receive: Receive a reviewed customer deposit.
#[utoipa::path(
    post, path = "/workspace/{workspace}/customer-deposits", tag = "customer_deposits",
    operation_id = "customer_deposits_receive",
    summary = "Receive a customer deposit",
    request_body = Create<DepositPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<DepositResource>, description = "Received"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or money place, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (review changed) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, date other than the book's day, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn customer_deposits_receive(
    State(state): State<AppState>,
    access: Can<Post>,
    cmd: Command<Create<DepositPayload>>,
) -> Result<Answer, AppError> {
    service::receive(&state, &access, cmd).await
}

fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// customer_deposits_apply_review: Preview applying a deposit to a recognised sale.
#[utoipa::path(
    post, path = "/workspace/{workspace}/customer-deposits/{deposit}/apply/review", tag = "customer_deposits",
    operation_id = "customer_deposits_apply_review",
    summary = "Preview applying a deposit",
    request_body = ApplyReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("deposit" = Uuid, Path, description = "Deposit id")),
    responses(
        (status = 200, body = Envelope<DepositReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or deposit, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or unknown recognised sale, or `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn customer_deposits_apply_review(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, deposit)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ApplyReviewBody>,
) -> Result<Data<DepositReview>, AppError> {
    Ok(Data(
        service::apply_review(&state, access.workspace_id, id(&deposit)?, &input).await?,
    ))
}

/// customer_deposits_apply: Apply a reviewed deposit to a recognised, unbilled sale.
#[utoipa::path(
    post, path = "/workspace/{workspace}/customer-deposits/{deposit}/apply", tag = "customer_deposits",
    operation_id = "customer_deposits_apply",
    summary = "Apply a deposit to a recognised sale",
    request_body = Action<ApplyPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("deposit" = Uuid, Path, description = "Deposit id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<DepositResource>, description = "Applied"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or deposit, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review changed) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, another customer, more than either residual, or `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn customer_deposits_apply(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, deposit)): Path<(String, String)>,
    cmd: Command<Action<ApplyPayload>>,
) -> Result<Answer, AppError> {
    service::apply(&state, &access, id(&deposit)?, cmd).await
}
