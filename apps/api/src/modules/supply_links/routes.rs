//! Supply link routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::supply_links::handlers};

/// Workspace admins only; checked before the body is read.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::grant))
        .routes(routes!(handlers::accept))
        .routes(routes!(handlers::revoke))
}
