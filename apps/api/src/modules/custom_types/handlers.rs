//! HTTP edge for /v1/workspace/{workspace}/custom-types: `customtype.*` permission
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
        list::{Page, Paginated, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::custom_types::{
        dto::{
            CreateCustomTypeRequest, CustomTypeCreated, CustomTypeResource, ListQuery,
            UpdateCustomTypeRequest, domain_filter, flag_filter,
        },
        repo::ListFilter,
        service,
    },
};

// The catalog has `customtype.view` (no `get`), and Laravel checked `view`.
permissions! {
    CustomTypeCreate => "customtype.create",
    CustomTypeList => "customtype.list",
    CustomTypeView => "customtype.view",
    CustomTypeUpdate => "customtype.update",
    CustomTypeDelete => "customtype.delete",
}

/// `{workspace}` + `{custom_type}`; the id stays raw so a malformed one is a 404.
type TypePath = Path<(String, String)>;

/// GET /custom-types -> `{data:[...], meta}`. `default_for_work_order` and
/// `is_leave_hours_type` only filter hour types, `is_supplier` only relation types.
#[utoipa::path(
    get, path = "/workspace/{workspace}/custom-types", tag = "custom_types",
    operation_id = "custom_types_list", summary = "List custom types",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<CustomTypeResource>, description = "One page of custom types"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customtype.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long, or unknown `domain`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<CustomTypeList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<CustomTypeResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let domain = domain_filter(query.domain.as_deref())?;
    let hours = domain.as_deref() == Some("hour_registration_type");
    let relation = domain.as_deref() == Some("relation");
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = ListFilter {
        domain: domain.as_deref(),
        pattern: pattern.as_deref(),
        default_for_work_order: flag_filter(query.default_for_work_order.as_deref(), hours),
        is_leave_hours_type: flag_filter(query.is_leave_hours_type.as_deref(), hours),
        is_supplier: flag_filter(query.is_supplier.as_deref(), relation),
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /custom-types/{custom_type}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/custom-types/{custom_type}", tag = "custom_types",
    operation_id = "custom_types_get", summary = "Get a custom type",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("custom_type" = Uuid, Path, description = "Custom type id"),
    ),
    responses(
        (status = 200, body = Envelope<CustomTypeResource>, description = "The custom type"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customtype.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or custom type, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<CustomTypeView>,
    Path((_, id)): TypePath,
) -> Result<Data<CustomTypeResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /custom-types -> 201 `{data:{custom_type_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/custom-types", tag = "custom_types",
    operation_id = "custom_types_create", summary = "Create a custom type",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateCustomTypeRequest,
    responses(
        (status = 201, body = Envelope<CustomTypeCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customtype.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `name` already used in the domain"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<CustomTypeCreate>,
    ValidatedJson(input): ValidatedJson<CreateCustomTypeRequest>,
) -> Result<(StatusCode, Data<CustomTypeCreated>), AppError> {
    let custom_type_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(CustomTypeCreated { custom_type_id }),
    ))
}

/// POST /custom-types/{custom_type} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/custom-types/{custom_type}", tag = "custom_types",
    operation_id = "custom_types_update", summary = "Update a custom type",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("custom_type" = Uuid, Path, description = "Custom type id"),
    ),
    request_body = UpdateCustomTypeRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customtype.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or custom type, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `name` already used in the domain"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<CustomTypeUpdate>,
    Path((_, id)): TypePath,
    ValidatedJson(patch): ValidatedJson<UpdateCustomTypeRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /custom-types/{custom_type} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/custom-types/{custom_type}", tag = "custom_types",
    operation_id = "custom_types_delete", summary = "Delete a custom type",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("custom_type" = Uuid, Path, description = "Custom type id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `customtype.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or custom type, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<CustomTypeDelete>,
    Path((_, id)): TypePath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
