//! Shipment method routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::{Router, routing::get};

use crate::{AppState, modules::shipment_methods::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/shipment-methods";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{shipment_method}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
}
