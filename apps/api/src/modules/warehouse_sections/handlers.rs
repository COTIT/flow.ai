//! HTTP edge for /v1/workspace/{workspace}/warehouses/{warehouse}/sections. Reads take
//! `warehouse.get`, writes `warehouse.update`.
// Laravel had no catalog names for the hierarchy (user decision, docs/api/warehouses.md).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::{Rules, ValidatedJson},
        list::{Paginated, contains_pattern, flag},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::warehouse_sections::{
        dto::{ListQuery, SectionBody, SectionCreated, SectionResource},
        repo::ListFilter,
        service,
    },
};

permissions! {
    HierarchyRead => "warehouse.get",
    HierarchyWrite => "warehouse.update",
}

type WarehousePath = Path<(String, String)>;
type SectionPath = Path<(String, String, String)>;

/// GET /sections -> `{data:[...], meta}`, sorted by name.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/sections", tag = "warehouse_sections",
    operation_id = "warehouse_sections_list", summary = "List sections of a warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ListQuery),
    responses(
        (status = 200, body = Paginated<SectionResource>, description = "One page of sections, by name"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh)): WarehousePath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<SectionResource>>, AppError> {
    let warehouse_id = service::warehouse(&state, access.workspace_id, &wh).await?;
    let page = q.page();
    let search = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(s) = search {
        let mut r = Rules::default();
        r.max_chars("search", s, 255);
        r.into_result()?;
    }
    let pattern = search.map(contains_pattern);
    let filter = ListFilter {
        category: blank(&q.category),
        zone: blank(&q.zone),
        priority_level: blank(&q.priority_level),
        restricted_access: blank(&q.restricted_access).map(flag),
        search: pattern.as_deref(),
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, warehouse_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /sections/{section}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}", tag = "warehouse_sections",
    operation_id = "warehouse_sections_get", summary = "Get a section",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id")),
    responses(
        (status = 200, body = Envelope<SectionResource>, description = "The section"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse or section, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, id)): SectionPath,
) -> Result<Data<SectionResource>, AppError> {
    Ok(Data(
        service::find(&state, access.workspace_id, &wh, &id)
            .await?
            .into(),
    ))
}

/// POST /sections -> 201 `{data:{section_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/sections", tag = "warehouse_sections",
    operation_id = "warehouse_sections_create", summary = "Create a section (and its row/shelf/bin grid)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id")),
    request_body = SectionBody,
    responses(
        (status = 201, body = Envelope<SectionCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh)): WarehousePath,
    ValidatedJson(input): ValidatedJson<SectionBody>,
) -> Result<(StatusCode, Data<SectionCreated>), AppError> {
    let warehouse_id = service::warehouse(&state, access.workspace_id, &wh).await?;
    let section_id = service::create(&state, access.workspace_id, warehouse_id, input).await?;
    Ok((StatusCode::CREATED, Data(SectionCreated { section_id })))
}

/// POST /sections/{section} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}", tag = "warehouse_sections",
    operation_id = "warehouse_sections_update", summary = "Update a section",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id")),
    request_body = SectionBody,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse or section, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, id)): SectionPath,
    ValidatedJson(input): ValidatedJson<SectionBody>,
) -> Result<Message, AppError> {
    let warehouse_id = service::warehouse(&state, access.workspace_id, &wh).await?;
    service::update(&state, access.workspace_id, warehouse_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /sections/{section} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}", tag = "warehouse_sections",
    operation_id = "warehouse_sections_delete", summary = "Delete a section",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("warehouse" = Uuid, Path, description = "Warehouse id"), ("section" = Uuid, Path, description = "Section id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, warehouse or section, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, id)): SectionPath,
) -> Result<Message, AppError> {
    let warehouse_id = service::warehouse(&state, access.workspace_id, &wh).await?;
    service::delete(&state, access.workspace_id, warehouse_id, &id).await?;
    Ok(Message("ok"))
}

/// Trimmed query value; blank = not sent.
fn blank(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}
