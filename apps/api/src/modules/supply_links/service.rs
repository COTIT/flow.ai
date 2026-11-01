//! Supply link rules: a workspace admin grants to another live workspace; only the receiving
//! workspace's admin accepts a pending link; either side's admin revokes, and a revoked link
//! stays revoked. One open link per pair. Workspaces outside a link never see it.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    modules::supply_links::{
        dto::NewLink,
        repo::{self, LinkRow},
    },
};

const OPEN: &str = "A pending or live link to this workspace already exists.";
const NOT_PENDING: &str = "This link is not pending.";
const NOT_RECEIVER: &str = "Only the receiving workspace accepts a link.";

async fn one(state: &AppState, ws: Uuid, id: Uuid) -> Result<LinkRow, AppError> {
    let (mut rows, _) = repo::list(&state.db, ws, Some(id), 1, 0).await?;
    rows.pop().ok_or(AppError::NotFound)
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Grant a pending link from this workspace.
pub async fn grant(
    state: &AppState,
    access: &WorkspaceAccess,
    input: NewLink,
) -> Result<LinkRow, AppError> {
    let ws = access.workspace_id;
    if input.to_workspace_id == ws
        || !repo::workspace_exists(&state.db, input.to_workspace_id).await?
    {
        return Err(AppError::field(
            "to_workspace_id",
            "The selected to workspace id is invalid.",
        ));
    }
    let id = repo::insert(
        &state.db,
        ws,
        input.to_workspace_id,
        &input.fields,
        access.user_id,
    )
    .await
    .map_err(
        |e| match e.as_database_error().and_then(|d| d.constraint()) {
            Some("supply_links_open_pair") => AppError::Conflict(OPEN),
            _ => e.into(),
        },
    )?;
    one(state, ws, id).await
}

/// One page of the workspace's links, newest first.
pub async fn list(
    state: &AppState,
    access: &WorkspaceAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LinkRow>, i64), AppError> {
    Ok(repo::list(&state.db, access.workspace_id, None, limit, offset).await?)
}

/// Accept a pending link addressed to this workspace.
pub async fn accept(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
) -> Result<LinkRow, AppError> {
    let ws = access.workspace_id;
    let link = one(state, ws, parse(id)?).await?;
    if link.to_workspace_id != ws {
        return Err(AppError::Forbidden(NOT_RECEIVER));
    }
    if !repo::accept(&state.db, link.id, ws, access.user_id).await? {
        return Err(AppError::Conflict(NOT_PENDING));
    }
    one(state, ws, link.id).await
}

/// Revoke a link this workspace is a side of; revoking again changes nothing.
pub async fn revoke(state: &AppState, access: &WorkspaceAccess, id: &str) -> Result<(), AppError> {
    let ws = access.workspace_id;
    let link = one(state, ws, parse(id)?).await?;
    repo::revoke(&state.db, link.id, ws, access.user_id).await?;
    Ok(())
}
