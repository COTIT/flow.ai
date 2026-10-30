//! Shelf routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, routing::get};

use crate::{AppState, modules::warehouse_shelves::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base =
        "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{shelf}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
}
