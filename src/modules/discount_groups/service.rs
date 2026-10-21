//! Discount group rules: supplier must be a live relation of the workspace; internal and
//! supplier are fixed at create; delete retires the group's discounts and relation links.

use std::collections::HashMap;

use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        discount_groups::{
            dto::{GroupPatch, GroupResource, INVALID_RELATIONS, NewGroup},
            repo::{self, GroupRow, ListFilter},
        },
        relations,
    },
};

/// Laravel RelationExist text.
const NO_RELATION: &str = "This relation does not exist";

/// List rows with the window total (0 when the page is empty past the end: recounted).
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    filter: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<GroupRow>, i64), AppError> {
    let rows = repo::list(&state.db, workspace_id, filter, limit, offset).await?;
    let total = match rows.first() {
        Some(r) => r.total.unwrap_or(0),
        None if offset > 0 => repo::count(&state.db, workspace_id, filter).await?,
        None => 0,
    };
    Ok((rows, total))
}

/// Render rows with their suppliers (one batched relation load).
pub async fn render(
    state: &AppState,
    workspace_id: Uuid,
    rows: Vec<GroupRow>,
) -> Result<Vec<GroupResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|g| g.supplier_relation_id).collect();
    let mut suppliers: HashMap<Uuid, Value> =
        relations::service::index_values(state, workspace_id, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|g| {
            let supplier = suppliers.remove(&g.supplier_relation_id);
            GroupResource::new(g, supplier)
        })
        .collect())
}

/// One live group, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<GroupRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// Create after checking the supplier; returns the new id.
pub async fn create(state: &AppState, workspace_id: Uuid, g: NewGroup) -> Result<Uuid, AppError> {
    if repo::live_relations(&state.db, workspace_id, &[g.supplier_relation_id]).await? != 1 {
        return Err(AppError::field("supplier_relation_id", NO_RELATION));
    }
    Ok(repo::insert(&state.db, workspace_id, &g).await?)
}

/// Patch a live group, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    p: GroupPatch,
) -> Result<(), AppError> {
    found(repo::update(&state.db, workspace_id, parse(id)?, &p).await?)
}

/// Soft delete the group, its discounts and its relation links in one transaction.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let retired = repo::retire(&mut tx, workspace_id, id).await?;
    tx.commit().await?;
    found(retired)
}

/// Link (`attach`) or unlink relations; every id must be a live relation of the workspace.
pub async fn link(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    relation_ids: &[Uuid],
    attach: bool,
) -> Result<(), AppError> {
    let group = get(state, workspace_id, id).await?;
    let mut unique = relation_ids.to_vec();
    unique.sort_unstable();
    unique.dedup();
    let live = repo::live_relations(&state.db, workspace_id, &unique).await?;
    if usize::try_from(live).ok() != Some(unique.len()) {
        return Err(AppError::field("relation_ids", INVALID_RELATIONS));
    }
    if attach {
        repo::attach(&state.db, workspace_id, group.id, &unique).await?;
    } else {
        repo::detach(&state.db, workspace_id, group.id, &unique).await?;
    }
    Ok(())
}

/// Page of the group's relations as RelationIndexResource values, plus the total.
pub async fn relations_page(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Value>, i64), AppError> {
    let group = get(state, workspace_id, id).await?;
    let (ids, total) =
        repo::relation_page(&state.db, workspace_id, group.id, limit, offset).await?;
    let mut values = relations::service::index_values(state, workspace_id, &ids).await?;
    Ok((ids.iter().filter_map(|i| values.remove(i)).collect(), total))
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn found(changed: bool) -> Result<(), AppError> {
    changed.then_some(()).ok_or(AppError::NotFound)
}
