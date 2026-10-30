//! Variant routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::product_variants::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/products/{product}/variants";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(&format!("{base}/generate"), post(handlers::generate))
        .route(
            &format!("{base}/{{variant}}"),
            get(handlers::get)
                .put(handlers::update)
                .delete(handlers::delete),
        )
}
