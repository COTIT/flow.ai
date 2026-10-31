//! Deal routes, mounted under /v1 (`deals`, plus one board column under `deal-pipelines`).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::deals::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/deals";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{deal}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
        .route(
            &format!("{base}/{{deal}}/update-status"),
            post(handlers::move_to),
        )
        .route(
            &format!("{base}/{{deal}}/add-relation"),
            post(handlers::add_relation),
        )
        .route(
            "/workspace/{workspace}/deal-pipelines/{pipeline}/status/{status}/deals",
            get(handlers::column),
        )
}
