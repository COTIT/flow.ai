//! Warehouse location routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, routing::get};

use crate::{AppState, modules::warehouse_locations::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/erp/warehouses/{warehouse}/locations";
    Router::new()
        .route(
            base,
            get(handlers::list)
                .post(handlers::create)
                .delete(handlers::delete_many),
        )
        .route(
            &format!("{base}/{{location}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
}
