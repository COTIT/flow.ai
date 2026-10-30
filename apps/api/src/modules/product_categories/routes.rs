//! Category routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, routing::get};

use crate::{AppState, modules::product_categories::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/erp/product-categories";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{category}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
}
