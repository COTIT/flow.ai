//! Voucher routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_vouchers::handlers as h};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(h::mine))
        .routes(routes!(h::lookup))
        .routes(routes!(h::redeem))
        .routes(routes!(h::void))
        .routes(routes!(h::timeline))
        .routes(routes!(h::redemptions))
        .routes(routes!(h::totals))
}
