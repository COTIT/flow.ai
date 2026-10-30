//! Workspace record routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::workspaces::handlers};

/// Workspace members with `workspace.get` / `workspace.update` (`Can`).
pub fn private() -> Router<AppState> {
    Router::new()
        .route("/workspace/{workspace}", get(handlers::get))
        .route("/workspace/{workspace}/update", post(handlers::update))
}
