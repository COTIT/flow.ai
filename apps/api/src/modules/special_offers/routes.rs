//! Special offer routes, mounted under /v1 (paths match vimdesk-api; update is POST). The
//! product routes see only that product's offers; the global routes see every offer.

use axum::{Router, routing::get};

use crate::{AppState, modules::special_offers::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let product = "/workspace/{workspace}/erp/products/{product}/special-offers";
    let global = "/workspace/{workspace}/erp/special-offers";
    let mut router = Router::new();
    for base in [product, global] {
        router = router
            .route(base, get(handlers::list).post(handlers::create))
            .route(
                &format!("{base}/{{offer}}"),
                get(handlers::get)
                    .post(handlers::update)
                    .delete(handlers::delete),
            );
    }
    router
}
