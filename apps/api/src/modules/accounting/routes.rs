//! Accounting routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::accounting::handlers};

/// Routes for this module; each handler runs the workspace permission guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::setup_get, handlers::setup_update))
        .routes(routes!(handlers::accounts_list, handlers::accounts_create))
        .routes(routes!(handlers::accounts_update))
        .routes(routes!(
            handlers::bank_accounts_list,
            handlers::bank_accounts_create
        ))
        .routes(routes!(handlers::years_list, handlers::years_create))
        .routes(routes!(handlers::periods_list, handlers::periods_create))
        .routes(routes!(handlers::journals_list, handlers::journals_create))
        .routes(routes!(handlers::journals_get, handlers::journals_update))
        .routes(routes!(handlers::journals_post_review))
        .routes(routes!(handlers::journals_post))
        .routes(routes!(handlers::journals_reverse_review))
        .routes(routes!(handlers::journals_reverse))
        .routes(routes!(handlers::trial_balance))
        .routes(routes!(handlers::general_ledger))
        .routes(routes!(handlers::profit_and_loss))
        .routes(routes!(handlers::balance_sheet))
}
