//! HTTP edge for /v1/workspace/{workspace}/erp/storage-locations. Reads take `warehouse.get`,
//! writes `warehouse.update`.
// Laravel had no catalog permissions here (owner/admin only, reads unchecked); user decision,
// batch 6.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::storage_locations::{
        dto::{PlaceBody, PlaceCreated, PlaceList, PlaceRequestDoc},
        service,
    },
};

permissions! {
    PlaceRead => "warehouse.get",
    PlaceWrite => "warehouse.update",
}

/// POST / -> 201 `{data:{storage_location_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/storage-locations", tag = "storage_locations",
    operation_id = "storage_locations_create", summary = "Attach a storage location to a record",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = PlaceRequestDoc,
    responses(
        (status = 201, body = Envelope<PlaceCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, parent, label or bin, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/storage-locations/{domain}/{parent}", tag = "storage_locations",
    operation_id = "storage_locations_list", summary = "List a record's storage locations",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("domain" = String, Path, description = "Parent domain, e.g. `product`"), ("parent" = Uuid, Path, description = "Parent record id")),
    responses(
        (status = 200, body = PlaceList, description = "Every storage location; not paged"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, domain or parent, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<PlaceRead>,
    Path((_, domain, id)): Path<(String, String, String)>,
) -> Result<Json<Value>, AppError> {
    let data = service::list(&state, access.workspace_id, &domain, &id).await?;
    Ok(Json(json!({ "data": data })))
}

/// DELETE /{id} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/erp/storage-locations/{id}", tag = "storage_locations",
    operation_id = "storage_locations_delete", summary = "Delete a storage location",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("id" = Uuid, Path, description = "Storage location id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `warehouse.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or storage location, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<PlaceWrite>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
