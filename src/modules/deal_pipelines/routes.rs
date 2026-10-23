//! Deal pipeline routes, mounted under /v1 (`deal-pipelines`, as in vimdesk-api).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::deal_pipelines::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/deal-pipelines";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{pipeline}}"),
            get(handlers::get).post(handlers::update),
        )
        .route(
            &format!("{base}/{{pipeline}}/status"),
            post(handlers::create_status),
        )
        .route(
            &format!("{base}/{{pipeline}}/status/{{status}}"),
            post(handlers::update_status).delete(handlers::delete_status),
        )
}
