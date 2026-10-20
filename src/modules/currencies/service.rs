//! Currency rules: one live default per workspace, soft delete. Scoped to one workspace.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::currencies::{
        dto::{CurrencyPatch, NewCurrency},
        repo::{self, CurrencyRow},
    },
};

/// Lost a race against another default switch (the partial unique index caught it).
const DEFAULT_RACE: &str = "Another currency became the default at the same time. Try again.";

/// One list page plus the total matching count.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CurrencyRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, pattern, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, pattern).await?;
    }
    Ok((rows, total))
}

pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<CurrencyRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// A new default unsets the old one in the same transaction.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewCurrency,
) -> Result<Uuid, AppError> {
    let mut tx = state.db.begin().await?;
    if input.default_currency {
        repo::clear_default(&mut *tx, workspace_id, None).await?;
    }
    let id = repo::insert(&mut *tx, workspace_id, &input)
        .await
        .map_err(race)?;
    tx.commit().await?;
    Ok(id)
}

/// Patch. Becoming default unsets the old one; an unknown id rolls that back (404).
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: CurrencyPatch,
) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    if patch.default_currency == Some(true) {
        repo::clear_default(&mut *tx, workspace_id, Some(id)).await?;
    }
    if !repo::update(&mut *tx, workspace_id, id, &patch)
        .await
        .map_err(race)?
    {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Soft delete, no guard (Laravel parity): deleting the default leaves none.
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

fn race(err: sqlx::Error) -> AppError {
    match err.as_database_error().and_then(|e| e.constraint()) {
        Some("currencies_one_default") => AppError::field("default_currency", DEFAULT_RACE),
        _ => err.into(),
    }
}
