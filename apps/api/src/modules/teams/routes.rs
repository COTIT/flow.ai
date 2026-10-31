//! Team routes, mounted under /v1 (paths match vimdesk-api). Paths live in each handler's
//! `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::teams::handlers};

/// Workspace members only (`WorkspaceAccess`); each handler checks its `team.*` permission.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::create, handlers::list))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::add_users))
        .routes(routes!(handlers::remove_users))
        .routes(routes!(handlers::permissions))
}
