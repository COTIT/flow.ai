//! Import routes under /v1; paths live on each handler's `#[utoipa::path]`.
use utoipa_axum::{router::OpenApiRouter, routes};

use super::handlers;
use crate::AppState;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get))
        .routes(routes!(handlers::upload))
        .routes(routes!(handlers::validate))
        .routes(routes!(handlers::apply))
        .routes(routes!(handlers::rows))
}
