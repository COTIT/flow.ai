//! HTTP edge for /v1/workspace/{workspace}/purchase-orders. Every route needs a
//! `purchaseorder.*` permission (checked before the body or any replay is read). Writes take
//! the C/A envelope plus a required Idempotency-Key and replay permanently by `client_ref`.

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
        purchasing::{
            dto::{
                BookPayload, BookReview, BookReviewBody, CancelPayload, CloseLinesPayload,
                ConfirmPayload, ConfirmReview, OrderListQuery, OrderPayload, PageQuery,
                PurchaseOrderEvent, PurchaseOrderResource, PurchaseReceiptResource, ReceiptPayload,
            },
            receipts, service,
        },
    },
};

permissions! {
    List => "purchaseorder.list",
    Get => "purchaseorder.get",
    CreatePo => "purchaseorder.create",
    Update => "purchaseorder.update",
    Confirm => "purchaseorder.confirm",
    Receive => "purchaseorder.receive",
    Book => "purchaseorder.book",
    Close => "purchaseorder.close",
    Cancel => "purchaseorder.cancel",
}

/// A path id; malformed reads as missing.
fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// purchase_orders_list: List purchase orders.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-orders", tag = "purchasing",
    operation_id = "purchase_orders_list",
    summary = "List purchase orders",
    params(("workspace" = Uuid, Path, description = "Workspace id"), OrderListQuery),
    responses(
        (status = 200, body = Paginated<PurchaseOrderResource>, description = "Success, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid filter"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_list(
    State(state): State<AppState>,
    access: Can<List>,
    Query(q): Query<OrderListQuery>,
) -> Result<Json<Paginated<PurchaseOrderResource>>, AppError> {
    Ok(Json(service::list(&state, access.workspace_id, &q).await?))
}

/// purchase_orders_create: Create a draft purchase order.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-orders", tag = "purchasing",
    operation_id = "purchase_orders_create",
    summary = "Create a draft purchase order",
    request_body = Create<OrderPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<PurchaseOrderResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref with another request"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_create(
    State(state): State<AppState>,
    access: Can<CreatePo>,
    cmd: Command<Create<OrderPayload>>,
) -> Result<Answer, AppError> {
    service::create(&state, &access, cmd).await
}

/// purchase_orders_get: Read a purchase order.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-orders/{order}", tag = "purchasing",
    operation_id = "purchase_orders_get",
    summary = "Read a purchase order",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id")),
    responses(
        (status = 200, body = Envelope<PurchaseOrderResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_get(
    State(state): State<AppState>,
    access: Can<Get>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<PurchaseOrderResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        service::get(&mut conn, access.workspace_id, id(&raw)?).await?,
    ))
}

/// purchase_orders_update: Replace a draft's fields and lines.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/purchase-orders/{order}", tag = "purchasing",
    operation_id = "purchase_orders_update",
    summary = "Replace a draft's fields and lines",
    request_body = Action<OrderPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<PurchaseOrderResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or not a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_update(
    State(state): State<AppState>,
    access: Can<Update>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<OrderPayload>>,
) -> Result<Answer, AppError> {
    service::update(&state, &access, id(&raw)?, cmd).await
}

/// purchase_orders_confirm_review: Preview confirming a draft (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-orders/{order}/confirm/review", tag = "purchasing",
    operation_id = "purchase_orders_confirm_review",
    summary = "Preview confirming a draft (no writes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id")),
    responses(
        (status = 200, body = Envelope<ConfirmReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Not a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_confirm_review(
    State(state): State<AppState>,
    access: Can<Confirm>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<ConfirmReview>, AppError> {
    Ok(Data(
        service::confirm_review(&state, access.workspace_id, id(&raw)?).await?,
    ))
}

/// purchase_orders_confirm: Confirm a reviewed draft.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-orders/{order}/confirm", tag = "purchasing",
    operation_id = "purchase_orders_confirm",
    summary = "Confirm a reviewed draft",
    request_body = Action<ConfirmPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<PurchaseOrderResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review changed) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or not a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_confirm(
    State(state): State<AppState>,
    access: Can<Confirm>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ConfirmPayload>>,
) -> Result<Answer, AppError> {
    service::confirm(&state, &access, id(&raw)?, cmd).await
}

/// purchase_orders_close_lines: Close open remainders.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-orders/{order}/close-lines", tag = "purchasing",
    operation_id = "purchase_orders_close_lines",
    summary = "Close open remainders",
    request_body = Action<CloseLinesPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<PurchaseOrderResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, record or line, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, more than open, or not confirmed"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_close_lines(
    State(state): State<AppState>,
    access: Can<Close>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<CloseLinesPayload>>,
) -> Result<Answer, AppError> {
    service::close_lines(&state, &access, id(&raw)?, cmd).await
}

/// purchase_orders_cancel: Cancel an order nothing was received on.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-orders/{order}/cancel", tag = "purchasing",
    operation_id = "purchase_orders_cancel",
    summary = "Cancel an order nothing was received on",
    request_body = Action<CancelPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<PurchaseOrderResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, already cancelled, or goods received"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_cancel(
    State(state): State<AppState>,
    access: Can<Cancel>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<CancelPayload>>,
) -> Result<Answer, AppError> {
    service::cancel(&state, &access, id(&raw)?, cmd).await
}

/// purchase_orders_events: An order's history, newest first.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-orders/{order}/events", tag = "purchasing",
    operation_id = "purchase_orders_events",
    summary = "An order's history, newest first",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id")),
    responses(
        (status = 200, body = Envelope<Vec<PurchaseOrderEvent>>, description = "Success (at most 500)"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn orders_events(
    State(state): State<AppState>,
    access: Can<Get>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<Vec<PurchaseOrderEvent>>, AppError> {
    Ok(Data(
        service::events(&state, access.workspace_id, id(&raw)?).await?,
    ))
}

/// purchase_receipts_list: List an order's receipts.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-orders/{order}/receipts", tag = "purchasing",
    operation_id = "purchase_receipts_list",
    summary = "List an order's receipts",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), PageQuery),
    responses(
        (status = 200, body = Paginated<PurchaseReceiptResource>, description = "Success, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn receipts_list(
    State(state): State<AppState>,
    access: Can<Get>,
    Path((_, raw)): Path<(String, String)>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<PurchaseReceiptResource>>, AppError> {
    Ok(Json(receipts::list(&state, &access, id(&raw)?, &q).await?))
}

/// purchase_receipts_create: Accept goods against open order lines (no stock yet).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-orders/{order}/receipts", tag = "purchasing",
    operation_id = "purchase_receipts_create",
    summary = "Accept goods against open order lines (no stock yet)",
    request_body = Create<ReceiptPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<PurchaseReceiptResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, order or order line, or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref with another request"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, more than open, order not confirmed, or `unsupported_active_erp_operation` (unvalued stock)"),
    ),
    security(("bearer" = []))
)]
pub async fn receipts_create(
    State(state): State<AppState>,
    access: Can<Receive>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Create<ReceiptPayload>>,
) -> Result<Answer, AppError> {
    receipts::receive(&state, &access, id(&raw)?, cmd).await
}

/// purchase_receipts_get: Read a receipt.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-orders/{order}/receipts/{receipt}", tag = "purchasing",
    operation_id = "purchase_receipts_get",
    summary = "Read a receipt",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), ("receipt" = Uuid, Path, description = "Receipt id")),
    responses(
        (status = 200, body = Envelope<PurchaseReceiptResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn receipts_get(
    State(state): State<AppState>,
    access: Can<Get>,
    Path((_, order, receipt)): Path<(String, String, String)>,
) -> Result<Data<PurchaseReceiptResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        receipts::get(
            &mut conn,
            access.workspace_id,
            id(&order)?,
            id(&receipt)?,
            access.allows("accounting.read"),
        )
        .await?,
    ))
}

/// purchase_receipts_book_review: Preview booking receipt lines (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-orders/{order}/receipts/{receipt}/book/review", tag = "purchasing",
    operation_id = "purchase_receipts_book_review",
    summary = "Preview booking receipt lines (no writes)",
    request_body = BookReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), ("receipt" = Uuid, Path, description = "Receipt id")),
    responses(
        (status = 200, body = Envelope<BookReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, record or line, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`: a line is already booked"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `receipt_booking_required` / `backdated_valuation` / `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn receipts_book_review(
    State(state): State<AppState>,
    access: Can<Book>,
    Path((_, order, receipt)): Path<(String, String, String)>,
    ValidatedJson(input): ValidatedJson<BookReviewBody>,
) -> Result<Data<BookReview>, AppError> {
    Ok(Data(
        receipts::book_review(&state, &access, (id(&order)?, id(&receipt)?), &input).await?,
    ))
}

/// purchase_receipts_book: Book receipt lines into valued stock.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-orders/{order}/receipts/{receipt}/book", tag = "purchasing",
    operation_id = "purchase_receipts_book",
    summary = "Book receipt lines into valued stock",
    request_body = Action<BookPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Purchase order id"), ("receipt" = Uuid, Path, description = "Receipt id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<PurchaseReceiptResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, record or line, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version, review or line already booked) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `receipt_booking_required` / `backdated_valuation` / `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn receipts_book(
    State(state): State<AppState>,
    access: Can<Book>,
    Path((_, order, receipt)): Path<(String, String, String)>,
    cmd: Command<Action<BookPayload>>,
) -> Result<Answer, AppError> {
    receipts::book(&state, &access, (id(&order)?, id(&receipt)?), cmd).await
}
