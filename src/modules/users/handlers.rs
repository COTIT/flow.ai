//! HTTP edge for /v1/me endpoints.

use axum::extract::State;

use crate::{
    AppState,
    error::AppError,
    http::{auth::AuthUser, response::Data},
    modules::users::{
        dto::{MeResponse, WorkspaceResource},
        service,
    },
};

/// GET /v1/me -> `{data:{user:{...}}}`.
pub async fn me(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Data<MeResponse>, AppError> {
    Ok(Data(service::me(&state, user.user_id).await?.into()))
}

/// GET /v1/me/workspaces -> `{data:[...]}`.
pub async fn workspaces(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Data<Vec<WorkspaceResource>>, AppError> {
    let list = service::workspaces(&state, user.user_id).await?;
    Ok(Data(list.into_iter().map(Into::into).collect()))
}
