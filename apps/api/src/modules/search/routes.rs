//! Search route, mounted under /v1 (path matches vimdesk-api). Path lives in the handler's
//! `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::search::handlers};

/// Routes for this module; the handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::search))
        .routes(routes!(handlers::quick))
}
