//! Shipment routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::shipments::handlers};

/// Routes for this module; each handler runs the workspace permission guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::shipments_list, handlers::shipments_create))
        .routes(routes!(handlers::shipments_get, handlers::shipments_update))
        .routes(routes!(handlers::shipments_post_review))
        .routes(routes!(handlers::shipments_post))
        .routes(routes!(handlers::shipments_cancel))
}
