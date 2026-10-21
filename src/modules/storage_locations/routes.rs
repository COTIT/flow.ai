//! Storage location routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::{AppState, modules::storage_locations::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/erp/storage-locations";
    Router::new()
        .route(base, post(handlers::create))
        .route(&format!("{base}/{{id}}"), delete(handlers::delete))
        .route(
            &format!("{base}/{{domain}}/{{parent}}"),
            get(handlers::list),
        )
}
