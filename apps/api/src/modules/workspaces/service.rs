//! Workspace record rules: read with counts, merge a validated profile patch. No HTTP types.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::workspaces::{
        dto::ProfilePatch,
        repo::{self, Record},
    },
};

pub async fn record(state: &AppState, workspace_id: Uuid) -> Result<Record, AppError> {
    repo::record(&state.db, workspace_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Sent keys overwrite (null clears); `workspace_type` and the name never change here.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    patch: ProfilePatch,
) -> Result<(), AppError> {
    // Nothing sent: don't create an empty profile (it would start showing in /me/workspaces).
    if patch.profile.as_object().is_some_and(|m| m.is_empty()) && patch.meta.is_none() {
        return record(state, workspace_id).await.map(|_| ());
    }
    let meta = patch.meta.as_ref().map(Option::as_ref);
    let updated = repo::patch_profile(&state.db, workspace_id, &patch.profile, meta).await?;
    updated.then_some(()).ok_or(AppError::NotFound)
}
