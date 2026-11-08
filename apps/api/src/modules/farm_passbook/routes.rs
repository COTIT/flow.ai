//! Passbook and consent routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_passbook::handlers};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::mine))
        .routes(routes!(handlers::balances))
        .routes(routes!(handlers::of_person))
        .routes(routes!(
            handlers::get_consent,
            handlers::give_consent,
            handlers::withdraw_consent
        ))
}
