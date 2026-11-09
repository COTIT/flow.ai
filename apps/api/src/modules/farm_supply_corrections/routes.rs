//! Supply correction routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_supply_corrections::handlers};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::create))
        .routes(routes!(handlers::list))
        .routes(routes!(handlers::get))
        .routes(routes!(handlers::approve))
        .routes(routes!(handlers::reject))
}
