//! /v1/me + workspace user routes. All need a Bearer token (`AuthUser` / `WorkspaceAccess`).
//! Paths live in each handler's `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::users::handlers};

/// Bearer token required (checked by each handler's extractor).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::me))
        .routes(routes!(handlers::workspaces))
        .routes(routes!(handlers::members))
        .routes(routes!(handlers::member))
}
