//! /v1/me + workspace user routes. All need a Bearer token (`AuthUser` / `WorkspaceAccess`).

use axum::{Router, routing::get};

use crate::{AppState, modules::users::handlers};

pub fn private() -> Router<AppState> {
    Router::new()
        .route("/me", get(handlers::me))
        .route("/me/workspaces", get(handlers::workspaces))
        .route("/workspace/{workspace}/users", get(handlers::members))
        .route("/workspace/{workspace}/users/{user}", get(handlers::member))
}
