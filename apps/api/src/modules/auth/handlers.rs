//! HTTP edge for auth endpoints: extract, call service, wrap response.

use axum::{extract::State, http::StatusCode};

use crate::{
    AppState,
    error::AppError,
    http::{
        auth::AuthUser,
        json::ValidatedJson,
        response::{Data, Message},
    },
    modules::auth::{
        dto::{CreateWorkspaceRequest, LoginRequest, TokenResponse},
        service,
    },
};

/// POST /v1/create-workspace -> 201 token body.
pub async fn create_workspace(
    State(state): State<AppState>,
    ValidatedJson(input): ValidatedJson<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Data<TokenResponse>), AppError> {
    let session = service::create_workspace(&state, input).await?;
    Ok((StatusCode::CREATED, Data(session.into())))
}

/// POST /v1/login -> 200 token body.
pub async fn login(
    State(state): State<AppState>,
    ValidatedJson(input): ValidatedJson<LoginRequest>,
) -> Result<Data<TokenResponse>, AppError> {
    let session = service::login(&state, input).await?;
    Ok(Data(session.into()))
}

/// POST /v1/logout -> 200 `{"message":"ok"}`, current token dead.
pub async fn logout(State(state): State<AppState>, user: AuthUser) -> Result<Message, AppError> {
    service::logout(&state, user.token_id).await?;
    Ok(Message("ok"))
}
