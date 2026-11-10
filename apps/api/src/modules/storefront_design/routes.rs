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
}
