//! Custom status routes, mounted under /v1 (singular `custom-status`, as in vimdesk-api).

use axum::{Router, routing::get};

use crate::{AppState, modules::custom_statuses::handlers};

pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/custom-status";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{custom_status}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
}
