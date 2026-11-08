//! HTTP edge for invite endpoints: extract, call service, wrap response.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        auth::AuthUser,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, Message, MessageBody},
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
#[utoipa::path(
    get, path = "/workspace/{workspace}/invites", tag = "invites",
    operation_id = "invites_list", summary = "List sent invites",
    params(("workspace" = Uuid, Path, description = "Workspace id"), InvitesQuery),
    responses(
        (status = 200, body = Paginated<InviteResource>, description = "One page of invites"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invite.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    delete, path = "/workspace/{workspace}/invites/{invite}", tag = "invites",
    operation_id = "invites_delete", summary = "Withdraw an invite",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("invite" = Uuid, Path, description = "Invite id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invite.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invite, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<InviteDelete>,
    Path((_, invite)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &invite).await?;
    Ok(Message("ok"))
}

/// POST /v1/workspace/{workspace}/invites -> 201 `{data:{invite_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/invites", tag = "invites",
    operation_id = "invites_send",
    summary = "Invite someone by email, optionally into a team (role) such as Managers that sets their permissions",
    description = "Pass `team_id` (from `teams_list`) to give the person that team's role and permissions when they accept. \
        A removed member may be invited again; only this invite's team applies.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = SendInviteRequest,
    responses(
        (status = 201, body = Envelope<InviteCreated>, description = "Invite sent; returns its id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invite.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, already a member, or invite pending"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    get, path = "/invites/get-invite-status/{code}", tag = "invites",
    operation_id = "invites_status", summary = "Look up an invite link",
    params(("code" = String, Path, description = "Code from the invite link")),
    responses(
        (status = 200, body = Envelope<InviteStatusResponse>, description = "The invite"),
        (status = 404, body = MessageBody, description = "Unknown code"),
    )
)]
pub async fn status(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Data<InviteStatusResponse>, AppError> {
    Ok(Data(service::status(&state, &code).await?.into()))
}

/// POST /v1/register -> 201 token body.
#[utoipa::path(
    post, path = "/register", tag = "invites",
    operation_id = "invites_register", summary = "Create an account through an invite",
    request_body = RegisterRequest,
    responses(
        (status = 201, body = Envelope<TokenResponse>, description = "Account created and invite accepted; logged in"),
        (status = 404, body = MessageBody, description = "Unknown, used or expired invite"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or email mismatch"),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    ValidatedJson(input): ValidatedJson<RegisterRequest>,
) -> Result<(StatusCode, Data<TokenResponse>), AppError> {
    let session = service::register(&state, input).await?;
    Ok((StatusCode::CREATED, Data(session.into())))
}

/// POST /v1/invites -> 200 `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/invites", tag = "invites",
    operation_id = "invites_answer", summary = "Accept or decline an invite",
    request_body = AnswerRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Invite is for another email, or a phone-only (farmer) account"),
        (status = 404, body = MessageBody, description = "Unknown, used or expired invite"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn answer(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(input): ValidatedJson<AnswerRequest>,
) -> Result<Message, AppError> {
    service::answer(&state, user.user_id, input).await?;
    Ok(Message("ok"))
}
