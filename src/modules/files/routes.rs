//! File routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, extract::DefaultBodyLimit, routing::get};

use crate::{
    AppState,
    modules::files::{dto::MAX_BYTES, handlers},
};

/// The upload route's body: a 10 MB file plus a little form overhead (base64 JSON then holds
/// at most ~7.5 MB of file). Other routes keep the global limit.
const UPLOAD_LIMIT: usize = MAX_BYTES + 64 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/files";
    Router::new()
        .route(
            base,
            get(handlers::list)
                .post(handlers::create)
                .layer(DefaultBodyLimit::max(UPLOAD_LIMIT)),
        )
        .route(
            &format!("{base}/{{file}}"),
            get(handlers::get).delete(handlers::delete),
        )
}
