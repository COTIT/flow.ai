//! Stock routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.
// Paths match vimdesk-api.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::stock::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list))
        .routes(routes!(handlers::negative))
        .routes(routes!(handlers::mutate, handlers::delete))
        .routes(routes!(handlers::set_levels))
        .routes(routes!(handlers::set_requirements))
        .routes(routes!(handlers::move_between_bins))
        .routes(routes!(handlers::with_bins))
        .routes(routes!(handlers::configured_bins))
        .routes(routes!(handlers::movements))
}
