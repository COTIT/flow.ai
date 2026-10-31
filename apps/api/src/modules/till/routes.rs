//! Till routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::till::handlers};

/// Office routes (member bearer + `cashregister.*`), device routes (register device key) and
/// till routes (till session token).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list_registers, handlers::create_register))
        .routes(routes!(handlers::rotate_key))
        .routes(routes!(handlers::list_users, handlers::create_user))
        .routes(routes!(handlers::update_user))
        .routes(routes!(handlers::login_users))
        .routes(routes!(handlers::login))
        .routes(routes!(handlers::logout))
        .routes(routes!(handlers::me))
}
