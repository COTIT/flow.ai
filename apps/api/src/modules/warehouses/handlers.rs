//! HTTP edge for /v1/workspace/{workspace}/warehouses: `warehouse.*` permission (via `Can`),
//! call service, wrap.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, flag},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, Permission, permissions},
    },
    modules::warehouses::{
        dto::{
            BAD_PARENT, CreateWarehouseRequest, ListQuery, UpdateWarehouseRequest, WarehouseBody,
            WarehouseCreated, WarehouseResource,
        },
        repo::ListFilter,
        service,
    },
};

// Laravel checked `warehouse.view` for one row; genesy uses the catalog's `get`.
permissions! {
    WarehouseCreate => "warehouse.create",
    WarehouseList => "warehouse.list",
    WarehouseGet => "warehouse.get",
    WarehouseUpdate => "warehouse.update",
    WarehouseDelete => "warehouse.delete",
    WarehouseSetDefault => "warehouse.set_default",
}

/// `{workspace}` + `{warehouse}`; the id stays raw so a malformed one is a 404.
type WarehousePath = Path<(String, String)>;

/// GET /warehouses -> `{data:[...], meta}`. Roots only unless `parent_id` or `roots_only=0`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses", tag = "warehouses",
    operation_id = "warehouses_list", summary = "List warehouses",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<WarehouseResource>, description = "One page of warehouses"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Malformed `parent_id`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<WarehouseList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<WarehouseResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = ListFilter {
        parent_id: list_parent(query.parent_id.as_deref())?,
        roots_only: query.roots_only.as_deref().is_none_or(flag),
        pickup_only: query.pickup_enabled.as_deref().is_some_and(flag),
        limit: page.size,
        offset: page.offset(),
    };
    let p = service::list(&state, access.workspace_id, &filter).await?;
    let data = WarehouseResource::build(p.rows, p.children, &p.people, &p.occupied);
    Ok(Json(Paginated::new(data, p.total, page)))
}

/// `parent_id` query: blank = none, malformed = Laravel's `uuid` + `ValidateId` 422.
fn list_parent(raw: Option<&str>) -> Result<Option<Uuid>, AppError> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    Uuid::parse_str(raw).map(Some).map_err(|_| {
        AppError::Validation(BTreeMap::from([(
            "parent_id".to_owned(),
            vec![
                "The parent id must be a valid UUID.".to_owned(),
                BAD_PARENT.to_owned(),
            ],
        )]))
    })
}

/// GET /warehouses/{warehouse}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}", tag = "warehouses",
    operation_id = "warehouses_get", summary = "Get a warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id")),
    responses(
        (status = 200, body = Envelope<WarehouseResource>, description = "The warehouse"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<WarehouseGet>,
    Path((_, id)): WarehousePath,
) -> Result<Data<WarehouseResource>, AppError> {
    let p = service::get(&state, access.workspace_id, &id).await?;
    WarehouseResource::build(p.rows, p.children, &p.people, &p.occupied)
        .pop()
        .map(Data)
        .ok_or(AppError::NotFound)
}

/// POST /warehouses -> 201 `{data:{warehouse_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses", tag = "warehouses",
    operation_id = "warehouses_create", summary = "Create a warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = WarehouseBody,
    responses(
        (status = 201, body = Envelope<WarehouseCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `warehouse.create`, or `default_warehouse` without `warehouse.set_default`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<WarehouseCreate>,
    ValidatedJson(input): ValidatedJson<CreateWarehouseRequest>,
) -> Result<(StatusCode, Data<WarehouseCreated>), AppError> {
    let may = access.allows(WarehouseSetDefault::NAME);
    let warehouse_id = service::create(&state, access.workspace_id, input, may).await?;
    Ok((StatusCode::CREATED, Data(WarehouseCreated { warehouse_id })))
}

/// POST /warehouses/{warehouse} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}", tag = "warehouses",
    operation_id = "warehouses_update", summary = "Update a warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id")),
    request_body = WarehouseBody,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `warehouse.update`, or `default_warehouse` without `warehouse.set_default`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<WarehouseUpdate>,
    Path((_, id)): WarehousePath,
    ValidatedJson(patch): ValidatedJson<UpdateWarehouseRequest>,
) -> Result<Message, AppError> {
    let may = access.allows(WarehouseSetDefault::NAME);
    service::update(&state, access.workspace_id, &id, patch, may).await?;
    Ok(Message("ok"))
}

/// POST /warehouses/{warehouse}/default -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/default", tag = "warehouses",
    operation_id = "warehouses_set_default", summary = "Make a warehouse the default",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.set_default`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn set_default(
    State(state): State<AppState>,
    access: Can<WarehouseSetDefault>,
    Path((_, id)): WarehousePath,
) -> Result<Message, AppError> {
    service::set_default(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// DELETE /warehouses/{warehouse} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/warehouses/{warehouse}", tag = "warehouses",
    operation_id = "warehouses_delete", summary = "Delete a warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<WarehouseDelete>,
    Path((_, id)): WarehousePath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
