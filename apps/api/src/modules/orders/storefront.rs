//! HTTP edge for /v1/storefront/orders: shops holding a `checkout:write` key place web orders,
//! read their minimal view and start the payment.

use axum::extract::{Path, State};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        app_key::AppKey,
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
    },
    idempotency::{Answer, Keyed},
    modules::{
        app_keys::secret::CHECKOUT_WRITE,
        orders::{
            checkout,
            dto::{CheckoutRequest, PaymentIntentResource, PaymentRequest, StorefrontOrder},
        },
    },
};

/// POST /storefront/orders -> 201 `{data: order}`.
#[utoipa::path(
    post, path = "/storefront/orders", tag = "storefront",
    operation_id = "storefront_orders_create", summary = "Place a web order", description = "429 also when the workspace has `CHECKOUT_HOLD_MAX` web orders awaiting payment.",
    params(("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request or another key's use of it is a 422. Kept for a day.")),
    request_body = CheckoutRequest,
    responses(
        (status = 201, body = Envelope<StorefrontOrder>, description = "Awaiting payment; the stock is held until `expires_at`"),
        (status = 401, body = MessageBody, description = "Missing, unknown or revoked app key"),
        (status = 403, body = MessageBody, description = "Key lacks `checkout:write`"),
        (status = 422, body = UnprocessableBody, description = "Invalid lines or customer, a price sent, not enough stock, or invalid Idempotency-Key"),
    ),
    security(("app_key" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    key: AppKey,
    Keyed { input, key: idem }: Keyed<CheckoutRequest>,
) -> Result<Answer, AppError> {
    key.require(CHECKOUT_WRITE)?;
    checkout::create(&state, &key, input, idem).await
}

/// GET /storefront/orders/{order}.
#[utoipa::path(
    get, path = "/storefront/orders/{order}", tag = "storefront",
    operation_id = "storefront_orders_get", summary = "Get a web order",
    params(("order" = Uuid, Path, description = "Order id")),
    responses(
        (status = 200, body = Envelope<StorefrontOrder>, description = "Ids, number, status, totals and currency; never the customer"),
        (status = 401, body = MessageBody, description = "Missing, unknown or revoked app key"),
        (status = 403, body = MessageBody, description = "Key lacks `checkout:write`"),
        (status = 404, body = MessageBody, description = "Unknown order, an office order, or another workspace's"),
    ),
    security(("app_key" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    key: AppKey,
    Path(id): Path<String>,
) -> Result<Data<StorefrontOrder>, AppError> {
    key.require(CHECKOUT_WRITE)?;
    Ok(Data(checkout::get(&state, &key, &id).await?))
}

/// POST /storefront/orders/{order}/payment -> `{data: intent}`.
#[utoipa::path(
    post, path = "/storefront/orders/{order}/payment", tag = "storefront",
    operation_id = "storefront_orders_payment", summary = "Start or resume the payment", description = "503 with code `payment_setup_required` when merchant setup is incomplete or new collections are disabled; retrying cannot complete setup. Legacy providers return 503 `no payment provider configured` without a provider. Other failures can follow an accepted submission: check the order before retrying.",
    params(("order" = Uuid, Path, description = "Order id")),
    request_body = PaymentRequest,
    responses(
        (status = 200, body = Envelope<PaymentIntentResource>, description = "The order's payment intent (the same one on every call while it is open)"),
        (status = 401, body = MessageBody, description = "Missing, unknown or revoked app key"),
        (status = 403, body = MessageBody, description = "Key lacks `checkout:write`"),
        (status = 404, body = MessageBody, description = "Unknown order, an office order, or another workspace's"),
        (status = 409, body = MessageBody, description = "The order isn't awaiting payment (paid, expired or cancelled)"),
        (status = 422, body = UnprocessableBody, description = "Invalid redirect_url"),
    ),
    security(("app_key" = []))
)]
pub async fn payment(
    State(state): State<AppState>,
    key: AppKey,
    Path(id): Path<String>,
    ValidatedJson(input): ValidatedJson<PaymentRequest>,
) -> Result<Data<PaymentIntentResource>, AppError> {
    key.require(CHECKOUT_WRITE)?;
    Ok(Data(checkout::intent(&state, &key, &id, input).await?))
}
