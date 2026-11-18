//! Sales return routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::sales_returns::handlers};

/// Routes for this module; each handler runs the workspace permission guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            handlers::sales_returns_list,
            handlers::sales_returns_create
        ))
        .routes(routes!(
            handlers::sales_returns_get,
            handlers::sales_returns_update
        ))
        .routes(routes!(handlers::sales_returns_cancel))
        .routes(routes!(handlers::sales_returns_post_review))
        .routes(routes!(handlers::sales_returns_post))
        .routes(routes!(handlers::sales_returns_restock_review))
        .routes(routes!(handlers::sales_returns_restock))
        .routes(routes!(handlers::sales_returns_reverse_review))
        .routes(routes!(handlers::sales_returns_reverse))
        .routes(routes!(handlers::sales_return_restocks_reverse_review))
        .routes(routes!(handlers::sales_return_restocks_reverse))
        .routes(routes!(handlers::legacy_return_costs_create))
}
