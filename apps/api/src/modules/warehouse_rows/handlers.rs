//! HTTP edge for .../warehouses/{warehouse}/sections/{section}/rows. Reads take
//! `warehouse.get`, writes `warehouse.update`.
// No Laravel catalog names here.

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
    modules::warehouse_rows::{
        dto::{CreateRowRequest, RowBody, RowCreated, RowResource, UpdateRowRequest},
        service,
    },
};

permissions! {
    HierarchyRead => "warehouse.get",
    HierarchyWrite => "warehouse.update",
}

type SectionPath = Path<(String, String, String)>;
type RowPath = Path<(String, String, String, String)>;

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

/// GET /rows -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows", tag = "warehouse_rows",
    operation_id = "warehouse_rows_list", summary = "List rows of a section",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ListQuery),
    responses(
        (status = 200, body = Paginated<RowResource>, description = "One page of rows"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse or section, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s)): SectionPath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<RowResource>>, AppError> {
    let section = service::section(&state, access.workspace_id, &wh, &s).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        section.id,
        page.size,
        page.offset(),
    )
    .await?;
    let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
    let occupied = service::occupied(&state, access.workspace_id, &ids).await?;
    let data = rows
        .into_iter()
        .map(|r| {
            let n = occupied.get(&r.id).copied().unwrap_or_default();
            RowResource::new(r, access.workspace_id, &section, n)
        })
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /rows/{row}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}", tag = "warehouse_rows",
    operation_id = "warehouse_rows_get", summary = "Get a row",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id")),
    responses(
        (status = 200, body = Envelope<RowResource>, description = "The row"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section or row, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, id)): RowPath,
) -> Result<Data<RowResource>, AppError> {
    let (section, row) = service::find(&state, access.workspace_id, (&wh, &s, &id)).await?;
    let occupied = service::occupied(&state, access.workspace_id, &[row.id]).await?;
    let n = occupied.get(&row.id).copied().unwrap_or_default();
    Ok(Data(RowResource::new(
        row,
        access.workspace_id,
        &section,
        n,
    )))
}

/// POST /rows -> 201 `{data:{row_id, warehouse_section_row_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows", tag = "warehouse_rows",
    operation_id = "warehouse_rows_create", summary = "Create a row",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id")),
    request_body = RowBody,
    responses(
        (status = 201, body = Envelope<RowCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse or section, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s)): SectionPath,
    ValidatedJson(fields): ValidatedJson<CreateRowRequest>,
) -> Result<(StatusCode, Data<RowCreated>), AppError> {
    let section = service::section(&state, access.workspace_id, &wh, &s).await?;
    let id = service::create(&state, access.workspace_id, &section, fields).await?;
    Ok((
        StatusCode::CREATED,
        Data(RowCreated {
            row_id: id,
            warehouse_section_row_id: id,
        }),
    ))
}

/// POST /rows/{row} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}", tag = "warehouse_rows",
    operation_id = "warehouse_rows_update", summary = "Update a row",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id")),
    request_body = RowBody,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section or row, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, id)): RowPath,
    ValidatedJson(fields): ValidatedJson<UpdateRowRequest>,
) -> Result<Message, AppError> {
    let section = service::section(&state, access.workspace_id, &wh, &s).await?;
    service::update(&state, access.workspace_id, &section, &id, fields).await?;
    Ok(Message("ok"))
}

/// DELETE /rows/{row} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}", tag = "warehouse_rows",
    operation_id = "warehouse_rows_delete", summary = "Delete a row",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id"), ("row" = Uuid, Path, description = "Row id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse, section or row, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, id)): RowPath,
) -> Result<Message, AppError> {
    let section = service::section(&state, access.workspace_id, &wh, &s).await?;
    service::delete(&state, access.workspace_id, &section, &id).await?;
    Ok(Message("ok"))
}
