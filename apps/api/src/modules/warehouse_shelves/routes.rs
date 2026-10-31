//! Shelf routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.
// Paths match vimdesk-api.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::warehouse_shelves::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
}
