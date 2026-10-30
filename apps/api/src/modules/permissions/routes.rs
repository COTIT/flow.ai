//! Permission routes, mounted under /v1.

use axum::{Router, routing::get};

use crate::{AppState, modules::permissions::handlers};

/// Workspace members only (checked by `WorkspaceAccess`).
pub fn private() -> Router<AppState> {
    Router::new().route("/workspace/{workspace}/permissions", get(handlers::catalog))
}
