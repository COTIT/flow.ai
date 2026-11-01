//! Farm record routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::farm_records::handlers as h};

/// Every handler runs the farm guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(h::register))
        .routes(routes!(h::list_persons))
        .routes(routes!(h::get_person, h::correct_person))
        .routes(routes!(h::verify_person))
        .routes(routes!(h::holdings))
        .routes(routes!(h::list_households))
        .routes(routes!(h::get_household, h::correct_household))
        .routes(routes!(h::verify_household))
        .routes(routes!(h::list_farms))
        .routes(routes!(h::get_farm, h::correct_farm))
        .routes(routes!(h::verify_farm))
        .routes(routes!(h::list_plots))
        .routes(routes!(h::get_plot, h::correct_plot))
        .routes(routes!(h::verify_plot))
        .routes(routes!(h::list_cycles))
        .routes(routes!(h::get_cycle, h::correct_cycle))
        .routes(routes!(h::verify_cycle))
        .routes(routes!(h::list_harvests, h::revise_harvest))
        .routes(routes!(h::list_all_harvests))
        .routes(routes!(h::verify_harvest))
}
