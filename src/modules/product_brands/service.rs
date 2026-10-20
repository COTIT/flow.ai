//! Brand rules: plain CRUD with soft delete, scoped to one workspace.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::product_brands::{
        dto::BrandInput,
        repo::{self, BrandRow},
    },
};

/// One page plus total; `pattern` from `list::search`.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BrandRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, pattern, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, pattern).await?;
    }
    Ok((rows, total))
}

/// One live brand, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<BrandRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// Insert; returns the new id.
pub async fn create(state: &AppState, workspace_id: Uuid, b: BrandInput) -> Result<Uuid, AppError> {
    Ok(repo::insert(&state.db, workspace_id, &b).await?)
}

/// Patch, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    b: BrandInput,
) -> Result<(), AppError> {
    repo::update(&state.db, workspace_id, parse(id)?, &b)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Soft delete, else 404. No in-use guard (products don't link brands yet).
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
