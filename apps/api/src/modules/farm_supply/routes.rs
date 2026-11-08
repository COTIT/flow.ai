//! Supply batch routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_supply::handlers};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::options))
        .routes(routes!(handlers::dispatch_options))
        .routes(routes!(handlers::quiet_farmers))
        .routes(routes!(handlers::raw_product))
        .routes(routes!(handlers::get))
}
