//! Settings routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::{Router, routing::get};

use crate::{AppState, modules::settings::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    Router::new().route(
        "/workspace/{workspace}/settings",
        get(handlers::get).post(handlers::update),
    )
}
