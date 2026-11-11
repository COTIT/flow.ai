//! Home design routes; staff and published reads retain separate extractors.

use super::handlers;
use crate::AppState;
use utoipa_axum::{router::OpenApiRouter, routes};

/// Design endpoints; each handler declares authenticated identity and permission.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::get))
        .routes(routes!(handlers::save))
        .routes(routes!(handlers::revisions))
        .routes(routes!(handlers::preview))
        .routes(routes!(handlers::restore))
        .routes(routes!(handlers::publish))
        .routes(routes!(handlers::edit))
        .routes(routes!(handlers::edit_preview))
        .routes(routes!(handlers::capabilities))
        .routes(routes!(handlers::published))
        .routes(routes!(
            handlers::preview_link_create,
            handlers::preview_links
        ))
        .routes(routes!(handlers::preview_link_revoke))
}

/// Preview-token resolve; `build_app` gives it its own per-app-key limit.
pub fn previews() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::preview_resolve))
}
