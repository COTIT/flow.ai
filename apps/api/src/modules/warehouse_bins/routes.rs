//! Bin routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, routing::get};

use crate::{AppState, modules::warehouse_bins::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows/{row}\
                /shelves/{shelf}/bins";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{bin}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
        .route(
            "/workspace/{workspace}/warehouses/{warehouse}/bins",
            get(handlers::by_warehouse),
        )
}
