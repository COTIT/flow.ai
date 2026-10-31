//! HTTP edge for /v1/workspace/{workspace}/custom-status: `customstatus.*` permission
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
        list::{Paginated, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::{
        custom_statuses::{
            dto::{
                CreateCustomStatusRequest, CustomStatusCreated, CustomStatusResource, ListQuery,
                UpdateCustomStatusRequest,
            },
            repo::ListFilter,
            service,
        },
        custom_types::dto::domain_filter,
    },
};

// Laravel checked `customstatus.view` for one row; genesy uses the catalog's `get`.
permissions! {
    CustomStatusCreate => "customstatus.create",
    CustomStatusList => "customstatus.list",
    CustomStatusGet => "customstatus.get",
    CustomStatusUpdate => "customstatus.update",
    CustomStatusDelete => "customstatus.delete",
}

/// `{workspace}` + `{custom_status}`; the id stays raw so a malformed one is a 404.
type StatusPath = Path<(String, String)>;

/// GET /custom-status -> `{data:[...], meta}`, archived rows left out.
#[utoipa::path(
    get, path = "/workspace/{workspace}/custom-status", tag = "custom_statuses",
    operation_id = "custom_statuses_list", summary = "List custom statuses",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<CustomStatusResource>, description = "One page of custom statuses"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customstatus.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long, unknown `domain`, or bad `limit`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<CustomStatusList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<CustomStatusResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let domain = domain_filter(query.domain.as_deref())?;
    let page = query.page()?;
    let filter = ListFilter {
        domain: domain.as_deref(),
        pattern: pattern.as_deref(),
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /custom-status/{custom_status}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/custom-status/{custom_status}", tag = "custom_statuses",
    operation_id = "custom_statuses_get", summary = "Get a custom status",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("custom_status" = Uuid, Path, description = "Custom status id"),
    ),
    responses(
        (status = 200, body = Envelope<CustomStatusResource>, description = "The custom status"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customstatus.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or custom status, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<CustomStatusGet>,
    Path((_, id)): StatusPath,
) -> Result<Data<CustomStatusResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /custom-status -> 201 `{data:{custom_status_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/custom-status", tag = "custom_statuses",
    operation_id = "custom_statuses_create", summary = "Create a custom status",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateCustomStatusRequest,
    responses(
        (status = 201, body = Envelope<CustomStatusCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customstatus.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `name` already used in the domain"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<CustomStatusCreate>,
    ValidatedJson(input): ValidatedJson<CreateCustomStatusRequest>,
) -> Result<(StatusCode, Data<CustomStatusCreated>), AppError> {
    let custom_status_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(CustomStatusCreated { custom_status_id }),
    ))
}

/// POST /custom-status/{custom_status} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/custom-status/{custom_status}", tag = "custom_statuses",
    operation_id = "custom_statuses_update", summary = "Update a custom status",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("custom_status" = Uuid, Path, description = "Custom status id"),
    ),
    request_body = UpdateCustomStatusRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customstatus.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or custom status, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `name` already used in the domain"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<CustomStatusUpdate>,
    Path((_, id)): StatusPath,
    ValidatedJson(patch): ValidatedJson<UpdateCustomStatusRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /custom-status/{custom_status} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/custom-status/{custom_status}", tag = "custom_statuses",
    operation_id = "custom_statuses_delete", summary = "Delete a custom status",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("custom_status" = Uuid, Path, description = "Custom status id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customstatus.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or custom status, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<CustomStatusDelete>,
    Path((_, id)): StatusPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
