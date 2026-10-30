//! HTTP edge for /v1/workspace/{workspace}/users/teams: permission (via `Can`), call service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, contains_pattern},
        response::{Data, Message},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::teams::{
        dto::{
            CreateTeamRequest, ListQuery, PermissionsRequest, TeamCreated, TeamResource,
            UpdateTeamRequest, UserIdsRequest,
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
pub async fn create(
    State(state): State<AppState>,
    access: Can<TeamCreate>,
    ValidatedJson(input): ValidatedJson<CreateTeamRequest>,
) -> Result<(StatusCode, Data<TeamCreated>), AppError> {
    let team_id = service::create(&state, access.workspace_id, caller(&access), input).await?;
    Ok((StatusCode::CREATED, Data(TeamCreated { team_id })))
}

/// GET /users/teams -> `{data:[...], meta}`.
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<TeamDelete>,
    Path((_, team)): TeamPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &team, caller(&access)).await?;
    Ok(Message("ok"))
}

/// POST /users/teams/{team}/add-users -> `{"message":"ok"}`.
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
pub async fn permissions(
    State(state): State<AppState>,
    access: Can<TeamPermissions>,
    Path((_, team)): TeamPath,
    ValidatedJson(entries): ValidatedJson<PermissionsRequest>,
) -> Result<Message, AppError> {
    service::set_permissions(&state, access.workspace_id, &team, caller(&access), entries).await?;
    Ok(Message("ok"))
}
