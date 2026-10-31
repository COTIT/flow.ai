//! File routes, mounted under /v1 (paths match vimdesk-api).

use axum::extract::DefaultBodyLimit;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    modules::files::{dto::MAX_BYTES, handlers},
};

/// The upload route's body: a 10 MB file plus a little form overhead (base64 JSON then holds
/// at most ~7.5 MB of file). Other routes keep the global limit.
const UPLOAD_LIMIT: usize = MAX_BYTES + 64 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    let upload = OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .layer(DefaultBodyLimit::max(UPLOAD_LIMIT));
    OpenApiRouter::new()
        .merge(upload)
        .routes(routes!(handlers::get, handlers::delete))
}
