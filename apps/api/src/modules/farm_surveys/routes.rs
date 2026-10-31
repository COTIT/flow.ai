//! Survey routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_surveys::handlers as h};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(h::list, h::create))
        .routes(routes!(h::eligibility))
        .routes(routes!(h::get, h::update))
        .routes(routes!(h::add_question))
        .routes(routes!(h::replace_question, h::remove_question))
        .routes(routes!(h::publish))
        .routes(routes!(h::close))
        .routes(routes!(h::answer))
        .routes(routes!(h::results))
        .routes(routes!(h::vouchers))
}
