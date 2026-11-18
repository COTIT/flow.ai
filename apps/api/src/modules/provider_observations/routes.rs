//! Accounting exception routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::provider_observations::handlers};

/// Routes for this module; each handler runs the workspace permission guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::accounting_exceptions_list))
        .routes(routes!(handlers::accounting_exceptions_get))
        .routes(routes!(handlers::accounting_exceptions_retry))
        .routes(routes!(handlers::accounting_exceptions_resolve_review))
        .routes(routes!(handlers::accounting_exceptions_resolve))
}
