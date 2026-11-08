//! HTTP edge for /v1/me and /v1/workspace/{workspace}/users.

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
        list::{Page, Paginated, contains_pattern},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::users::{
        dto::{MeResponse, MemberResource, MembersQuery, UserDetail, WorkspaceResource},
        repo::MemberFilter,
        service,
    },
};

permissions! {
    WorkspaceList => "workspace.list",
    // Laravel had no check on user detail.
    UserGet => "user.get",
}

/// GET /v1/workspace/{workspace}/users -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/users", tag = "users",
    operation_id = "users_list", summary = "List workspace members",
    params(("workspace" = Uuid, Path, description = "Workspace id"), MembersQuery),
    responses(
        (status = 200, body = Paginated<MemberResource>, description = "One page of members"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `workspace.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid filter"),
    ),
    security(("bearer" = []))
)]
pub async fn members(
    State(state): State<AppState>,
    access: Can<WorkspaceList>,
    Query(query): Query<MembersQuery>,
) -> Result<Json<Paginated<MemberResource>>, AppError> {
    let f = query.filter()?;
    let pattern = f.q.as_deref().map(contains_pattern);
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = MemberFilter {
        pattern: pattern.as_deref(),
        team_id: f.team_id,
        has_pin_code: f.has_pin_code,
        limit: page.size,
        offset: page.offset(),
    };
    let (views, total) = service::members(&state, access.workspace_id, &filter).await?;
    let data = views
        .into_iter()
        .map(|v| MemberResource::new(v, access.workspace_id))
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /v1/workspace/{workspace}/users/{user} -> `{data:{...}}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/users/{user}", tag = "users",
    operation_id = "users_get", summary = "Get a workspace member",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("user" = Uuid, Path, description = "User id"),
    ),
    responses(
        (status = 200, body = Envelope<UserDetail>, description = "The member, scoped to this workspace"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `user.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or user, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn member(
    State(state): State<AppState>,
    access: Can<UserGet>,
    Path((_, user)): Path<(String, String)>,
) -> Result<Data<UserDetail>, AppError> {
    let view = service::member(&state, access.workspace_id, &user).await?;
    Ok(Data(UserDetail::new(view, access.workspace_id)))
}

/// DELETE /v1/workspace/{workspace}/users/{user} -> 204.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/users/{user}", tag = "users",
    operation_id = "users_remove",
    summary = "Remove a member's access to this workspace (offboard someone who left; keeps their history)",
    description = "Owner or `workspace_admin` only. Access ends at once: the membership turns inactive, \
        the person leaves this workspace's teams, loses farm roles here, and their assistant sessions here close. \
        Orders, invoices and audit keep their name. Their login is deleted only when no other access is left. \
        Invite them again to give access back; only the new invite's team applies.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("user" = Uuid, Path, description = "User id of the member to remove"),
    ),
    responses(
        (status = 204, description = "Access removed"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller is neither owner nor `workspace_admin`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or user, user not an active member here, or caller not a member"),
        (status = 409, body = MessageBody, description = "Removing yourself or the workspace owner"),
    ),
    security(("bearer" = []))
)]
pub async fn remove(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, user)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let admin = access.is_admin();
    service::remove(&state, access.workspace_id, access.user_id, admin, &user).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /v1/me -> `{data:{user:{...}}}`.
#[utoipa::path(
    get, path = "/me", tag = "users",
    operation_id = "users_me", summary = "Get the logged-in user",
    responses(
        (status = 200, body = Envelope<MeResponse>, description = "The user, with permissions per workspace and pending invites"),
        (status = 403, body = MessageBody, description = "A phone-only (farmer) account: it has no email profile"),
    ),
    security(("bearer" = []))
)]
pub async fn me(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Data<MeResponse>, AppError> {
    Ok(Data(service::me(&state, user.user_id).await?.into()))
}

/// GET /v1/me/workspaces -> `{data:[...]}`.
#[utoipa::path(
    get, path = "/me/workspaces", tag = "users",
    operation_id = "users_workspaces", summary = "List the logged-in user's workspaces",
    responses(
        (status = 200, body = Envelope<Vec<WorkspaceResource>>, description = "Workspaces where the user is an active member"),
        (status = 403, body = MessageBody, description = "A phone-only (farmer) account: it has no email profile"),
    ),
    security(("bearer" = []))
)]
pub async fn workspaces(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Data<Vec<WorkspaceResource>>, AppError> {
    let list = service::workspaces(&state, user.user_id).await?;
    Ok(Data(list.into_iter().map(Into::into).collect()))
}
