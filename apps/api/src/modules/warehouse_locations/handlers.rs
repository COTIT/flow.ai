//! HTTP edge for /v1/workspace/{workspace}/erp/warehouses/{warehouse}/locations.
//! Laravel had no catalog names here, so reads take `warehouse.get` and writes
//! `warehouse.update` (user decision, docs/api/warehouses.md).

use std::borrow::Cow;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<LocationWrite>,
    Path((_, wh, id)): LocationPath,
) -> Result<Message, AppError> {
    let warehouse = service::warehouse(&state, access.workspace_id, &wh).await?;
    service::delete(&state, access.workspace_id, warehouse.id, &id).await?;
    Ok(Message("ok"))
}

/// DELETE /locations?locations=a,b -> `{"message":"ok"}` (vimdesk-web sends ids as a query).
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
