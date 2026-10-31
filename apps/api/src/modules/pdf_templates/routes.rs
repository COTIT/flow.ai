//! PDF template routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::extract::DefaultBodyLimit;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::pdf_templates::handlers};

/// Four rich-text texts of up to 64 KiB each plus the rest.
const BODY_LIMIT: usize = 512 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::available_fields))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::preview))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}
