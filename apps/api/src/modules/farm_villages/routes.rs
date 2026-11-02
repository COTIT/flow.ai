//! Village routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_villages::handlers};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::update))
        .routes(routes!(handlers::add_capacity))
}
