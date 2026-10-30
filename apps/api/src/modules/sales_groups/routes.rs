//! Sales group routes, mounted under /v1 (paths match vimdesk-api, including `/settings/`).

use axum::{Router, routing::get};

use crate::{AppState, modules::sales_groups::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/settings/sales-groups";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{sales_group}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
}
