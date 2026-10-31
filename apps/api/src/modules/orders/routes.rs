//! Order routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::orders::handlers};

/// Office routes; each handler runs the workspace guard and its `order.*` permission.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::confirm))
        .routes(routes!(handlers::pick))
        .routes(routes!(handlers::complete))
        .routes(routes!(handlers::cancel))
}
