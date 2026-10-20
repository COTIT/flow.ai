//! VAT group rules: plain CRUD with soft delete, scoped to one workspace.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::vat_groups::{
        dto::{NewVatGroup, VatGroupPatch},
        repo::{self, ListFilter, VatGroupRow},
    },
};

/// One list page plus the total matching count.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<VatGroupRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, f).await?;
    }
    Ok((rows, total))
}

pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<VatGroupRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewVatGroup,
) -> Result<Uuid, AppError> {
    Ok(repo::insert(&state.db, workspace_id, &input).await?)
}

pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: VatGroupPatch,
) -> Result<(), AppError> {
    found(repo::update(&state.db, workspace_id, parse(id)?, &patch).await?)
}

/// Soft delete, no in-use guard (Laravel parity): products keep pointing at the row.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    found(repo::soft_delete(&state.db, workspace_id, parse(id)?).await?)
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn found(changed: bool) -> Result<(), AppError> {
    changed.then_some(()).ok_or(AppError::NotFound)
}
