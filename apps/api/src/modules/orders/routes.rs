//! Order routes. Paths live in each handler's `#[utoipa::path]`.

use axum::extract::DefaultBodyLimit;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    modules::orders::{handlers, storefront, webhook},
};

/// Office routes; each handler runs the workspace guard and its `order.*` permission.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
        .routes(routes!(handlers::confirm))
        .routes(routes!(handlers::pick))
        .routes(routes!(handlers::complete))
        .routes(routes!(handlers::cancel))
        .routes(routes!(handlers::returns))
        .routes(routes!(handlers::invoice))
        .routes(routes!(handlers::refund))
}

/// Web shop routes (`/storefront/orders`); each handler checks the key's `checkout:write`.
pub fn storefront() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(storefront::create))
        .routes(routes!(storefront::get))
        .routes(routes!(storefront::payment))
}

/// Provider webhooks (no auth; `build_app` puts them behind the IP rate limit). A status call
/// is a short id: 1 KiB is plenty.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(webhook::receive))
        .layer(DefaultBodyLimit::max(1024))
}
