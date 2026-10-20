//! Team routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::teams::handlers};

/// Workspace members only (`WorkspaceAccess`); each handler checks its `team.*` permission.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/users/teams";
    Router::new()
        .route(base, post(handlers::create).get(handlers::list))
        .route(
            &format!("{base}/{{team}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
        .route(
            &format!("{base}/{{team}}/add-users"),
            post(handlers::add_users),
        )
        .route(
            &format!("{base}/{{team}}/remove-users"),
            post(handlers::remove_users),
        )
        .route(
            &format!("{base}/{{team}}/permissions"),
            post(handlers::permissions),
        )
}
