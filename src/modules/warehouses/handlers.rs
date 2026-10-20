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
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, flag},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::warehouses::{
        dto::{
            BAD_PARENT, CreateWarehouseRequest, ListQuery, UpdateWarehouseRequest,
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
    let data = WarehouseResource::build(p.rows, p.children, &p.people);
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
pub async fn get(
    State(state): State<AppState>,
    access: Can<WarehouseGet>,
    Path((_, id)): WarehousePath,
) -> Result<Data<WarehouseResource>, AppError> {
    let p = service::get(&state, access.workspace_id, &id).await?;
    WarehouseResource::build(p.rows, p.children, &p.people)
        .pop()
        .map(Data)
        .ok_or(AppError::NotFound)
}

/// POST /warehouses -> 201 `{data:{warehouse_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<WarehouseCreate>,
    ValidatedJson(input): ValidatedJson<CreateWarehouseRequest>,
) -> Result<(StatusCode, Data<WarehouseCreated>), AppError> {
    let warehouse_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(WarehouseCreated { warehouse_id })))
}

/// POST /warehouses/{warehouse} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<WarehouseUpdate>,
    Path((_, id)): WarehousePath,
    ValidatedJson(patch): ValidatedJson<UpdateWarehouseRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// POST /warehouses/{warehouse}/default -> `{"message":"ok"}`.
pub async fn set_default(
    State(state): State<AppState>,
    access: Can<WarehouseSetDefault>,
    Path((_, id)): WarehousePath,
) -> Result<Message, AppError> {
    service::set_default(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// DELETE /warehouses/{warehouse} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<WarehouseDelete>,
    Path((_, id)): WarehousePath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
