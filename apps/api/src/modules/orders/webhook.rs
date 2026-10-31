//! HTTP edge for provider webhooks: `POST /v1/payments/webhooks/{provider}/{workspace}`. No
//! auth (providers can't hold our keys): the body only names an id, which is re-fetched from
//! the provider before anything changes. Always an empty 200.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
};

use crate::{AppState, error::AppError, modules::orders::payments};

/// POST /payments/webhooks/{provider}/{workspace}.
#[utoipa::path(
    post, path = "/payments/webhooks/{provider}/{workspace}", tag = "payments",
    operation_id = "payments_webhook", summary = "Payment provider status-changed call",
    description = "The body names a payment or refund id in the provider's format (the in-process test provider: `id=<id>`, form encoded). The status is always re-fetched from the provider; the answer is an empty 200 whatever the body names. At most 1 KiB.",
    params(("provider" = String, Path, description = "Provider name, e.g. `fake`"), ("workspace" = Uuid, Path, description = "Workspace id")),
    request_body(content = String, content_type = "application/x-www-form-urlencoded", description = "`id=<payment or refund id>`"),
    responses((status = 200, description = "Received (empty body)")),
)]
pub async fn receive(
    State(state): State<AppState>,
    Path((provider, workspace)): Path<(String, String)>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    payments::receive(&state, &provider, &workspace, &body).await?;
    Ok(StatusCode::OK)
}
