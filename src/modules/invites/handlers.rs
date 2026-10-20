//! HTTP edge for invite endpoints: extract, call service, wrap response.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::AppError,
    http::{
        auth::AuthUser,
        json::ValidatedJson,
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::{
        auth::dto::TokenResponse,
        invites::{
            dto::{
                AnswerRequest, InviteCreated, InviteStatusResponse, RegisterRequest,
                SendInviteRequest,
            },
            service,
        },
    },
};

permissions!(InviteCreate => "invite.create");

/// POST /v1/workspace/{workspace}/invites -> 201 `{data:{invite_id}}`.
pub async fn send(
    State(state): State<AppState>,
    access: Can<InviteCreate>,
    ValidatedJson(invite): ValidatedJson<SendInviteRequest>,
) -> Result<(StatusCode, Data<InviteCreated>), AppError> {
    let invite_id = service::send(
        &state,
        access.user_id,
        access.is_admin(),
        access.workspace_id,
        invite,
    )
    .await?;
    Ok((StatusCode::CREATED, Data(InviteCreated { invite_id })))
}

/// GET /v1/invites/get-invite-status/{code} -> 200 status body.
pub async fn status(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Data<InviteStatusResponse>, AppError> {
    Ok(Data(service::status(&state, &code).await?.into()))
}

/// POST /v1/register -> 201 token body.
pub async fn register(
    State(state): State<AppState>,
    ValidatedJson(input): ValidatedJson<RegisterRequest>,
) -> Result<(StatusCode, Data<TokenResponse>), AppError> {
    let session = service::register(&state, input).await?;
    Ok((StatusCode::CREATED, Data(session.into())))
}

/// POST /v1/invites -> 200 `{"message":"ok"}`.
pub async fn answer(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(input): ValidatedJson<AnswerRequest>,
) -> Result<Message, AppError> {
    service::answer(&state, user.user_id, input).await?;
    Ok(Message("ok"))
}
