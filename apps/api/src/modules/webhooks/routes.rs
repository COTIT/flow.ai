//! Webhook endpoint routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::webhooks::handlers};

/// Management routes; each handler runs the workspace guard and the admin check.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::deliveries))
}
