//! HTTP edge for /v1/workspace/{workspace}/erp/warehouses/{warehouse}/locations. Reads take
//! `warehouse.get`, writes `warehouse.update`.
// Laravel had no catalog names here (user decision, docs/api/warehouses.md).

use std::borrow::Cow;

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
        list::{Page, Paginated, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::{
        warehouse_locations::{
            dto::{DeleteQuery, ListQuery, LocationBody, LocationCreated, LocationResource},
            service,
        },
        warehouses::dto::WarehouseCompacted,
    },
};

permissions! {
    LocationRead => "warehouse.get",
    LocationWrite => "warehouse.update",
}

/// Largest `limit`: inventory reconciliation loads a warehouse's labels in one call (5000).
const LIMIT_MAX: i64 = 5000;

/// `{workspace}` + `{warehouse}`.
type WarehousePath = Path<(String, String)>;
/// `{workspace}` + `{warehouse}` + `{location}`.
type LocationPath = Path<(String, String, String)>;

/// GET /locations -> `{data:[...], meta}`; `q` matches the label, `limit` sets the page size.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/warehouses/{warehouse}/locations", tag = "warehouse_locations",
    operation_id = "warehouse_locations_list", summary = "List a warehouse's scan labels",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ListQuery),
    responses(
        (status = 200, body = Paginated<LocationResource>, description = "One page of locations"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<LocationRead>,
    Path((_, wh)): WarehousePath,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let warehouse = service::warehouse(&state, access.workspace_id, &wh).await?;
    let pattern = search(query.q.as_deref())?;
    let mut page = Page::parse(query.page.as_deref(), None);
    if let Some(n) = query
        .limit
        .as_deref()
        .and_then(|s| s.trim().parse::<i64>().ok())
    {
        page.size = n.clamp(1, LIMIT_MAX);
    }
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        warehouse.id,
        pattern.as_deref(),
        page,
    )
    .await?;
    let compact = WarehouseCompacted::from(&warehouse);
    let data = rows
        .into_iter()
        .map(|l| LocationResource::new(l, Cow::Borrowed(&compact)))
        .collect();
    Ok(Json(Paginated::new(data, total, page)).into_response())
}

/// GET /locations/{location}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/warehouses/{warehouse}/locations/{location}", tag = "warehouse_locations",
    operation_id = "warehouse_locations_get", summary = "Get a scan label",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("location" = Uuid, Path, description = "Location id")),
    responses(
        (status = 200, body = Envelope<LocationResource>, description = "The location"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse or location, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<LocationRead>,
    Path((_, wh, id)): LocationPath,
) -> Result<Response, AppError> {
    let warehouse = service::warehouse(&state, access.workspace_id, &wh).await?;
    let row = service::get(&state, access.workspace_id, warehouse.id, &id).await?;
    let compact = WarehouseCompacted::from(&warehouse);
    Ok(Data(LocationResource::new(row, Cow::Owned(compact))).into_response())
}

/// POST /locations -> 201 `{data:{warehouse_location_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/warehouses/{warehouse}/locations", tag = "warehouse_locations",
    operation_id = "warehouse_locations_create", summary = "Create a scan label",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id")),
    request_body = LocationBody,
    responses(
        (status = 201, body = Envelope<LocationCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<LocationWrite>,
    Path((_, wh)): WarehousePath,
    ValidatedJson(input): ValidatedJson<LocationBody>,
) -> Result<(StatusCode, Data<LocationCreated>), AppError> {
    let warehouse = service::warehouse(&state, access.workspace_id, &wh).await?;
    let warehouse_location_id =
        service::create(&state, access.workspace_id, warehouse.id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(LocationCreated {
            warehouse_location_id,
        }),
    ))
}

/// POST /locations/{location} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/warehouses/{warehouse}/locations/{location}", tag = "warehouse_locations",
    operation_id = "warehouse_locations_update", summary = "Update a scan label",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("location" = Uuid, Path, description = "Location id")),
    request_body = LocationBody,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse or location, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<LocationWrite>,
    Path((_, wh, id)): LocationPath,
    ValidatedJson(input): ValidatedJson<LocationBody>,
) -> Result<Message, AppError> {
    let warehouse = service::warehouse(&state, access.workspace_id, &wh).await?;
    service::update(&state, access.workspace_id, warehouse.id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /locations/{location} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/erp/warehouses/{warehouse}/locations/{location}", tag = "warehouse_locations",
    operation_id = "warehouse_locations_delete", summary = "Delete a scan label",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("location" = Uuid, Path, description = "Location id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse or location, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<LocationWrite>,
    Path((_, wh, id)): LocationPath,
) -> Result<Message, AppError> {
    let warehouse = service::warehouse(&state, access.workspace_id, &wh).await?;
    service::delete(&state, access.workspace_id, warehouse.id, &id).await?;
    Ok(Message("ok"))
}

/// DELETE /locations?locations=a,b -> `{"message":"ok"}`.
// vimdesk-web sends ids as a query.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/erp/warehouses/{warehouse}/locations", tag = "warehouse_locations",
    operation_id = "warehouse_locations_delete_many", summary = "Delete several scan labels",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), DeleteQuery),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Missing, malformed or too many ids"),
    ),
    security(("bearer" = []))
)]
pub async fn delete_many(
    State(state): State<AppState>,
    access: Can<LocationWrite>,
    Path((_, wh)): WarehousePath,
    Query(query): Query<DeleteQuery>,
) -> Result<Message, AppError> {
    let warehouse = service::warehouse(&state, access.workspace_id, &wh).await?;
    let ids = query.ids()?;
    service::delete_many(&state, access.workspace_id, warehouse.id, &ids).await?;
    Ok(Message("ok"))
}
