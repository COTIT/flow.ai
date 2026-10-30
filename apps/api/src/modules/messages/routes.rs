//! Mail routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::{AppState, modules::messages::handlers};

/// A mail body is the user's HTML (pasted signatures and images included).
const SEND_LIMIT: usize = 1024 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let ws = "/workspace/{workspace}";
    Router::new()
        .route(
            &format!("{ws}/send-message"),
            post(handlers::send_message).layer(DefaultBodyLimit::max(SEND_LIMIT)),
        )
        .route(&format!("{ws}/settings/email-logs"), get(handlers::logs))
        .route(
            &format!("{ws}/settings/office365/test-connection"),
            post(handlers::office365_connection),
        )
        .route(
            &format!("{ws}/settings/office365/test-send"),
            post(handlers::office365_send),
        )
        .route(
            &format!("{ws}/email-profiles/{{profile}}/test-connection"),
            get(handlers::profile_test),
        )
}
