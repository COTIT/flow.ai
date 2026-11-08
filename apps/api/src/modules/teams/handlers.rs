//! HTTP edge for /v1/workspace/{workspace}/users/teams: permission (via `Can`), call service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, contains_pattern},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::teams::{
        dto::{
            CreateTeamRequest, ListQuery, PermissionChangeReply, PermissionsRequest, TeamCreated,
            TeamResource, UpdateTeamRequest, UserIdsRequest,
        },
        repo::ListFilter,
        service::{self, Caller},
    },
};

permissions! {
    TeamCreate => "team.create",
    TeamList => "team.list",
    // Laravel checked `team.view`, which no catalog entry grants.
    TeamGet => "team.get",
    TeamUpdate => "team.update",
    // Laravel checked `invite.delete`.
    TeamDelete => "team.delete",
    TeamInvite => "team.invite",
    TeamPermissions => "team.permissions",
}

/// `{workspace}` + `{team}` path segments; the team stays raw so a malformed id is a 404.
type TeamPath = Path<(String, String)>;

fn caller(access: &WorkspaceAccess) -> Caller {
    Caller {
        is_owner: access.is_owner(),
        is_admin: access.is_admin(),
    }
}

/// POST /users/teams -> 201 `{data:{team_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/users/teams", tag = "teams",
    operation_id = "teams_create", summary = "Create a team",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateTeamRequest,
    responses(
        (status = 201, body = Envelope<TeamCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `team.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<TeamCreate>,
    ValidatedJson(input): ValidatedJson<CreateTeamRequest>,
) -> Result<Response, AppError> {
    let created = service::create(&state, access.workspace_id, caller(&access), input).await?;
    let receipt = created.permission_changes.clone();
    let mut response = (StatusCode::CREATED, Data(created)).into_response();
    response.extensions_mut().insert(receipt);
    Ok(response)
}

/// GET /users/teams -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/users/teams", tag = "teams",
    operation_id = "teams_list",
    summary = "List teams (roles such as Managers) and the permissions each grants",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<TeamResource>, description = "One page of teams, each with its members"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `team.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<TeamList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<TeamResource>>, AppError> {
    let pattern = query.search()?.as_deref().map(contains_pattern);
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = ListFilter {
        pattern: pattern.as_deref(),
        sort: query.sort.as_deref().unwrap_or(""),
        limit: page.size,
        offset: page.offset(),
    };
    let found = service::list(&state, access.workspace_id, &filter).await?;
    let data = TeamResource::page(found.teams, found.users);
    Ok(Json(Paginated::new(data, found.total, page)))
}

/// GET /users/teams/{team}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/users/teams/{team}", tag = "teams",
    operation_id = "teams_get", summary = "Get a team",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("team" = Uuid, Path, description = "Team id"),
    ),
    responses(
        (status = 200, body = Envelope<TeamResource>, description = "The team, with its members"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `team.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or team, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<TeamGet>,
    Path((_, team)): TeamPath,
) -> Result<Data<TeamResource>, AppError> {
    let view = service::get(&state, access.workspace_id, &team).await?;
    let mut page = TeamResource::page(vec![view.team], view.users);
    page.pop().map(Data).ok_or(AppError::NotFound)
}

/// POST /users/teams/{team} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/users/teams/{team}", tag = "teams",
    operation_id = "teams_update", summary = "Update a team (send only changed fields)",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("team" = Uuid, Path, description = "Team id"),
    ),
    request_body = UpdateTeamRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `team.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or team, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<TeamUpdate>,
    Path((_, team)): TeamPath,
    ValidatedJson(patch): ValidatedJson<UpdateTeamRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &team, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /users/teams/{team} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/users/teams/{team}", tag = "teams",
    operation_id = "teams_delete", summary = "Delete a team",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("team" = Uuid, Path, description = "Team id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `team.delete`, or an admin team changed by a non-admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or team, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<TeamDelete>,
    Path((_, team)): TeamPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &team, caller(&access)).await?;
    Ok(Message("ok"))
}

/// POST /users/teams/{team}/add-users -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/users/teams/{team}/add-users", tag = "teams",
    operation_id = "teams_add_users", summary = "Add members to a team",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("team" = Uuid, Path, description = "Team id"),
    ),
    request_body = UserIdsRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `team.invite`, or an admin team changed by a non-admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or team, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or a user is not a workspace member"),
    ),
    security(("bearer" = []))
)]
pub async fn add_users(
    State(state): State<AppState>,
    access: Can<TeamInvite>,
    Path((_, team)): TeamPath,
    ValidatedJson(ids): ValidatedJson<UserIdsRequest>,
) -> Result<Message, AppError> {
    service::add_users(&state, access.workspace_id, &team, caller(&access), ids).await?;
    Ok(Message("ok"))
}

/// POST /users/teams/{team}/remove-users -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/users/teams/{team}/remove-users", tag = "teams",
    operation_id = "teams_remove_users", summary = "Remove members from a team",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("team" = Uuid, Path, description = "Team id"),
    ),
    request_body = UserIdsRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `team.invite`, or an admin team changed by a non-admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or team, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or a user is not in the team"),
    ),
    security(("bearer" = []))
)]
pub async fn remove_users(
    State(state): State<AppState>,
    access: Can<TeamInvite>,
    Path((_, team)): TeamPath,
    ValidatedJson(ids): ValidatedJson<UserIdsRequest>,
) -> Result<Message, AppError> {
    service::remove_users(&state, access.workspace_id, &team, caller(&access), ids).await?;
    Ok(Message("ok"))
}

/// POST /users/teams/{team}/permissions -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/users/teams/{team}/permissions", tag = "teams",
    operation_id = "teams_set_permissions", summary = "Grant or revoke team permissions",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("team" = Uuid, Path, description = "Team id"),
    ),
    request_body = PermissionsRequest,
    responses(
        (status = 200, body = PermissionChangeReply, description = "Processed; receipt identifies unapplied or ignored changes"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `team.permissions`, or a non-admin removing `workspace_admin`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or team, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn permissions(
    State(state): State<AppState>,
    access: Can<TeamPermissions>,
    Path((_, team)): TeamPath,
    ValidatedJson(entries): ValidatedJson<PermissionsRequest>,
) -> Result<Response, AppError> {
    let permission_changes =
        service::set_permissions(&state, access.workspace_id, &team, caller(&access), entries)
            .await?;
    let receipt = permission_changes.clone();
    let mut response = Json(PermissionChangeReply {
        message: "ok",
        permission_changes,
    })
    .into_response();
    response.extensions_mut().insert(receipt);
    Ok(response)
}
