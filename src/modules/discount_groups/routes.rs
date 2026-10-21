//! Discount group routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::discount_groups::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/products/discount-groups";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{group}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
        .route(
            &format!("{base}/{{group}}/relations"),
            get(handlers::relations),
        )
        .route(
            &format!("{base}/{{group}}/attach-to-relations"),
            post(handlers::attach),
        )
        .route(
            &format!("{base}/{{group}}/detach-from-relations"),
            post(handlers::detach),
        )
}
