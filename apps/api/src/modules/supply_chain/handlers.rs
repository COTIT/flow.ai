//! HTTP edge for packing (/lots/{lot}/packings) and supply transfers (/lots/{lot}/transfers,
//! /supply-transfers).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::supply_chain::{
        dto::{
            DispatchRequest, ListQuery, PackedLotResource, PackingRequest, ReceiveRequest,
            SupplyTransferResource,
        },
        service,
    },
};

permissions! {
    ProductUpdate => "product.update",
}

/// POST /lots/{lot}/packings -> 201 packed lot.
#[utoipa::path(
    post, path = "/workspace/{workspace}/lots/{lot}/packings", tag = "supply_chain",
    operation_id = "supply_chain_pack", summary = "Pack a supply lot into jars",
    description = "Makes a child lot of the jar product with `parent_lot_id`, and the kg used and units made as two exact numbers. Moves `raw_kg` out of the parent and `units` into the child in the warehouse, in one transaction (`lot.created`, `packed` on the parent). Only a lot from a supply batch or a supply transfer; never a packed one.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Parent lot id"),
    ),
    request_body = PackingRequest,
    responses(
        (status = 201, body = Envelope<PackedLotResource>, description = "Packed"),
        (status = 403, body = MessageBody, description = "No `product.update`"),
        (status = 404, body = MessageBody, description = "Unknown lot, or another workspace's"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, not enough kg free, or a lot that can't be packed"),
    ),
    security(("bearer" = []))
)]
pub async fn pack(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(p): ValidatedJson<PackingRequest>,
) -> Result<(StatusCode, Data<PackedLotResource>), AppError> {
    let lot = service::pack(&state, &access, &id, p).await?;
    Ok((StatusCode::CREATED, Data(lot)))
}

/// POST /lots/{lot}/transfers -> 201 dispatched transfer.
#[utoipa::path(
    post, path = "/workspace/{workspace}/lots/{lot}/transfers", tag = "supply_chain",
    operation_id = "supply_chain_dispatch", summary = "Send lot stock over a supply link",
    description = "Over a live link this workspace granted. Takes the quantity out of the lot in the warehouse (never more than it holds free) and leaves a dispatched transfer for the other side.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    request_body = DispatchRequest,
    responses(
        (status = 201, body = Envelope<SupplyTransferResource>, description = "Dispatched"),
        (status = 403, body = MessageBody, description = "No `product.update`, or the link is not live"),
        (status = 404, body = MessageBody, description = "Unknown lot, or another workspace's"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, unknown link, not enough stock, or not a supply lot"),
    ),
    security(("bearer" = []))
)]
pub async fn dispatch(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(d): ValidatedJson<DispatchRequest>,
) -> Result<(StatusCode, Data<SupplyTransferResource>), AppError> {
    let t = service::dispatch(&state, &access, &id, d).await?;
    let ws = access.workspace_id;
    Ok((StatusCode::CREATED, Data(SupplyTransferResource::new(t, ws))))
}

/// GET /supply-transfers -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supply-transfers", tag = "supply_chain",
    operation_id = "supply_chain_list", summary = "List supply transfers",
    description = "Transfers this workspace sent (`outgoing`) or is sent (`incoming`), newest first. No farmers, kg shares or prices.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<SupplyTransferResource>, description = "One page"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<SupplyTransferResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(&state, &access, page.size, page.offset()).await?;
    let ws = access.workspace_id;
    let data = rows
        .into_iter()
        .map(|t| SupplyTransferResource::new(t, ws))
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /supply-transfers/{transfer}/receive -> 201 received.
#[utoipa::path(
    post, path = "/workspace/{workspace}/supply-transfers/{transfer}/receive", tag = "supply_chain",
    operation_id = "supply_chain_receive", summary = "Receive a supply transfer",
    description = "The receiving workspace, while the link is live, once. Makes a new lot of its product (`lot.created`) holding the received quantity (at most the dispatched one) in its warehouse.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("transfer" = Uuid, Path, description = "Transfer id"),
    ),
    request_body = ReceiveRequest,
    responses(
        (status = 201, body = Envelope<SupplyTransferResource>, description = "Received"),
        (status = 403, body = MessageBody, description = "No `product.update`, the sending side, or the link is not live"),
        (status = 404, body = MessageBody, description = "Unknown transfer, or this workspace is no side of it"),
        (status = 409, body = MessageBody, description = "Already received"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or more than dispatched"),
    ),
    security(("bearer" = []))
)]
pub async fn receive(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(r): ValidatedJson<ReceiveRequest>,
) -> Result<(StatusCode, Data<SupplyTransferResource>), AppError> {
    let t = service::receive(&state, &access, &id, r).await?;
    let ws = access.workspace_id;
    Ok((StatusCode::CREATED, Data(SupplyTransferResource::new(t, ws))))
}
