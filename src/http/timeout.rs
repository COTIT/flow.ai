//! Request time limits on `/v1`: the body must arrive within `body_deadline` (a slow upload
//! gets 408), the whole request must finish within `request_timeout` (503). Either way the
//! request's slots and pooled connection are released, and an open transaction rolls back.

use std::time::Duration;

use axum::{
    Router,
    extract::Request,
    middleware::{self, Next},
    response::IntoResponse,
};
use tower_http::timeout::{RequestBodyDeadlineLayer, TimeoutError};

use crate::{AppState, error::AppError};

/// Wrap `router` in both limits; the request timeout is the outer one.
pub fn apply(
    router: Router<AppState>,
    request_timeout: Duration,
    body_deadline: Duration,
) -> Router<AppState> {
    router
        .layer(RequestBodyDeadlineLayer::new(body_deadline))
        .layer(middleware::from_fn(
            move |req: Request, next: Next| async move {
                match tokio::time::timeout(request_timeout, next.run(req)).await {
                    Ok(res) => res,
                    Err(_) => {
                        tracing::warn!(?request_timeout, "request timed out");
                        AppError::Overloaded.into_response()
                    }
                }
            },
        ))
}

/// True when `e` (or an error under it) is the body deadline firing.
pub fn is_body_timeout(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut cur = Some(e);
    while let Some(err) = cur {
        if err.is::<TimeoutError>() {
            return true;
        }
        cur = err.source();
    }
    false
}
