//! Relation routes, mounted under /v1 (paths match vimdesk-api, plus the trailing-slash create
//! vimdesk-web and the POS send).
//! Paths live in each handler's `#[utoipa::path]`, so router and OpenAPI spec share one source.

use axum::routing::post;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::relations::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        // Old clients post to `/relations/`; same handler, left out of the spec.
        .route("/workspace/{workspace}/relations/", post(handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::financial_summary))
}
