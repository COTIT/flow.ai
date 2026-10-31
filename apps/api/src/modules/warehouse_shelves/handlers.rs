//! HTTP edge for .../sections/{section}/rows/{row}/shelves. Reads take `warehouse.get`,
//! writes `warehouse.update`.
// Laravel's shelf update even checked `warehouse.update`.

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
    modules::warehouse_shelves::{
        dto::{CreateShelfRequest, ShelfBody, ShelfCreated, ShelfResource, UpdateShelfRequest},
        service,
    },
};

permissions! {
    HierarchyRead => "warehouse.get",
    HierarchyWrite => "warehouse.update",
}

type RowPath = Path<(String, String, String, String)>;
type ShelfPath = Path<(String, String, String, String, String)>;

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

/// GET /shelves -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves", tag = "warehouse_shelves",
    operation_id = "warehouse_shelves_list", summary = "List shelfs",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ListQuery),
    responses(
        (status = 200, body = Paginated<ShelfResource>, description = "One page of shelfs"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section or row, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, r)): RowPath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<ShelfResource>>, AppError> {
    let (_, row) = service::row(&state, access.workspace_id, (&wh, &s, &r)).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        row.id,
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /shelves/{shelf}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}", tag = "warehouse_shelves",
    operation_id = "warehouse_shelves_get", summary = "Get a shelf",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ("shelf" = Uuid, Path, description = "Shelf id")),
    responses(
        (status = 200, body = Envelope<ShelfResource>, description = "The shelf"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section, row or shelf, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, r, id)): ShelfPath,
) -> Result<Data<ShelfResource>, AppError> {
    let shelf = service::find(&state, access.workspace_id, (&wh, &s, &r), &id).await?;
    Ok(Data(shelf.into()))
}

/// POST /shelves -> 201 `{data:{shelf_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves", tag = "warehouse_shelves",
    operation_id = "warehouse_shelves_create", summary = "Create a shelf",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id")),
    request_body = ShelfBody,
    responses(
        (status = 201, body = Envelope<ShelfCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section or row, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r)): RowPath,
    ValidatedJson(fields): ValidatedJson<CreateShelfRequest>,
) -> Result<(StatusCode, Data<ShelfCreated>), AppError> {
    let (section, row) = service::row(&state, access.workspace_id, (&wh, &s, &r)).await?;
    let shelf_id = service::create(&state, access.workspace_id, &section, &row, fields).await?;
    Ok((StatusCode::CREATED, Data(ShelfCreated { shelf_id })))
}

/// POST /shelves/{shelf} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}", tag = "warehouse_shelves",
    operation_id = "warehouse_shelves_update", summary = "Update a shelf",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ("shelf" = Uuid, Path, description = "Shelf id")),
    request_body = ShelfBody,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section, row or shelf, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, id)): ShelfPath,
    ValidatedJson(fields): ValidatedJson<UpdateShelfRequest>,
) -> Result<Message, AppError> {
    let (_, row) = service::row(&state, access.workspace_id, (&wh, &s, &r)).await?;
    service::update(&state, access.workspace_id, row.id, &id, fields).await?;
    Ok(Message("ok"))
}

/// DELETE /shelves/{shelf} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}", tag = "warehouse_shelves",
    operation_id = "warehouse_shelves_delete", summary = "Delete a shelf",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id"), ("shelf" = Uuid, Path, description = "Shelf id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section, row or shelf, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, id)): ShelfPath,
) -> Result<Message, AppError> {
    let (_, row) = service::row(&state, access.workspace_id, (&wh, &s, &r)).await?;
    service::delete(&state, access.workspace_id, row.id, &id).await?;
    Ok(Message("ok"))
}
