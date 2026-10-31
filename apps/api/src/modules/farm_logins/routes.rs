//! Farmer login routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_logins::handlers};

/// Sign-in: no token, behind the IP rate limit.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::login))
}

/// Officer issue/history (farm guard) and the farmer's password change (any signed-in account).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::issue))
        .routes(routes!(handlers::history))
        .routes(routes!(handlers::change_password))
}
