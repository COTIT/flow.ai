//! Customer deposit routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::customer_deposits::handlers};

/// Routes for this module; each handler runs the workspace permission guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::customer_deposits_receive_review))
        .routes(routes!(handlers::customer_deposits_receive))
        .routes(routes!(handlers::customer_deposits_apply_review))
        .routes(routes!(handlers::customer_deposits_apply))
}
