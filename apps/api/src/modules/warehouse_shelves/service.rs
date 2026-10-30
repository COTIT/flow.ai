//! Shelf rules: reached only through a live row, section and warehouse; code unique per row.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        warehouse_rows::{repo::RowRow, service as rows},
        warehouse_sections::{repo::SectionRow, service::parse},
        warehouse_shelves::{
            dto::{CODE_TAKEN, Fields, NewShelf},
            repo::{self, Parent, ShelfRow},
        },
    },
};

/// `{warehouse}/{section}/{row}` of the request path.
pub type RowPath<'a> = (&'a str, &'a str, &'a str);

/// The path's row (with its section), else 404.
pub async fn row(
    state: &AppState,
    workspace_id: Uuid,
    path: RowPath<'_>,
) -> Result<(SectionRow, RowRow), AppError> {
    rows::find(state, workspace_id, path).await
}

/// The path's shelf, else 404.
pub async fn find(
    state: &AppState,
    workspace_id: Uuid,
    path: RowPath<'_>,
    id: &str,
) -> Result<ShelfRow, AppError> {
    let (_, r) = row(state, workspace_id, path).await?;
    repo::find(&state.db, workspace_id, r.id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// One page plus total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    row_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ShelfRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, row_id, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, row_id).await?;
    }
    Ok((rows, total))
}

/// Insert; returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    section: &SectionRow,
    row: &RowRow,
    n: NewShelf,
) -> Result<Uuid, AppError> {
    let parent = Parent {
        workspace_id,
        warehouse_id: section.warehouse_id,
        section_id: section.id,
        row_id: row.id,
    };
    repo::insert(&state.db, &parent, &n)
        .await
        .map_err(code_clash)
}

/// Patch; 404 when the shelf is gone.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    row_id: Uuid,
    id: &str,
    f: Fields,
) -> Result<(), AppError> {
    let id = parse(id)?;
    // No code pre-check: an unknown id must stay a 404; the index catches a clash.
    repo::update(&state.db, workspace_id, row_id, id, &f)
        .await
        .map_err(code_clash)?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Soft delete, else 404.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    row_id: Uuid,
    id: &str,
) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, row_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// The live-code unique index fired: same 422 as the pre-check.
fn code_clash(e: sqlx::Error) -> AppError {
    AppError::unique(
        e,
        "warehouse_section_row_shelves_code_unique",
        "shelf_code",
        CODE_TAKEN,
    )
}
