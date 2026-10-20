//! HTTP edge for /v1/workspace/{workspace}/shipment-methods: `shipmentmethod.*` permission
//! (via `Can`), call service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<ShipmentMethodDelete>,
    Path((_, id)): MethodPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
