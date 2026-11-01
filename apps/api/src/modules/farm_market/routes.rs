//! Market routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_market::handlers};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list_buyers, handlers::create_buyer))
        .routes(routes!(handlers::list_demands, handlers::create_demand))
        .routes(routes!(handlers::get_demand))
        .routes(routes!(handlers::propose))
        .routes(routes!(handlers::list_opportunities))
        .routes(routes!(handlers::get_opportunity))
        .routes(routes!(handlers::set_status))
        .routes(routes!(handlers::attach))
        .routes(routes!(handlers::available))
        .routes(routes!(handlers::village_supply))
        .routes(routes!(handlers::demand_matches))
        .routes(routes!(handlers::village_matches))
}
