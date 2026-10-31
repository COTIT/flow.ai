//! Special offer routes, mounted under /v1. The product routes see only that product's
//! offers; the global routes see every offer. Paths live in each handler's `#[utoipa::path]`.
// Paths match vimdesk-api; update is POST.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::special_offers::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::list_all, handlers::create_all))
        .routes(routes!(
            handlers::get_all,
            handlers::update_all,
            handlers::delete_all
        ))
}
