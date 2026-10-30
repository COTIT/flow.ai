//! PDF template routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::{Router, extract::DefaultBodyLimit, routing::get};

use crate::{AppState, modules::pdf_templates::handlers};

/// Four rich-text texts of up to 64 KiB each plus the rest.
const BODY_LIMIT: usize = 512 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/settings/pdf-templates";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/available-fields/{{domain}}"),
            get(handlers::available_fields),
        )
        .route(
            &format!("{base}/{{template}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
        .route(
            &format!("{base}/{{template}}/preview"),
            get(handlers::preview),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}
