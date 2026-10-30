//! HTTP edge for /v1/me and /v1/workspace/{workspace}/users.

use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::{
    AppState,
    error::AppError,
    http::{
        auth::AuthUser,
        list::{Page, Paginated, contains_pattern},
        response::Data,
        workspace::{Can, permissions},
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
pub async fn member(
    State(state): State<AppState>,
    access: Can<UserGet>,
    Path((_, user)): Path<(String, String)>,
) -> Result<Data<UserDetail>, AppError> {
    let view = service::member(&state, access.workspace_id, &user).await?;
    Ok(Data(UserDetail::new(view, access.workspace_id)))
}

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
