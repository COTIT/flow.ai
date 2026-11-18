//! PrivilegedTill extractor (D14): member bearer in `Authorization` + live till token in
//! `X-Till-Session`. Member authority comes only from the member login; the till session gives
//! register + cashier context. Till PIN or a till user's member link never counts as login.

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, request::Parts},
};

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    modules::{auth::token, till::repo},
};

/// Header carrying the till session token on privileged routes. Secret: never log it.
pub const TILL_SESSION_HEADER: &str = "x-till-session";

/// Active member of the path workspace plus a live till session of the same workspace.
pub struct PrivilegedTill {
    /// The approving member; permissions read live this request.
    pub member: WorkspaceAccess,
    /// Register and cashier the approval happens on.
    pub till: repo::SessionRow,
}

impl FromRequestParts<AppState> for PrivilegedTill {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        // Member first: `access_tokens` only, so a till token here is 401; outsider 404.
        let member = WorkspaceAccess::from_request_parts(parts, state).await?;
        let plain = till_token(&parts.headers).ok_or(AppError::Unauthorized)?;
        // Same query as TillSession: ended, expired, rotated/revoked key, inactive user -> None.
        let till = repo::live_session(&state.db, &token::hash(plain))
            .await?
            .ok_or(AppError::Unauthorized)?;
        if till.workspace_id != member.workspace_id {
            return Err(AppError::NotFound);
        }
        Ok(Self { member, till })
    }
}

/// Bare till token from `X-Till-Session`; malformed -> None, no DB round trip.
fn till_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(TILL_SESSION_HEADER)?.to_str().ok()?.trim();
    token::looks_valid(value).then_some(value)
}
