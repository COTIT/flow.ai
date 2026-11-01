//! Lot routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::lots::handlers};

/// Office routes (workspace guard; writes need `product.update`) and the storefront mount of
/// the public page (app key with `catalog:read`).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::get, handlers::update))
        .routes(routes!(handlers::events))
        .routes(routes!(handlers::farmer_credits))
        .routes(routes!(handlers::stock))
        .routes(routes!(handlers::passport_get, handlers::passport_save))
        .routes(routes!(
            handlers::passport_publish,
            handlers::passport_unpublish
        ))
        .routes(routes!(handlers::storefront_get))
        .routes(routes!(handlers::stock_mutate))
}

/// The lot's public page; `build_app` puts it behind the IP rate limit.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::public_get))
}
