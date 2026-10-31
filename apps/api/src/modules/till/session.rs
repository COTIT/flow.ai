//! TillSession extractor: `Authorization: Bearer <till token>` -> live till session, else 401.
//! A session of another workspace than the `{workspace}` path segment -> 404.

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{
    AppState,
    error::AppError,
    http::{auth, slots::Slot, workspace::path_workspace},
    modules::{auth::token, till::repo},
};

/// A till user logged in on a register. Put it in a handler's args to require one.
pub struct TillSession {
    pub row: repo::SessionRow,
    /// Same in-flight cap as members, with the till user in the user's place.
    _slot: Slot,
}

impl FromRequestParts<AppState> for TillSession {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let plain = auth::bearer(&parts.headers).ok_or(AppError::Unauthorized)?;
        let row = repo::live_session(&state.db, &token::hash(plain))
            .await?
            .ok_or(AppError::Unauthorized)?;
        if path_workspace(parts, state).await? != row.workspace_id {
            return Err(AppError::NotFound);
        }
        let slot = state.slots.take(row.till_user_id, row.workspace_id).await?;
        Ok(Self { row, _slot: slot })
    }
}
