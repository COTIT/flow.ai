//! Sales group rules: name unique per workspace (case-insensitive), soft delete.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::sales_groups::{
        dto::{NewSalesGroup, SalesGroupPatch},
        repo::{self, SalesGroupRow},
    },
};

/// Laravel `unique` rule message.
const TAKEN: &str = "The name has already been taken.";

/// One list page plus the total matching count.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SalesGroupRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, pattern, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, pattern).await?;
    }
    Ok((rows, total))
}

/// One live row of this workspace, else 404.
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<SalesGroupRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// Insert after the module's checks; returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewSalesGroup,
) -> Result<Uuid, AppError> {
    unique(state, workspace_id, &input.name, None).await?;
    repo::insert(&state.db, workspace_id, &input)
        .await
        .map_err(taken)
}

/// Patch a live row of this workspace, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: SalesGroupPatch,
) -> Result<(), AppError> {
    let id = parse(id)?;
    unique(state, workspace_id, &patch.name, Some(id)).await?;
    found(
        repo::update(&state.db, workspace_id, id, &patch)
            .await
            .map_err(taken)?,
    )
}

/// Soft delete a live row of this workspace, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    found(repo::soft_delete(&state.db, workspace_id, parse(id)?).await?)
}

/// Friendly pre-check; the unique index still catches a parallel insert (`taken`).
async fn unique(
    state: &AppState,
    workspace_id: Uuid,
    name: &str,
    except: Option<Uuid>,
) -> Result<(), AppError> {
    if repo::name_taken(&state.db, workspace_id, name, except).await? {
        return Err(AppError::field("name", TAKEN));
    }
    Ok(())
}

fn taken(err: sqlx::Error) -> AppError {
    match err.as_database_error().and_then(|e| e.constraint()) {
        Some("sales_groups_name_unique") => AppError::field("name", TAKEN),
        _ => err.into(),
    }
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn found(changed: bool) -> Result<(), AppError> {
    changed.then_some(()).ok_or(AppError::NotFound)
}
