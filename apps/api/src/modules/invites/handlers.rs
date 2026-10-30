//! HTTP edge for invite endpoints: extract, call service, wrap response.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::AppError,
    http::{
        auth::AuthUser,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::{
        auth::dto::TokenResponse,
        invites::{
            dto::{
                AnswerRequest, InviteCreated, InviteResource, InviteStatusResponse, InvitesQuery,
                RegisterRequest, SendInviteRequest,
            },
            service,
        },
    },
};

permissions! {
    InviteCreate => "invite.create",
    InviteList => "invite.list",
    InviteDelete => "invite.delete",
}

/// GET /v1/workspace/{workspace}/invites -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<InviteList>,
    Query(query): Query<InvitesQuery>,
) -> Result<Json<Paginated<InviteResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let status = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let (invites, total) = service::list(
        &state,
        access.workspace_id,
        status,
        page.size,
        page.offset(),
    )
    .await?;
    let data = invites.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// DELETE /v1/workspace/{workspace}/invites/{invite} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<InviteDelete>,
    Path((_, invite)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &invite).await?;
    Ok(Message("ok"))
}

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
