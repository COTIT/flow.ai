//! HTTP edge for /v1/workspace/{workspace}/orders/{order}/shipments. Every route needs a
//! `shipment.*` permission (checked before the body or any replay is read). Writes take the
//! C/A envelope plus a required Idempotency-Key and replay permanently by `client_ref`.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::Paginated,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Command},
    modules::{
        accounting::dto::{Action, Create},
        purchasing::dto::{CancelPayload, PageQuery},
        shipments::{
            dto::{PostPayload, PostReview, PostReviewBody, ShipmentPayload, ShipmentResource},
            service,
        },
    },
};

permissions! {
    List => "shipment.list",
    Get => "shipment.get",
    CreateShipment => "shipment.create",
    Update => "shipment.update",
    Post => "shipment.post",
    Cancel => "shipment.cancel",
}

/// A path id; malformed reads as missing.
fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// shipments_list: List an order's shipments.
#[utoipa::path(
    get, path = "/workspace/{workspace}/orders/{order}/shipments", tag = "shipments",
    operation_id = "shipments_list",
    summary = "List an order's shipments",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), PageQuery),
    responses(
        (status = 200, body = Paginated<ShipmentResource>, description = "Success, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn shipments_list(
    State(state): State<AppState>,
    access: Can<List>,
    Path((_, order)): Path<(String, String)>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<ShipmentResource>>, AppError> {
    Ok(Json(
        service::list(&state, access.workspace_id, id(&order)?, &q).await?,
    ))
}

/// shipments_create: Plan a draft shipment (no stock moves).
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders/{order}/shipments", tag = "shipments",
    operation_id = "shipments_create",
    summary = "Plan a draft shipment (no stock moves)",
    request_body = Create<ShipmentPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<ShipmentResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or not a member"),
        (status = 409, body = MessageBody, description = "Order not open, or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or more than is left to ship"),
    ),
    security(("bearer" = []))
)]
pub async fn shipments_create(
    State(state): State<AppState>,
    access: Can<CreateShipment>,
    Path((_, order)): Path<(String, String)>,
    cmd: Command<Create<ShipmentPayload>>,
) -> Result<Answer, AppError> {
    service::create(&state, &access, id(&order)?, cmd).await
}

/// shipments_get: Read a shipment.
#[utoipa::path(
    get, path = "/workspace/{workspace}/orders/{order}/shipments/{shipment}", tag = "shipments",
    operation_id = "shipments_get",
    summary = "Read a shipment",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("shipment" = Uuid, Path, description = "Shipment id")),
    responses(
        (status = 200, body = Envelope<ShipmentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, order or shipment, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn shipments_get(
    State(state): State<AppState>,
    access: Can<Get>,
    Path((_, order, shipment)): Path<(String, String, String)>,
) -> Result<Data<ShipmentResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        service::get(&mut conn, access.workspace_id, id(&order)?, id(&shipment)?).await?,
    ))
}

/// shipments_update: Replace a draft's lines and note.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/orders/{order}/shipments/{shipment}", tag = "shipments",
    operation_id = "shipments_update",
    summary = "Replace a draft's lines and note",
    request_body = Action<ShipmentPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("shipment" = Uuid, Path, description = "Shipment id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<ShipmentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, order or shipment, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, order not open, or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, more than is left to ship, or not a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn shipments_update(
    State(state): State<AppState>,
    access: Can<Update>,
    Path((_, order, shipment)): Path<(String, String, String)>,
    cmd: Command<Action<ShipmentPayload>>,
) -> Result<Answer, AppError> {
    service::update(&state, &access, (id(&order)?, id(&shipment)?), cmd).await
}

/// shipments_post_review: Preview posting a draft (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders/{order}/shipments/{shipment}/post/review", tag = "shipments",
    operation_id = "shipments_post_review",
    summary = "Preview posting a draft (no writes)",
    request_body = PostReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("shipment" = Uuid, Path, description = "Shipment id")),
    responses(
        (status = 200, body = Envelope<PostReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, order or shipment, or not a member"),
        (status = 409, body = MessageBody, description = "Order not open"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, not a draft, or `unsupported_active_erp_operation` (foreign-currency order in an active book)"),
    ),
    security(("bearer" = []))
)]
pub async fn shipments_post_review(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, order, shipment)): Path<(String, String, String)>,
    ValidatedJson(date): ValidatedJson<PostReviewBody>,
) -> Result<Data<PostReview>, AppError> {
    Ok(Data(
        service::post_review(
            &state,
            access.workspace_id,
            (id(&order)?, id(&shipment)?),
            date,
        )
        .await?,
    ))
}

/// shipments_post: Post a reviewed draft: stock out, delivered, sale recognised (active).
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders/{order}/shipments/{shipment}/post", tag = "shipments",
    operation_id = "shipments_post",
    summary = "Post a reviewed draft",
    request_body = Action<PostPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("shipment" = Uuid, Path, description = "Shipment id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<ShipmentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, order or shipment, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review changed), order not open, lot refused, or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, not a draft, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn shipments_post(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, order, shipment)): Path<(String, String, String)>,
    cmd: Command<Action<PostPayload>>,
) -> Result<Answer, AppError> {
    service::post(&state, &access, (id(&order)?, id(&shipment)?), cmd).await
}

/// shipments_cancel: Cancel a draft, freeing its planned quantity.
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders/{order}/shipments/{shipment}/cancel", tag = "shipments",
    operation_id = "shipments_cancel",
    summary = "Cancel a draft",
    request_body = Action<CancelPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("shipment" = Uuid, Path, description = "Shipment id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<ShipmentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, order or shipment, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or not a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn shipments_cancel(
    State(state): State<AppState>,
    access: Can<Cancel>,
    Path((_, order, shipment)): Path<(String, String, String)>,
    cmd: Command<Action<CancelPayload>>,
) -> Result<Answer, AppError> {
    service::cancel(&state, &access, (id(&order)?, id(&shipment)?), cmd).await
}
