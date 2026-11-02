//! HTTP edge for provider webhooks: `POST /v1/payments/webhooks/{provider}[/{workspace}]`. No
//! bearer auth (providers can't hold our keys); signing providers (Stripe) are checked on the
//! raw body first. The body only names an id, which is re-fetched from the provider before
//! anything changes. Always an empty 200, so nothing is learned and nothing is retried.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};

use crate::{AppState, error::AppError, modules::orders::payments};

/// POST /payments/webhooks/{provider}/{workspace}.
#[utoipa::path(
    post, path = "/payments/webhooks/{provider}/{workspace}", tag = "payments",
    operation_id = "payments_webhook", summary = "Payment provider status-changed call",
    description = "The body names a payment or refund id in the provider's format (the in-process test provider: `id=<id>`, form encoded; Stripe: a signed event, `Stripe-Signature` checked first). The status is always re-fetched from the provider; the answer is an empty 200 whatever the body names. At most 64 KiB.",
    params(("provider" = String, Path, description = "Provider name, e.g. `fake`"), ("workspace" = Uuid, Path, description = "Workspace id")),
    request_body(content = String, content_type = "application/x-www-form-urlencoded", description = "`id=<payment or refund id>`"),
    responses((status = 200, description = "Received (empty body)")),
)]
pub async fn receive(
    State(state): State<AppState>,
    Path((provider, workspace)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    payments::receive(&state, &provider, &workspace, &headers, &body).await?;
    Ok(StatusCode::OK)
}

/// POST /payments/webhooks/{provider}.
#[utoipa::path(
    post, path = "/payments/webhooks/{provider}", tag = "payments",
    operation_id = "payments_webhook_any", summary = "Payment provider event, one endpoint for every workspace",
    description = "For providers that hold one fixed endpoint (Stripe: `https://<api>/v1/payments/webhooks/stripe`). The `Stripe-Signature` header is checked on the raw body before anything is read; then only the Checkout Session or Refund id is taken. The workspace comes from our own payment row for that id, and the status is re-fetched from the provider. The answer is an empty 200 whatever the body holds (bad signature, unknown id, other events). At most 64 KiB.",
    params(("provider" = String, Path, description = "Provider name, e.g. `stripe`")),
    request_body(content = String, content_type = "application/json", description = "The provider's event, e.g. a Stripe event object"),
    responses((status = 200, description = "Received (empty body)")),
)]
pub async fn receive_any(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    payments::receive_any(&state, &provider, &headers, &body).await?;
    Ok(StatusCode::OK)
}
