//! Mapping rules: the pair must be the workspace's; a `user` target must be an active member.

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    integrations::{
        connections::service::parse,
        mappings::{
            dto::{ENTITIES, NewMapping},
            repo::{self, MappingRow},
        },
    },
};

const BAD_TARGET: &str = "The selected target id is invalid.";

async fn pair(state: &AppState, ws: Uuid, id: &str) -> Result<Uuid, AppError> {
    let id = parse(id)?;
    if repo::pair_exists(&state.db, ws, id).await? {
        Ok(id)
    } else {
        Err(AppError::NotFound)
    }
}

/// One page of a pair's mappings.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    pair_id: &str,
    entity: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MappingRow>, i64), AppError> {
    let pair = pair(state, ws, pair_id).await?;
    if entity.is_some_and(|e| !ENTITIES.contains(&e)) {
        return Err(AppError::field("entity", "The selected entity is invalid."));
    }
    Ok(repo::list(&state.db, ws, pair, entity, limit, offset).await?)
}

/// Insert or replace by `(entity, source_id)`.
pub async fn upsert(
    state: &AppState,
    ws: Uuid,
    user: Uuid,
    pair_id: &str,
    m: NewMapping,
) -> Result<Uuid, AppError> {
    let pair = pair(state, ws, pair_id).await?;
    if m.entity == "user" {
        let member = match Uuid::parse_str(&m.target_id) {
            Ok(u) => repo::active_member(&state.db, ws, u).await?,
            Err(_) => false,
        };
        if !member {
            return Err(AppError::field("target_id", BAD_TARGET));
        }
    }
    Ok(repo::upsert(
        &state.db,
        ws,
        pair,
        &m.entity,
        &m.source_id,
        &m.target_id,
        m.source_label.as_deref(),
        m.target_label.as_deref(),
        user,
    )
    .await?)
}

/// Delete one mapping.
pub async fn delete(state: &AppState, ws: Uuid, pair_id: &str, id: &str) -> Result<(), AppError> {
    let pair = pair(state, ws, pair_id).await?;
    if repo::delete(&state.db, ws, pair, parse(id)?).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// `source_id → target_id` for the mapped ones among `source_ids`.
pub async fn resolve(
    db: impl sqlx::PgExecutor<'_>,
    ws: Uuid,
    pair: Uuid,
    entity: &str,
    source_ids: &[String],
) -> Result<BTreeMap<String, String>, AppError> {
    Ok(repo::resolve(db, ws, pair, entity, source_ids)
        .await?
        .into_iter()
        .collect())
}
