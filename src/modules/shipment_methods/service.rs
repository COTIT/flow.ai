//! Shipment method rules: max_weight > min_weight on the merged row (DB CHECK), soft delete.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::shipment_methods::{
        dto::{NewShipmentMethod, ShipmentMethodPatch, weight_message},
        repo::{self, ListFilter, ShipmentMethodRow},
    },
};

/// One list page plus the total matching count.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<ShipmentMethodRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, f).await?;
    }
    Ok((rows, total))
}

pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<ShipmentMethodRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewShipmentMethod,
) -> Result<Uuid, AppError> {
    Ok(repo::insert(&state.db, workspace_id, &input).await?)
}

/// A one-sided weight change can cross the stored other side; the CHECK catches it and the
/// 422 names the minimum that applied.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: ShipmentMethodPatch,
) -> Result<(), AppError> {
    let id = parse(id)?;
    match repo::update(&state.db, workspace_id, id, &patch).await {
        Ok(changed) => changed.then_some(()).ok_or(AppError::NotFound),
        Err(e) if constraint(&e) == Some("shipment_methods_weight_range") => {
            let stored = repo::find(&state.db, workspace_id, id)
                .await?
                .ok_or(AppError::NotFound)?;
            let min = patch.min_weight.unwrap_or(stored.min_weight);
            Err(AppError::field("max_weight", weight_message(min)))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

fn constraint(e: &sqlx::Error) -> Option<&str> {
    e.as_database_error().and_then(|d| d.constraint())
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
