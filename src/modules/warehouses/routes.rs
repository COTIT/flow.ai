//! Warehouse routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::warehouses::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/warehouses";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{warehouse}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
        .route(
            &format!("{base}/{{warehouse}}/default"),
            post(handlers::set_default),
        )
}
