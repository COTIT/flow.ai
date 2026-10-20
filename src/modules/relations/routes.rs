//! Relation routes, mounted under /v1 (paths match vimdesk-api, plus the trailing-slash create
//! vimdesk-web and the POS send).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::relations::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/relations";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(&format!("{base}/"), post(handlers::create))
        .route(&format!("{base}/{{relation}}"), get(handlers::get))
}
