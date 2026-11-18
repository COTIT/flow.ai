//! Privileged Till return and refund routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::till_refunds::handlers};

/// Member bearer + `X-Till-Session` routes; each handler checks `cashregister.refund`.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::till_sales_return_review))
        .routes(routes!(handlers::till_sales_return))
        .routes(routes!(handlers::till_refunds_create_review))
        .routes(routes!(
            handlers::till_refunds_list,
            handlers::till_refunds_create
        ))
        .routes(routes!(handlers::till_refunds_get))
}
