//! Purchasing routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::purchasing::handlers};

/// Routes for this module; each handler runs the workspace permission guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::orders_list, handlers::orders_create))
        .routes(routes!(handlers::orders_get, handlers::orders_update))
        .routes(routes!(handlers::orders_confirm_review))
        .routes(routes!(handlers::orders_confirm))
        .routes(routes!(handlers::orders_close_lines))
        .routes(routes!(handlers::orders_cancel))
        .routes(routes!(handlers::orders_events))
        .routes(routes!(handlers::receipts_list, handlers::receipts_create))
        .routes(routes!(handlers::receipts_get))
        .routes(routes!(handlers::receipts_book_review))
        .routes(routes!(handlers::receipts_book))
}
