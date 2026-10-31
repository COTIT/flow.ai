//! HTTP edge for auth endpoints: extract, call service, wrap response.

use axum::{extract::State, http::StatusCode};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        auth::AnyUser,
        json::ValidatedJson,
        response::{Data, Envelope, Message, MessageBody},
    },
    modules::auth::{
        dto::{CreateWorkspaceRequest, LoginRequest, TokenResponse},
        service,
    },
};

/// POST /v1/create-workspace -> 201 token body.
#[utoipa::path(
    post, path = "/create-workspace", tag = "auth",
    operation_id = "auth_create_workspace", summary = "Sign up with a new workspace",
    request_body = CreateWorkspaceRequest,
    responses(
        (status = 201, body = Envelope<TokenResponse>, description = "Account and workspace created; logged in"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, email or workspace name taken"),
    )
)]
pub async fn create_workspace(
    State(state): State<AppState>,
    ValidatedJson(input): ValidatedJson<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Data<TokenResponse>), AppError> {
    let session = service::create_workspace(&state, input).await?;
    Ok((StatusCode::CREATED, Data(session.into())))
}

/// POST /v1/login -> 200 token body.
#[utoipa::path(
    post, path = "/login", tag = "auth",
    operation_id = "auth_login", summary = "Log in",
    request_body = LoginRequest,
    responses(
        (status = 200, body = Envelope<TokenResponse>, description = "Logged in"),
        (status = 401, body = MessageBody, description = "Wrong email or password"),
        (status = 403, body = MessageBody, description = "Account has no active workspace; no token issued"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ValidatedJson(input): ValidatedJson<LoginRequest>,
) -> Result<Data<TokenResponse>, AppError> {
    let session = service::login(&state, input).await?;
    Ok(Data(session.into()))
}

/// POST /v1/logout -> 200 `{"message":"ok"}`, current token dead.
#[utoipa::path(
    post, path = "/logout", tag = "auth",
    operation_id = "auth_logout", summary = "Log out (ends this token)",
    responses((status = 200, body = MessageBody, description = "Logged out")),
    security(("bearer" = []))
)]
pub async fn logout(
    State(state): State<AppState>,
    AnyUser(user): AnyUser,
) -> Result<Message, AppError> {
    service::logout(&state, user.token_id).await?;
    Ok(Message("ok"))
}
