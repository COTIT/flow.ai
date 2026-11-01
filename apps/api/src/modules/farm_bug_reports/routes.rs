//! Bug report routes. Paths live in each handler's `#[utoipa::path]`.

use axum::extract::DefaultBodyLimit;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    modules::farm_bug_reports::{dto::MAX_BYTES, handlers},
};

/// Two 5 MB images plus form overhead; other routes keep the global limit.
const UPLOAD_LIMIT: usize = 2 * MAX_BYTES + 64 * 1024;

/// Mounted with the large-body routes (twice the body deadline).
pub fn private() -> OpenApiRouter<AppState> {
    let upload = OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .layer(DefaultBodyLimit::max(UPLOAD_LIMIT));
    OpenApiRouter::new()
        .merge(upload)
        .routes(routes!(handlers::get))
}
