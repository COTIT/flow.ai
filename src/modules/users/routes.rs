//! /v1/me routes. All need a Bearer token (checked by `AuthUser`).

use axum::{Router, routing::get};

use crate::{AppState, modules::users::handlers};

pub fn private() -> Router<AppState> {
    Router::new()
        .route("/me", get(handlers::me))
        .route("/me/workspaces", get(handlers::workspaces))
}
