//! HTTP edge for /v1/workspace/{workspace}/settings/sales-groups: `salesgroup.*` permission
//! (via `Can`), call service, wrap.

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
        list::{Page, Paginated, search},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::sales_groups::{
        dto::{
            CreateSalesGroupRequest, ListQuery, SalesGroupCreated, SalesGroupResource,
            UpdateSalesGroupRequest,
        },
        service,
    },
};

permissions! {
    SalesGroupCreate => "salesgroup.create",
    SalesGroupList => "salesgroup.list",
    SalesGroupGet => "salesgroup.get",
    SalesGroupUpdate => "salesgroup.update",
    SalesGroupDelete => "salesgroup.delete",
}

/// `{workspace}` + `{sales_group}`; the id stays raw so a malformed one is a 404.
type GroupPath = Path<(String, String)>;

/// GET /settings/sales-groups -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<SalesGroupList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<SalesGroupResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        pattern.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /settings/sales-groups/{sales_group}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<SalesGroupGet>,
    Path((_, id)): GroupPath,
) -> Result<Data<SalesGroupResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /settings/sales-groups -> 201 `{data:{sales_group_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<SalesGroupCreate>,
    ValidatedJson(input): ValidatedJson<CreateSalesGroupRequest>,
) -> Result<(StatusCode, Data<SalesGroupCreated>), AppError> {
    let sales_group_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(SalesGroupCreated { sales_group_id }),
    ))
}

/// POST /settings/sales-groups/{sales_group} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<SalesGroupUpdate>,
    Path((_, id)): GroupPath,
    ValidatedJson(patch): ValidatedJson<UpdateSalesGroupRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /settings/sales-groups/{sales_group} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<SalesGroupDelete>,
    Path((_, id)): GroupPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
