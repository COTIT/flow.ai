//! HTTP edge for .../shelves/{shelf}/bins and GET /warehouses/{warehouse}/bins. Reads take
//! `warehouse.get`, writes `warehouse.update`.
// No Laravel catalog names for bins.

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
        list::{Page, Paginated},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::warehouse_bins::{
        dto::{
            BinBody, BinCreated, BinHierarchyResource, BinResource, CreateBinRequest,
            UpdateBinRequest,
        },
        service,
    },
};

permissions! {
    HierarchyRead => "warehouse.get",
    HierarchyWrite => "warehouse.update",
}

type ShelfPath = Path<(String, String, String, String, String)>;
type BinPath = Path<(String, String, String, String, String, String)>;

/// Paging params; unknown ones ignored.
#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    page_size: Option<String>,
}

/// GET /bins -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}/bins", tag = "warehouse_bins",
    operation_id = "warehouse_bins_list", summary = "List bins",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ("shelf" = Uuid, Path, description = "Shelf id"), ListQuery),
    responses(
        (status = 200, body = Paginated<BinResource>, description = "One page of bins"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section, row or shelf, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, r, sh)): ShelfPath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<BinResource>>, AppError> {
    let shelf = service::shelf(&state, access.workspace_id, ((&wh, &s, &r), &sh)).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        shelf.id,
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows
        .into_iter()
        .map(|b| BinResource::new(b, false))
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /bins/{bin}, with `stocks`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}/bins/{bin}", tag = "warehouse_bins",
    operation_id = "warehouse_bins_get", summary = "Get a bin",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ("shelf" = Uuid, Path, description = "Shelf id"), ("bin" = Uuid, Path, description = "Bin id")),
    responses(
        (status = 200, body = Envelope<BinResource>, description = "The bin"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section, row, shelf or bin, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, r, sh, id)): BinPath,
) -> Result<Data<BinResource>, AppError> {
    let bin = service::find(&state, access.workspace_id, ((&wh, &s, &r), &sh), &id).await?;
    Ok(Data(BinResource::new(bin, true)))
}

/// GET /warehouses/{warehouse}/bins -> `{data:[...]}` (not paginated), plus `meta.truncated`
/// only when the cap cut the list.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/bins", tag = "warehouse_bins",
    operation_id = "warehouse_bins_by_warehouse", summary = "List every bin of a warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id")),
    responses(
        (status = 200, body = WarehouseBins, description = "Every bin with its path; not paged"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn by_warehouse(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh)): Path<(String, String)>,
) -> Result<Json<WarehouseBins>, AppError> {
    let (rows, truncated) = service::of_warehouse(&state, access.workspace_id, &wh).await?;
    Ok(Json(WarehouseBins {
        data: rows.into_iter().map(Into::into).collect(),
        meta: truncated.then_some(Truncated { truncated: true }),
    }))
}

/// Body of `GET /warehouses/{warehouse}/bins`.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct WarehouseBins {
    data: Vec<BinHierarchyResource>,
    /// Sent only when the list was cut at the cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(inline)]
    meta: Option<Truncated>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct Truncated {
    truncated: bool,
}

/// POST /bins -> 201 `{data:{bin_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}/bins", tag = "warehouse_bins",
    operation_id = "warehouse_bins_create", summary = "Create a bin",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ("shelf" = Uuid, Path, description = "Shelf id")),
    request_body = BinBody,
    responses(
        (status = 201, body = Envelope<BinCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section, row or shelf, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, sh)): ShelfPath,
    ValidatedJson(fields): ValidatedJson<CreateBinRequest>,
) -> Result<(StatusCode, Data<BinCreated>), AppError> {
    let shelf = service::shelf(&state, access.workspace_id, ((&wh, &s, &r), &sh)).await?;
    let bin_id = service::create(&state, &shelf, fields).await?;
    Ok((StatusCode::CREATED, Data(BinCreated { bin_id })))
}

/// POST /bins/{bin} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}/bins/{bin}", tag = "warehouse_bins",
    operation_id = "warehouse_bins_update", summary = "Update a bin",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ("shelf" = Uuid, Path, description = "Shelf id"), ("bin" = Uuid, Path, description = "Bin id")),
    request_body = BinBody,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section, row, shelf or bin, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, sh, id)): BinPath,
    ValidatedJson(fields): ValidatedJson<UpdateBinRequest>,
) -> Result<Message, AppError> {
    let shelf = service::shelf(&state, access.workspace_id, ((&wh, &s, &r), &sh)).await?;
    service::update(&state, access.workspace_id, &shelf, &id, fields).await?;
    Ok(Message("ok"))
}

/// DELETE /bins/{bin} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}/bins/{bin}", tag = "warehouse_bins",
    operation_id = "warehouse_bins_delete", summary = "Delete a bin",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ("shelf" = Uuid, Path, description = "Shelf id"), ("bin" = Uuid, Path, description = "Bin id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section, row, shelf or bin, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, sh, id)): BinPath,
) -> Result<Message, AppError> {
    let shelf = service::shelf(&state, access.workspace_id, ((&wh, &s, &r), &sh)).await?;
    service::delete(&state, access.workspace_id, &shelf, &id).await?;
    Ok(Message("ok"))
}
