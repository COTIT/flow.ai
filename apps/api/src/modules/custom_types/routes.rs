//! Custom type routes, mounted under /v1 (paths match vimdesk-api; update is POST).
//! Paths live in each handler's `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::custom_types::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
}
