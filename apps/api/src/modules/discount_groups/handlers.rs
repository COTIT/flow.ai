//! HTTP edge for /v1/workspace/{workspace}/products/discount-groups (`discountgroup.*`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, flag, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::discount_groups::{
        dto::{
            CreateGroupRequest, GroupCreated, GroupListBody, GroupResource, ListQuery,
            RelationIdsRequest, UpdateGroupRequest,
        },
        repo::ListFilter,
        service,
    },
};

permissions! {
    GroupList => "discountgroup.list",
    GroupView => "discountgroup.view",
    GroupCreate => "discountgroup.create",
    GroupUpdate => "discountgroup.update",
    GroupDelete => "discountgroup.delete",
    GroupAttach => "discountgroup.attach",
    GroupDetach => "discountgroup.detach",
}

type GroupPath = Path<(String, String)>;

/// `limit` (0..100): a plain list without paging.
const MAX_LIMIT: i64 = 100;

/// GET /discount-groups -> page, or `{data:[...]}` with `?limit=`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/discount-groups", tag = "discount_groups",
    operation_id = "discount_groups_list", summary = "List discount groups",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = GroupListBody, description = "A page, or a plain list with `limit`"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discountgroup.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long or bad `limit`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<GroupList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let pattern = search(q.q.as_deref())?;
    let filter = ListFilter {
        pattern: pattern.as_deref(),
        active: q.active.as_deref().map(flag),
        internal: q.internal.as_deref().map(flag),
    };
    let limit = match q.limit.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        None => None,
        Some(raw) => match raw.parse::<i64>() {
            Ok(n) if (0..=MAX_LIMIT).contains(&n) => (n > 0).then_some(n),
            Ok(n) if n < 0 => {
                return Err(AppError::field("limit", "The limit must be at least 0."));
            }
            Ok(_) => {
                return Err(AppError::field(
                    "limit",
                    "The limit must not be greater than 100.",
                ));
            }
            Err(_) => return Err(AppError::field("limit", "The limit must be an integer.")),
        },
    };
    if let Some(n) = limit {
        let (rows, _) = service::list(&state, access.workspace_id, &filter, n, 0).await?;
        let data = service::render(&state, access.workspace_id, rows).await?;
        return Ok(Json(json!({ "data": data })));
    }
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        &filter,
        page.size,
        page.offset(),
    )
    .await?;
    let data = service::render(&state, access.workspace_id, rows).await?;
    Ok(Json(
        serde_json::to_value(Paginated::new(data, total, page))
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

/// GET /discount-groups/{id}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/discount-groups/{group}", tag = "discount_groups",
    operation_id = "discount_groups_get", summary = "Get a discount group",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("group" = Uuid, Path, description = "Discount group id")),
    responses(
        (status = 200, body = Envelope<GroupResource>, description = "The discount group"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discountgroup.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount group, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<GroupView>,
    Path((_, id)): GroupPath,
) -> Result<Data<GroupResource>, AppError> {
    let row = service::get(&state, access.workspace_id, &id).await?;
    let mut rendered = service::render(&state, access.workspace_id, vec![row]).await?;
    rendered.pop().map(Data).ok_or(AppError::NotFound)
}

/// POST /discount-groups -> 201 `{data:{discount_group_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/discount-groups", tag = "discount_groups",
    operation_id = "discount_groups_create", summary = "Create a discount group",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateGroupRequest,
    responses(
        (status = 201, body = Envelope<GroupCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discountgroup.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<GroupCreate>,
    ValidatedJson(input): ValidatedJson<CreateGroupRequest>,
) -> Result<(StatusCode, Data<GroupCreated>), AppError> {
    let discount_group_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(GroupCreated { discount_group_id }),
    ))
}

/// POST /discount-groups/{id}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/discount-groups/{group}", tag = "discount_groups",
    operation_id = "discount_groups_update", summary = "Update a discount group (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("group" = Uuid, Path, description = "Discount group id")),
    request_body = UpdateGroupRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discountgroup.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount group, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<GroupUpdate>,
    Path((_, id)): GroupPath,
    ValidatedJson(patch): ValidatedJson<UpdateGroupRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /discount-groups/{id}.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/products/discount-groups/{group}", tag = "discount_groups",
    operation_id = "discount_groups_delete", summary = "Delete a discount group",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("group" = Uuid, Path, description = "Discount group id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discountgroup.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount group, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<GroupDelete>,
    Path((_, id)): GroupPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// Paging params of the relations list.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    page_size: Option<String>,
}

/// GET /discount-groups/{id}/relations -> page of relations (relation index shape).
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/discount-groups/{group}/relations", tag = "discount_groups",
    operation_id = "discount_groups_relations", summary = "List relations linked to a discount group",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("group" = Uuid, Path, description = "Discount group id"), PageQuery),
    responses(
        (status = 200, body = Paginated<Value>, description = "One page of relations (relation index shape)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discountgroup.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount group, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn relations(
    State(state): State<AppState>,
    access: Can<GroupView>,
    Path((_, id)): GroupPath,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (data, total) =
        service::relations_page(&state, access.workspace_id, &id, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /discount-groups/{id}/attach-to-relations.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/discount-groups/{group}/attach-to-relations", tag = "discount_groups",
    operation_id = "discount_groups_attach", summary = "Link relations to a discount group",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("group" = Uuid, Path, description = "Discount group id")),
    request_body = RelationIdsRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discountgroup.attach`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount group, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn attach(
    State(state): State<AppState>,
    access: Can<GroupAttach>,
    Path((_, id)): GroupPath,
    ValidatedJson(ids): ValidatedJson<RelationIdsRequest>,
) -> Result<Message, AppError> {
    service::link(&state, access.workspace_id, &id, &ids, true).await?;
    Ok(Message("ok"))
}

/// POST /discount-groups/{id}/detach-from-relations.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/discount-groups/{group}/detach-from-relations", tag = "discount_groups",
    operation_id = "discount_groups_detach", summary = "Unlink relations from a discount group",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("group" = Uuid, Path, description = "Discount group id")),
    request_body = RelationIdsRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discountgroup.detach`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount group, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn detach(
    State(state): State<AppState>,
    access: Can<GroupDetach>,
    Path((_, id)): GroupPath,
    ValidatedJson(ids): ValidatedJson<RelationIdsRequest>,
) -> Result<Message, AppError> {
    service::link(&state, access.workspace_id, &id, &ids, false).await?;
    Ok(Message("ok"))
}
