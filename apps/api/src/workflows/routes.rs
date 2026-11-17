//! Workflow routes (executions, incidents). Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, workflows::handlers};

/// Each handler checks its integration permission; four-eyes runs in the service.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list))
        .routes(routes!(handlers::get))
        .routes(routes!(handlers::retry))
        .routes(routes!(handlers::resolve))
        .routes(routes!(handlers::incidents))
        .routes(routes!(handlers::resolve_incident))
}
