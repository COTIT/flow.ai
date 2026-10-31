//! Deal routes, mounted under /v1 (`deals`, plus one board column under `deal-pipelines`). Paths
//! live in each handler's `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::deals::handlers};

/// Routes for this module; each handler checks its `deal.*` permission.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::move_to))
        .routes(routes!(handlers::add_relation))
        .routes(routes!(handlers::column))
}
