//! HTTP edge for /v1/workspace/{workspace}/shipment-methods: `shipmentmethod.*` permission
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
    modules::shipment_methods::{
        dto::{
            CreateShipmentMethodRequest, ListQuery, ShipmentMethodCreated, ShipmentMethodResource,
            UpdateShipmentMethodRequest,
        },
        repo::ListFilter,
        service,
    },
};

permissions! {
    ShipmentMethodCreate => "shipmentmethod.create",
    ShipmentMethodList => "shipmentmethod.list",
    ShipmentMethodGet => "shipmentmethod.get",
    ShipmentMethodUpdate => "shipmentmethod.update",
    ShipmentMethodDelete => "shipmentmethod.delete",
}

/// `{workspace}` + `{shipment_method}`; the id stays raw so a malformed one is a 404.
type MethodPath = Path<(String, String)>;

/// GET /shipment-methods -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/shipment-methods", tag = "shipment_methods",
    operation_id = "shipment_methods_list", summary = "List shipment methods",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<ShipmentMethodResource>, description = "One page of shipment methods"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `shipmentmethod.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<ShipmentMethodList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<ShipmentMethodResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = ListFilter {
        pattern: pattern.as_deref(),
        can_be_selected: None,
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /shipment-methods/{shipment_method}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/shipment-methods/{shipment_method}", tag = "shipment_methods",
    operation_id = "shipment_methods_get", summary = "Get a shipment method",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("shipment_method" = Uuid, Path, description = "Shipment method id"),
    ),
    responses(
        (status = 200, body = Envelope<ShipmentMethodResource>, description = "The shipment method"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `shipmentmethod.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or shipment method, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<ShipmentMethodGet>,
    Path((_, id)): MethodPath,
) -> Result<Data<ShipmentMethodResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /shipment-methods -> 201 `{data:{shipment_method_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/shipment-methods", tag = "shipment_methods",
    operation_id = "shipment_methods_create", summary = "Create a shipment method",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateShipmentMethodRequest,
    responses(
        (status = 201, body = Envelope<ShipmentMethodCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `shipmentmethod.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<ShipmentMethodCreate>,
    ValidatedJson(input): ValidatedJson<CreateShipmentMethodRequest>,
) -> Result<(StatusCode, Data<ShipmentMethodCreated>), AppError> {
    let shipment_method_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(ShipmentMethodCreated { shipment_method_id }),
    ))
}

/// POST /shipment-methods/{shipment_method} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/shipment-methods/{shipment_method}", tag = "shipment_methods",
    operation_id = "shipment_methods_update", summary = "Update a shipment method",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("shipment_method" = Uuid, Path, description = "Shipment method id"),
    ),
    request_body = UpdateShipmentMethodRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `shipmentmethod.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or shipment method, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<ShipmentMethodUpdate>,
    Path((_, id)): MethodPath,
    ValidatedJson(patch): ValidatedJson<UpdateShipmentMethodRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /shipment-methods/{shipment_method} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/shipment-methods/{shipment_method}", tag = "shipment_methods",
    operation_id = "shipment_methods_delete", summary = "Delete a shipment method",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("shipment_method" = Uuid, Path, description = "Shipment method id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `shipmentmethod.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or shipment method, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<ShipmentMethodDelete>,
    Path((_, id)): MethodPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
