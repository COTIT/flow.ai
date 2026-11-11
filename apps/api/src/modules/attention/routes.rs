//! Attention route, mounted under /v1. Path lives in the handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::attention::handlers};

/// Routes for this module; the handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::summary))
        .routes(routes!(handlers::priorities))
        .routes(routes!(handlers::set_mark))
        .routes(routes!(handlers::delete_mark))
}
