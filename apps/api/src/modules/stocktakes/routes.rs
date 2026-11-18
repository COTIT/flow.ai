//! Stocktake routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::stocktakes::handlers};

/// Routes for this module; each handler runs the workspace permission guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get))
        .routes(routes!(handlers::start))
        .routes(routes!(handlers::count))
        .routes(routes!(handlers::post_review))
        .routes(routes!(handlers::post))
        .routes(routes!(handlers::cancel))
}
