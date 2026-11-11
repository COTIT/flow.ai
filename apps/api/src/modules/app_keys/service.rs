//! App key rules: secret shown once, revoke is idempotent. Routes take `Can<WorkspaceAdmin>`:
//! keys open the workspace's data to an app, so only the owner or a `workspace_admin` manages them.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    modules::app_keys::{
        dto::NewAppKey,
        repo::{self, KeyRow, NewKey},
        secret,
    },
    modules::auth::token::PlainSecret,
};

/// Issue a key: the stored row and the plaintext secret.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    input: NewAppKey,
) -> Result<(KeyRow, PlainSecret), AppError> {
    let (plain, hash, prefix) = secret::generate()?;
    let row = repo::insert(
        &state.db,
        &NewKey {
            workspace_id: access.workspace_id,
            name: &input.name,
            kind: &input.kind,
            scopes: &input.scopes,
            key_hash: &hash,
            prefix: &prefix,
            created_by: Some(access.user_id),
        },
    )
    .await?;
    Ok((row, plain))
}

/// One page of this workspace's keys, newest first, with the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<KeyRow>, i64), AppError> {
    Ok(repo::list(&state.db, workspace_id, limit, offset).await?)
}

/// Revoke a key of this workspace (again is fine), else 404.
pub async fn revoke(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let managed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM storefront_connections WHERE workspace_id=$1 AND id=$2)",
    )
    .bind(workspace_id)
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    if managed {
        return Err(AppError::Conflict(
            "This key belongs to a Shop connection. Disconnect the Shop instead.",
        ));
    }
    repo::revoke(&state.db, workspace_id, id)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}
