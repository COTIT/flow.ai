//! Invoice routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::extract::DefaultBodyLimit;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::invoices::handlers};

/// Like the item-total routes: a body carries up to `pricing::body::MAX_ITEMS` full rows.
const BODY_LIMIT: usize = 2 * 1024 * 1024;
/// Send and resend take the `/send-message` body.
const SEND_LIMIT: usize = 1024 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    let send = OpenApiRouter::new()
        .routes(routes!(handlers::send))
        .routes(routes!(handlers::resend))
        .layer(DefaultBodyLimit::max(SEND_LIMIT));
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::duplicate))
        .routes(routes!(handlers::preview))
        .merge(send)
        .routes(routes!(handlers::credit))
        .routes(routes!(handlers::send_tasks))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

/// The public payment page link; `build_app` puts it behind the IP rate limit.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::public))
}
