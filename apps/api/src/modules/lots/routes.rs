//! Lot routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::lots::handlers};

/// Office routes; each handler runs the workspace guard (writes: `product.update`).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update))
        .routes(routes!(handlers::events))
}
