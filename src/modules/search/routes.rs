//! Search route, mounted under /v1 (path matches vimdesk-api).

use axum::{Router, routing::get};

use crate::{AppState, modules::search::handlers};

pub fn private() -> Router<AppState> {
    Router::new().route("/workspace/{workspace}/searches", get(handlers::search))
}
