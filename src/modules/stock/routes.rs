//! Stock routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::stock::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/products";
    let product = format!("{base}/{{product}}");
    Router::new()
        .route(&format!("{base}/stocks"), get(handlers::list))
        .route(&format!("{base}/stocks/negative"), get(handlers::negative))
        .route(
            &format!("{product}/stocks"),
            post(handlers::mutate).delete(handlers::delete),
        )
        .route(
            &format!("{product}/set-stock-levels"),
            post(handlers::set_levels),
        )
        .route(
            &format!("{product}/set-stock-requirements"),
            post(handlers::set_requirements),
        )
        .route(
            &format!("{product}/move-stock-between-bins"),
            post(handlers::move_between_bins),
        )
        .route(
            &format!("{product}/stock-with-bins"),
            get(handlers::with_bins),
        )
        .route(
            &format!("{product}/configured-bins"),
            get(handlers::configured_bins),
        )
        .route(
            &format!("{product}/stock-movements"),
            get(handlers::movements),
        )
}
