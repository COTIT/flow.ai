//! HTTP edge for email verification: browser link redirects, API resend returns JSON.

use axum::{
    extract::{Path, State},
    response::Redirect,
};

use crate::{
    AppState,
    error::AppError,
    http::{auth::AuthUser, response::Message},
    modules::email_verification::service,
};

/// GET /v1/verify-email/{token} -> 303 to vimdesk-web login with `email_verified=1|0`.
/// A browser opens this from the email, so failures redirect too instead of showing JSON.
pub async fn verify(State(state): State<AppState>, Path(token): Path<String>) -> Redirect {
    let ok = match service::verify(&state, &token).await {
        Ok(ok) => ok,
        Err(err) => {
            tracing::error!(error = %err, "email verification failed");
            false
        }
    };
    let flag = if ok { 1 } else { 0 };
    Redirect::to(&format!(
        "{}/login?email_verified={flag}",
        state.frontend_url
    ))
}

/// POST /v1/email/verification-notification -> 200 `{"message":"ok"}`.
pub async fn resend(State(state): State<AppState>, user: AuthUser) -> Result<Message, AppError> {
    service::resend(&state, user.user_id).await?;
    Ok(Message("ok"))
}
