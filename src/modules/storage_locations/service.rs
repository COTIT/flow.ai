//! Storage location rules: the parent must be a live product of the workspace (other
//! domains aren't ported: 404); the place is a live warehouse location, or the one made from
//! a live bin's code path; `replace` drops the product's other places first.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        products::repo as products,
        storage_locations::{
            dto::{NewPlace, render},
            repo,
        },
    },
};

/// The live product behind `(domain, id)`, else 404.
async fn product(
    state: &AppState,
    workspace_id: Uuid,
    domain: &str,
    id: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    if domain != "product" {
        return Err(AppError::NotFound);
    }
    products::find(&state.db, workspace_id, id)
        .await?
        .map(|p| p.id)
        .ok_or(AppError::NotFound)
}

/// POST: create in one transaction; returns the new id.
pub async fn create(state: &AppState, workspace_id: Uuid, n: NewPlace) -> Result<Uuid, AppError> {
    let product = product(state, workspace_id, &n.parent_domain, &n.parent_id).await?;
    let from_bin = match (n.warehouse_location_id, n.bin_id) {
        (Some(l), _) => {
            if !repo::location_exists(&state.db, workspace_id, l).await? {
                return Err(AppError::NotFound);
            }
            None
        }
        (None, Some(b)) => Some(
            repo::bin_path(&state.db, workspace_id, b)
                .await?
                .map(|(wh, path)| (wh, path, b))
                .ok_or(AppError::NotFound)?,
        ),
        (None, None) => return Err(AppError::NotFound),
    };
    let mut tx = state.db.begin().await?;
    let location = match (n.warehouse_location_id, from_bin) {
        (Some(l), _) => l,
        (None, Some((wh, path, bin))) => {
            let meta = json!({ "generated_from": "bin_hierarchy", "bin_uuid": bin });
            repo::location_for(&mut tx, workspace_id, wh, &path, &meta).await?
        }
        (None, None) => return Err(AppError::NotFound),
    };
    if n.replace {
        repo::clear_product(&mut tx, workspace_id, product).await?;
    }
    let id = repo::insert(
        &mut tx,
        workspace_id,
        product,
        location,
        n.description.as_deref(),
        n.meta.as_ref(),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// GET `/{domain}/{id}`: the parent's places.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    domain: &str,
    id: &str,
) -> Result<Vec<Value>, AppError> {
    let product = product(state, workspace_id, domain, id).await?;
    let rows = repo::of_product(&state.db, workspace_id, product).await?;
    Ok(rows.iter().map(render).collect())
}

/// DELETE: soft delete, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    repo::soft_delete(&state.db, workspace_id, id)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}
