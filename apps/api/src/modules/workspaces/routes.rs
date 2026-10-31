//! Workspace record routes, mounted under /v1 (paths match vimdesk-api). Paths live in each
//! handler's `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::workspaces::handlers};

/// Workspace members with `workspace.get` / `workspace.update` (`Can`).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::get))
        .routes(routes!(handlers::update))
}
