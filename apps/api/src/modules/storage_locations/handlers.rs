//! HTTP edge for /v1/workspace/{workspace}/erp/storage-locations. Laravel had no catalog
//! permissions here (owner/admin only, reads unchecked); mapped to `warehouse.get` /
//! `warehouse.update` (user decision, batch 6).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::storage_locations::{dto::PlaceBody, service},
};

permissions! {
    PlaceRead => "warehouse.get",
    PlaceWrite => "warehouse.update",
}

/// POST / -> 201 `{data:{storage_location_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<PlaceWrite>,
    ValidatedJson(n): ValidatedJson<PlaceBody>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    let id = service::create(&state, access.workspace_id, n).await?;
    Ok((
        StatusCode::CREATED,
        Data(json!({ "storage_location_id": id })),
    ))
}

/// GET /{parent_domain}/{parent_id} -> `{data:[...]}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<PlaceRead>,
    Path((_, domain, id)): Path<(String, String, String)>,
) -> Result<Json<Value>, AppError> {
    let data = service::list(&state, access.workspace_id, &domain, &id).await?;
    Ok(Json(json!({ "data": data })))
}

/// DELETE /{id} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<PlaceWrite>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
