//! File routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, extract::DefaultBodyLimit, routing::get};

use crate::{AppState, modules::files::handlers};

/// A 10 MB file as base64 in JSON, plus the other fields.
const BODY_LIMIT: usize = 14 * 1024 * 1024 + 64 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/files";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{file}}"),
            get(handlers::get).delete(handlers::delete),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}
