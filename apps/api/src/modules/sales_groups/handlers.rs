//! HTTP edge for /v1/workspace/{workspace}/settings/sales-groups: `salesgroup.*` permission
//! (via `Can`), call service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, search},
        response::{Data, Envelope, Message, MessageBody},
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
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/sales-groups", tag = "sales_groups",
    operation_id = "sales_groups_list", summary = "List sales groups",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<SalesGroupResource>, description = "One page of sales groups"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `salesgroup.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/sales-groups/{sales_group}", tag = "sales_groups",
    operation_id = "sales_groups_get", summary = "Get a sales group",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("sales_group" = Uuid, Path, description = "Sales group id"),
    ),
    responses(
        (status = 200, body = Envelope<SalesGroupResource>, description = "The sales group"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `salesgroup.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or sales group, or caller not a member"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings/sales-groups", tag = "sales_groups",
    operation_id = "sales_groups_create", summary = "Create a sales group",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateSalesGroupRequest,
    responses(
        (status = 201, body = Envelope<SalesGroupCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `salesgroup.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `name` already taken"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings/sales-groups/{sales_group}", tag = "sales_groups",
    operation_id = "sales_groups_update", summary = "Update a sales group (`name` always required)",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("sales_group" = Uuid, Path, description = "Sales group id"),
    ),
    request_body = UpdateSalesGroupRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `salesgroup.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or sales group, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `name` already taken"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    delete, path = "/workspace/{workspace}/settings/sales-groups/{sales_group}", tag = "sales_groups",
    operation_id = "sales_groups_delete", summary = "Delete a sales group",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("sales_group" = Uuid, Path, description = "Sales group id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `salesgroup.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or sales group, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<SalesGroupDelete>,
    Path((_, id)): GroupPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
