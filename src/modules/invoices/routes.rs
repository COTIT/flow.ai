//! Invoice routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::{AppState, modules::invoices::handlers};

/// Like the item-total routes: a body carries up to `pricing::body::MAX_ITEMS` full rows.
const BODY_LIMIT: usize = 2 * 1024 * 1024;
/// Send and resend take the `/send-message` body.
const SEND_LIMIT: usize = 1024 * 1024;

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
        .route(
            &format!("{base}/{{invoice}}/send"),
            post(handlers::send).layer(DefaultBodyLimit::max(SEND_LIMIT)),
        )
        .route(
            &format!("{base}/{{invoice}}/resend"),
            post(handlers::resend).layer(DefaultBodyLimit::max(SEND_LIMIT)),
        )
        .route(
            &format!("{base}/{{invoice}}/credit"),
            post(handlers::credit),
        )
        .route(
            &format!("{base}/send-tasks/active"),
            get(handlers::send_tasks),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

/// No token: the payment page's link. lib.rs puts it behind the rate limit.
pub fn public() -> Router<AppState> {
    Router::new().route("/invoices/fetch-from-hashed/{token}", get(handlers::public))
}
