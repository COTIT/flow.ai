//! Product routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::products::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/products";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        // Static segment: wins over `/{product}` (Laravel registered it first too).
        .route(&format!("{base}/search"), post(handlers::search))
        .route(
            &format!("{base}/{{product}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
        .route(
            &format!("{base}/{{product}}/duplicate"),
            post(handlers::duplicate),
        )
        .route(
            &format!("{base}/{{product}}/price-history"),
            get(handlers::price_history),
        )
        // Product-page companions with a truthful answer in genesy (docs/api/products.md).
        .route(
            &format!("{base}/{{product}}/sale-statics"),
            get(handlers::sale_statics),
        )
        .route(
            &format!("{base}/{{product}}/linked-products"),
            get(handlers::linked_products),
        )
        .route(
            &format!("{base}/{{product}}/sync-product-stock-locations-from-external-service"),
            post(handlers::sync_external),
        )
}
