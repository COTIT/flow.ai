//! Invoice routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::{AppState, modules::invoices::handlers};

/// Like the item-total routes: a body carries up to `pricing::body::MAX_ITEMS` full rows.
const BODY_LIMIT: usize = 2 * 1024 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/invoices";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{invoice}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
        .route(
            &format!("{base}/{{invoice}}/duplicate"),
            post(handlers::duplicate),
        )
        .route(
            &format!("{base}/{{invoice}}/preview"),
            get(handlers::preview),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}
