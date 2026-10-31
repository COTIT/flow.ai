//! HTTP edge for /v1/workspace/{workspace}/farm/supply-batches.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        farm::FarmAccess,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_supply::{
        dto::{BatchResource, CreateBatchRequest, ListQuery},
        service,
    },
};

/// GET /farm/supply-batches -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/supply-batches", tag = "farm_supply",
    operation_id = "farm_supply_list", summary = "List supply batches",
    description = "Staff: batches of their villages (ops and admin: all), newest first.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<BatchResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "Caller is a farmer, not staff"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<BatchResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (data, total) = service::list(&state, &access, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /farm/supply-batches -> 201 created, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/supply-batches", tag = "farm_supply",
    operation_id = "farm_supply_create", summary = "Buy harvests into a raw lot",
    description = "ops or admin who verified none of the harvests. Each line is a current, verified, actual harvest of the village and crop; kg over all batches never pass the harvest. Makes the raw lot (`lot.created`) and the frozen batch in one transaction. The same `client_ref` and body replay (200); another body or member under it is 409.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateBatchRequest,
    responses(
        (status = 200, body = Envelope<BatchResource>, description = "Replayed: this client_ref was used with this body"),
        (status = 201, body = Envelope<BatchResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Not ops or admin, or the caller verified one of the harvests"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 409, body = MessageBody, description = "client_ref used with another body or by another member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, a harvest that can't be bought, over-commitment, or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<CreateBatchRequest>,
) -> Result<(StatusCode, Data<BatchResource>), AppError> {
    let (batch, created) = service::create(&state, &access, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(batch)))
}

/// GET /farm/supply-batches/{batch}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/supply-batches/{batch}", tag = "farm_supply",
    operation_id = "farm_supply_get", summary = "Get a supply batch",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("batch" = Uuid, Path, description = "Batch id"),
    ),
    responses(
        (status = 200, body = Envelope<BatchResource>, description = "The batch with its lines"),
        (status = 403, body = MessageBody, description = "Caller is a farmer, not staff"),
        (status = 404, body = MessageBody, description = "Unknown batch, or outside the caller's villages or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<BatchResource>, AppError> {
    Ok(Data(service::get(&state, &access, &id).await?))
}
