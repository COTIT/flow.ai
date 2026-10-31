//! Deal pipeline routes, mounted under /v1 (`deal-pipelines`, as in vimdesk-api). Paths live in
//! each handler's `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::deal_pipelines::handlers};

/// Routes for this module; each handler checks its `dealpipeline*` permission.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update))
        .routes(routes!(handlers::create_status))
        .routes(routes!(handlers::update_status, handlers::delete_status))
}
