//! Stock routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, routing::post};

use crate::{AppState, modules::stock::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/products";
    Router::new().route(
        &format!("{base}/{{product}}/stocks"),
        post(handlers::mutate),
    )
}
