//! Permission routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`, so
//! router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::permissions::handlers};

/// Workspace members only (checked by `WorkspaceAccess`).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::catalog))
}
