//! Catalogue routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_catalogue::handlers};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            handlers::list_categories,
            handlers::create_category
        ))
        .routes(routes!(
            handlers::list_equipment,
            handlers::create_equipment
        ))
        .routes(routes!(handlers::get_equipment))
}
