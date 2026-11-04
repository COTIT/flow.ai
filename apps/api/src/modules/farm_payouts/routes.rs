//! Payout desk and cash routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_payouts::handlers};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::pay, handlers::list))
        .routes(routes!(handlers::get))
        .routes(routes!(handlers::void))
        .routes(routes!(handlers::move_cash))
        .routes(routes!(handlers::reconciliation))
}
