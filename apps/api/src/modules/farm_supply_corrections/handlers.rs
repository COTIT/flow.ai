//! HTTP edge for /v1/workspace/{workspace}/farm/supply-batches/{batch}/corrections and
//! /farm/supply-corrections.

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
    modules::farm_supply_corrections::{
        dto::{CreateSupplyCorrectionRequest, SupplyCorrectionListQuery, SupplyCorrectionResource},
        service,
    },
};

/// POST /farm/supply-batches/{batch}/corrections -> 201 filed, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/supply-batches/{batch}/corrections", tag = "farm_supply",
    operation_id = "farm_supply_correction_request", summary = "Request a kg correction of a batch",
    description = "ops or admin. Each line sets the kg one farmer should have on a share line of the batch (kg only, no price). A wrong farmer is two lines on one share. Pending until a different ops or admin approves it in the Office. Refused when the raw lot is sold out, when a cut meets a lot dispatched over a supply link or passes its free stock, and when an increase passes the harvest. The response previews each farmer's money change and balance after; a negative balance blocks payouts until later credits cover it. The same `client_ref` and body replay (200).",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("batch" = Uuid, Path, description = "Batch id"),
    ),
    request_body = CreateSupplyCorrectionRequest,
    responses(
        (status = 200, body = Envelope<SupplyCorrectionResource>, description = "Replayed"),
        (status = 201, body = Envelope<SupplyCorrectionResource>, description = "Filed, pending"),
        (status = 403, body = MessageBody, description = "Not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown batch, or outside the caller's workspace"),
        (status = 409, body = MessageBody, description = "client_ref used with another body or by another member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid lines, no change, sold out, dispatched, short stock or over-commitment"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, batch)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<CreateSupplyCorrectionRequest>,
) -> Result<(StatusCode, Data<SupplyCorrectionResource>), AppError> {
    let (c, created) = service::file(&state, &access, &batch, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(c)))
}

/// GET /farm/supply-corrections -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/supply-corrections", tag = "farm_supply",
    operation_id = "farm_supply_correction_list", summary = "List supply corrections",
    description = "Staff: corrections of their villages (ops and admin: all), newest first, with each one's lines and money effect.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), SupplyCorrectionListQuery),
    responses(
        (status = 200, body = Paginated<SupplyCorrectionResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "Caller is a farmer, not staff"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown status or malformed batch id"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<SupplyCorrectionListQuery>,
) -> Result<Json<Paginated<SupplyCorrectionResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (status, batch) = (query.status.as_deref(), query.batch_id.as_deref());
    let (data, total) =
        service::list(&state, &access, status, batch, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /farm/supply-corrections/{correction}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/supply-corrections/{correction}", tag = "farm_supply",
    operation_id = "farm_supply_correction_get", summary = "Get a supply correction",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("correction" = Uuid, Path, description = "Correction id"),
    ),
    responses(
        (status = 200, body = Envelope<SupplyCorrectionResource>, description = "The correction"),
        (status = 403, body = MessageBody, description = "Caller is a farmer, not staff"),
        (status = 404, body = MessageBody, description = "Unknown correction, or outside the caller's villages or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<SupplyCorrectionResource>, AppError> {
    Ok(Data(service::get(&state, &access, &id).await?))
}

/// POST /farm/supply-corrections/{correction}/approve.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/supply-corrections/{correction}/approve", tag = "farm_supply",
    operation_id = "farm_supply_correction_approve", summary = "Approve a supply correction",
    description = "ops or admin other than the requester. One transaction: the raw lot's stock moves by the net kg, past credits of the batch are re-split by the corrected kg (one `supply_correction` passbook line per farmer whose money moved; the batch total stays), and `supply.corrected` is sent. The windows are checked again.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("correction" = Uuid, Path, description = "Correction id"),
    ),
    responses(
        (status = 200, body = Envelope<SupplyCorrectionResource>, description = "Approved"),
        (status = 403, body = MessageBody, description = "Not ops or admin, or the requester"),
        (status = 404, body = MessageBody, description = "Unknown correction, or outside the caller's workspace"),
        (status = 409, body = MessageBody, description = "Already approved or rejected"),
        (status = 422, body = ValidationErrorBody, description = "Sold out, dispatched, short stock, over-commitment, or kg below 0"),
    ),
    security(("bearer" = []))
)]
pub async fn approve(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<SupplyCorrectionResource>, AppError> {
    Ok(Data(service::decide(&state, &access, &id, true).await?))
}

/// POST /farm/supply-corrections/{correction}/reject.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/supply-corrections/{correction}/reject", tag = "farm_supply",
    operation_id = "farm_supply_correction_reject", summary = "Reject a supply correction",
    description = "ops or admin other than the requester. Records the decision; nothing else moves.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("correction" = Uuid, Path, description = "Correction id"),
    ),
    responses(
        (status = 200, body = Envelope<SupplyCorrectionResource>, description = "Rejected"),
        (status = 403, body = MessageBody, description = "Not ops or admin, or the requester"),
        (status = 404, body = MessageBody, description = "Unknown correction, or outside the caller's workspace"),
        (status = 409, body = MessageBody, description = "Already approved or rejected"),
    ),
    security(("bearer" = []))
)]
pub async fn reject(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<SupplyCorrectionResource>, AppError> {
    Ok(Data(service::decide(&state, &access, &id, false).await?))
}
