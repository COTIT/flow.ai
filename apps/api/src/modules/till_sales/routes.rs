//! Till sale routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::till_sales::handlers};

/// Till routes (session token) to sell, quote and look a sale up; the office list (`cashregister.list`).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::sell))
        .routes(routes!(handlers::quote))
        .routes(routes!(handlers::get))
}
