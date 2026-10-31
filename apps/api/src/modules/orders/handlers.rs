//! HTTP edge for /v1/workspace/{workspace}/orders (`order.*` permissions). Every write takes
//! an optional Idempotency-Key.

use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, KeyOnly, Keyed},
    modules::orders::{
        dto::{
            CreateOrderRequest, LinesQuantityRequest, ListQuery, OrderCreated, OrderResource,
            STATUSES, UpdateOrderRequest,
        },
        service,
    },
};

permissions! {
    OrderList => "order.list",
    OrderView => "order.view",
    OrderCreate => "order.create",
    OrderUpdate => "order.update",
    OrderDelete => "order.delete",
}

/// `{workspace}` + `{order}`; the id stays raw so a malformed one is a 404.
type OrderPath = Path<(String, String)>;

/// GET /orders -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/orders", tag = "orders",
    operation_id = "orders_list", summary = "List orders",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<OrderResource>, description = "One page of orders with lines, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Unknown status filter"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<OrderList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<OrderResource>>, AppError> {
    let status = query.status.as_deref().filter(|s| !s.is_empty());
    if status.is_some_and(|s| !STATUSES.contains(&s)) {
        return Err(AppError::field("status", "The selected status is invalid."));
    }
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (data, total) = service::list(
        &state,
        access.workspace_id,
        status,
        page.size,
        page.offset(),
    )
    .await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /orders -> 201 `{data:{order_id, number}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders", tag = "orders",
    operation_id = "orders_create", summary = "Create a draft order",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    request_body = CreateOrderRequest,
    responses(
        (status = 201, body = Envelope<OrderCreated>, description = "Created; returns the id and number"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or Idempotency-Key, key reused for another request"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<OrderCreate>,
    Keyed { input, key }: Keyed<CreateOrderRequest>,
) -> Result<Answer, AppError> {
    service::create(&state, access.workspace_id, access.user_id, input, key).await
}

/// GET /orders/{order}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/orders/{order}", tag = "orders",
    operation_id = "orders_get", summary = "Get an order",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id")),
    responses(
        (status = 200, body = Envelope<OrderResource>, description = "The order with its lines"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<OrderView>,
    Path((_, id)): OrderPath,
) -> Result<Data<OrderResource>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// PATCH /orders/{order} -> `{data: order}`.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/orders/{order}", tag = "orders",
    operation_id = "orders_update", summary = "Change an order",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    request_body = UpdateOrderRequest,
    responses(
        (status = 200, body = Envelope<OrderResource>, description = "The changed order"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or caller not a member"),
        (status = 409, body = MessageBody, description = "Lines or warehouse sent for an order past draft"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or Idempotency-Key, key reused for another request"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<OrderUpdate>,
    Path((_, id)): OrderPath,
    Keyed { input, key }: Keyed<UpdateOrderRequest>,
) -> Result<Answer, AppError> {
    service::update(&state, access.workspace_id, access.user_id, &id, input, key).await
}

/// DELETE /orders/{order}: drafts only.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/orders/{order}", tag = "orders",
    operation_id = "orders_delete", summary = "Delete a draft order",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    responses(
        (status = 200, body = MessageBody, description = "Deleted (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or caller not a member"),
        (status = 409, body = MessageBody, description = "Order is not a draft"),
        (status = 422, body = UnprocessableBody, description = "Invalid Idempotency-Key, or key reused for another request"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<OrderDelete>,
    Path((_, id)): OrderPath,
    KeyOnly(key): KeyOnly,
) -> Result<Answer, AppError> {
    service::delete(&state, access.workspace_id, access.user_id, &id, key).await
}

/// POST /orders/{order}/pick: picked totals per line.
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders/{order}/pick", tag = "orders",
    operation_id = "orders_pick", summary = "Record picked quantities",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    request_body = LinesQuantityRequest,
    responses(
        (status = 200, body = Envelope<OrderResource>, description = "The order with its picked quantities"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or caller not a member"),
        (status = 409, body = MessageBody, description = "Order not open"),
        (status = 422, body = UnprocessableBody, description = "Unknown line, quantity above the line's, or invalid Idempotency-Key"),
    ),
    security(("bearer" = []))
)]
pub async fn pick(
    State(state): State<AppState>,
    access: Can<OrderUpdate>,
    Path((_, id)): OrderPath,
    Keyed { input, key }: Keyed<LinesQuantityRequest>,
) -> Result<Answer, AppError> {
    service::pick(&state, access.workspace_id, access.user_id, &id, input, key).await
}

/// POST /orders/{order}/confirm.
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders/{order}/confirm", tag = "orders",
    operation_id = "orders_confirm", summary = "Confirm a draft",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    responses(
        (status = 200, body = Envelope<OrderResource>, description = "draft → open; reserves every stock line"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or caller not a member"),
        (status = 409, body = MessageBody, description = "Order not a draft"),
        (status = 422, body = UnprocessableBody, description = "Not enough stock on a line, or invalid Idempotency-Key"),
    ),
    security(("bearer" = []))
)]
pub async fn confirm(
    State(state): State<AppState>,
    access: Can<OrderUpdate>,
    Path((_, id)): OrderPath,
    KeyOnly(key): KeyOnly,
) -> Result<Answer, AppError> {
    service::confirm(&state, access.workspace_id, access.user_id, &id, key).await
}

/// POST /orders/{order}/complete.
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders/{order}/complete", tag = "orders",
    operation_id = "orders_complete", summary = "Complete an open order",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    responses(
        (status = 200, body = Envelope<OrderResource>, description = "open → completed; takes the reserved stock"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or caller not a member"),
        (status = 409, body = MessageBody, description = "Order not open, or a stock line not fully picked"),
        (status = 422, body = UnprocessableBody, description = "Invalid Idempotency-Key"),
    ),
    security(("bearer" = []))
)]
pub async fn complete(
    State(state): State<AppState>,
    access: Can<OrderUpdate>,
    Path((_, id)): OrderPath,
    KeyOnly(key): KeyOnly,
) -> Result<Answer, AppError> {
    service::complete(&state, access.workspace_id, access.user_id, &id, key).await
}

/// POST /orders/{order}/cancel.
#[utoipa::path(
    post, path = "/workspace/{workspace}/orders/{order}/cancel", tag = "orders",
    operation_id = "orders_cancel", summary = "Cancel an order",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Order id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    responses(
        (status = 200, body = Envelope<OrderResource>, description = "Cancelled; reserved stock released"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or order, or caller not a member"),
        (status = 409, body = MessageBody, description = "Order completed/returned/cancelled, or paid (`refund_required`)"),
        (status = 422, body = UnprocessableBody, description = "Invalid Idempotency-Key"),
    ),
    security(("bearer" = []))
)]
pub async fn cancel(
    State(state): State<AppState>,
    access: Can<OrderUpdate>,
    Path((_, id)): OrderPath,
    KeyOnly(key): KeyOnly,
) -> Result<Answer, AppError> {
    service::cancel(&state, access.workspace_id, access.user_id, &id, key).await
}
