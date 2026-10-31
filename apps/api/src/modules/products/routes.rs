//! Product routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.
// Paths match vimdesk-api. The static `/search` segment wins over `/{product}`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::products::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::search))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::duplicate))
        .routes(routes!(handlers::price_history))
        .routes(routes!(handlers::sale_statics))
        .routes(routes!(handlers::linked_products))
        .routes(routes!(handlers::sync_external))
}
