//! Deal rules. A deal's place on the board is a gap rank in its column: a drag reads the two
//! neighbours at the target index and writes one row; a closed gap renumbers the column once.
//! Rank writers lock boards (share) → source and target columns → deal, all in id order, so
//! drags and column edits (B8-1's board write lock) never cross and no deal leaves a column
//! while it respaces. A drag still retries once if Postgres reports a deadlock.

use std::collections::HashMap;

use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::Rules,
    modules::{
        currencies::{self, dto::CurrencyResource},
        deal_pipelines,
        deals::{
            dto::{
                AGENT_MISSING, CURRENCY_MISSING, DealMove, DealPatch, DealResource, Embeds,
                NO_MAIN_CURRENCY, NewDeal, PIPELINE_MISSING, RELATION_MISSING,
                STATUS_NOT_IN_PIPELINE, not_past,
            },
            repo::{self, DealRow, ListFilter},
        },
        products, relations,
        users::{self, dto::UserEmailResource},
    },
};

/// Rank spacing: 16 drops between the same two neighbours before a column renumbers.
pub const STEP: i64 = 65_536;
/// Tries when the deal moved between the unlocked read and the locks (then 503, retry later).
const ATTEMPTS: usize = 3;
/// Index meaning "at the end": appends read the last rank only, never the column.
const END: i64 = i64::MAX;
/// Laravel's no-relation-twice check (its translation key was missing).
const HAS_RELATION: &str = "This deal already has a relation attached.";

/// One page of deals (full resource), newest first, plus the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<DealResource>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, f).await?;
    }
    Ok((resources(state, workspace_id, rows, true).await?, total))
}

/// One deal, full resource.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<DealResource, AppError> {
    let row = repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    one(state, workspace_id, row).await
}

/// One board column (index resource), in the board's order, plus its deal count.
pub async fn column(
    state: &AppState,
    workspace_id: Uuid,
    pipeline: &str,
    status: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DealResource>, i64), AppError> {
    let (pipeline, status) = (parse(pipeline)?, parse(status)?);
    let board = deal_pipelines::repo::find(&state.db, workspace_id, pipeline)
        .await?
        .ok_or(AppError::NotFound)?;
    if !deal_pipelines::repo::status_exists(&state.db, workspace_id, pipeline, status).await? {
        return Err(AppError::NotFound);
    }
    let rows = repo::column(
        &state.db,
        workspace_id,
        status,
        &board.sorting,
        limit,
        offset,
    )
    .await?;
    let total = repo::column_count(&state.db, workspace_id, status).await?;
    Ok((resources(state, workspace_id, rows, false).await?, total))
}

/// New deal at the end of its column; returns its id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    input: NewDeal,
) -> Result<Uuid, AppError> {
    let mut tx = state.db.begin().await?;
    let mut r = Rules::default();
    let to = (input.pipeline, input.status);
    place_checks(&mut tx, &mut r, workspace_id, to, None).await?;
    if let Some(rel) = input.relation
        && !repo::relation_exists(&mut *tx, workspace_id, rel).await?
    {
        r.add("relation_id", RELATION_MISSING);
    }
    let currency = match input.currency {
        Some(c) if repo::currency_exists(&mut *tx, workspace_id, c).await? => Some(c),
        Some(_) => {
            r.add("currency_id", CURRENCY_MISSING);
            None
        }
        None => {
            let main = products::repo::main_currency(&mut *tx, workspace_id).await?;
            if main.is_none() {
                r.add("currency_id", NO_MAIN_CURRENCY);
            }
            main
        }
    };
    agent_check(&mut tx, &mut r, workspace_id, input.sales_agent).await?;
    r.into_result()?;
    let currency = currency.ok_or_else(|| AppError::Internal("currency checked above".into()))?;
    let position = rank_at(&mut tx, workspace_id, input.status, Uuid::nil(), END).await?;
    let id = repo::insert(&mut tx, workspace_id, &input, currency, position, user_id).await?;
    tx.commit().await?;
    Ok(id)
}

/// Patch a live deal, else 404. A new column (or board) appends the deal there; the edit
/// modal's unchanged column moves nothing.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: DealPatch,
) -> Result<(), AppError> {
    let id = parse(id)?;
    for _ in 0..ATTEMPTS {
        let current = repo::find(&state.db, workspace_id, id)
            .await?
            .ok_or(AppError::NotFound)?;
        let from = (current.deal_pipeline_id, current.deal_pipeline_status_id);
        let to = (
            patch.pipeline.unwrap_or(from.0),
            patch.status.unwrap_or(from.1),
        );
        let moving = to != from;
        let mut tx = state.db.begin().await?;
        let mut r = Rules::default();
        if moving {
            place_checks(&mut tx, &mut r, workspace_id, to, Some(from)).await?;
        }
        if let Some(Some(agent)) = patch.sales_agent {
            agent_check(&mut tx, &mut r, workspace_id, agent).await?;
        }
        let deal = repo::lock_deal(&mut tx, workspace_id, id)
            .await?
            .ok_or(AppError::NotFound)?;
        if moving && (deal.deal_pipeline_id, deal.deal_pipeline_status_id) != from {
            continue; // moved since the read: its locked source column is stale
        }
        // The edit modal re-sends the stored close date; only a new one must not be past.
        if let Some(Some(close)) = patch.expected_close_date
            && deal.expected_close_date != Some(close)
        {
            not_past(&mut r, close);
        }
        r.into_result()?;
        let place = if moving {
            let rank = rank_at(&mut tx, workspace_id, to.1, id, END).await?;
            Some((to.0, to.1, rank))
        } else {
            None
        };
        repo::update(&mut tx, workspace_id, id, &patch, place).await?;
        tx.commit().await?;
        return Ok(());
    }
    Err(AppError::Overloaded)
}

/// Kanban drag: put the deal at `index` of the target column (0-based, the deal itself not
/// counted); past the end appends.
pub async fn move_to(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    m: DealMove,
) -> Result<(), AppError> {
    let id = parse(id)?;
    match drag(state, workspace_id, id, &m).await {
        Err(e) if e.is_deadlock() => drag(state, workspace_id, id, &m).await,
        done => done,
    }
}

/// One drag transaction. The deal's source column is locked with the target, so nothing else
/// takes a deal out of a column while this drag respaces it; a deal moved since the unlocked
/// read is read again.
async fn drag(
    state: &AppState,
    workspace_id: Uuid,
    id: Uuid,
    m: &DealMove,
) -> Result<(), AppError> {
    for _ in 0..ATTEMPTS {
        let current = repo::find(&state.db, workspace_id, id).await?;
        let from = current
            .as_ref()
            .map(|d| (d.deal_pipeline_id, d.deal_pipeline_status_id));
        let mut tx = state.db.begin().await?;
        let mut r = Rules::default();
        place_checks(&mut tx, &mut r, workspace_id, (m.pipeline, m.status), from).await?;
        r.into_result()?;
        let deal = repo::lock_deal(&mut tx, workspace_id, id)
            .await?
            .ok_or(AppError::NotFound)?;
        if Some((deal.deal_pipeline_id, deal.deal_pipeline_status_id)) != from {
            continue;
        }
        let rank = rank_at(&mut tx, workspace_id, m.status, id, m.index).await?;
        let place = Some((m.pipeline, m.status, rank));
        repo::update(&mut tx, workspace_id, id, &DealPatch::default(), place).await?;
        tx.commit().await?;
        return Ok(());
    }
    Err(AppError::Overloaded)
}

/// Attach a relation to a deal that has none. 400 when one is set already (Laravel).
pub async fn add_relation(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    relation: Uuid,
) -> Result<(), AppError> {
    let id = parse(id)?;
    if !repo::relation_exists(&state.db, workspace_id, relation).await? {
        return Err(AppError::field("relation_id", RELATION_MISSING));
    }
    if repo::attach_relation(&state.db, workspace_id, id, relation).await? {
        return Ok(());
    }
    match repo::find(&state.db, workspace_id, id).await? {
        Some(_) => Err(AppError::BadRequest(HAS_RELATION.into())),
        None => Err(AppError::NotFound),
    }
}

/// Soft delete a live deal, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Board rows `FOR SHARE`, then the target and source columns, all in id order; records
/// Laravel's texts for a missing board or a target that isn't a live column of it.
async fn place_checks(
    tx: &mut sqlx::PgConnection,
    r: &mut Rules,
    workspace_id: Uuid,
    to: (Uuid, Uuid),
    from: Option<(Uuid, Uuid)>,
) -> Result<(), AppError> {
    let mut boards = vec![to.0];
    let mut columns = vec![to.1];
    if let Some((board, column)) = from {
        boards.push(board);
        columns.push(column);
    }
    if !repo::lock_boards(tx, workspace_id, &boards)
        .await?
        .contains(&to.0)
    {
        r.add("deal_pipeline_id", PIPELINE_MISSING);
        r.add("deal_pipeline_status_id", STATUS_NOT_IN_PIPELINE);
        return Ok(());
    }
    if !repo::lock_columns(tx, workspace_id, &columns)
        .await?
        .contains(&(to.1, to.0))
    {
        r.add("deal_pipeline_status_id", STATUS_NOT_IN_PIPELINE);
    }
    Ok(())
}

/// The sales agent must be a member of the workspace (Laravel UserExist).
async fn agent_check(
    tx: &mut sqlx::PgConnection,
    r: &mut Rules,
    workspace_id: Uuid,
    agent: Uuid,
) -> Result<(), AppError> {
    if users::repo::members_among(&mut *tx, workspace_id, &[agent])
        .await?
        .is_empty()
    {
        r.add("sales_agent_id", AGENT_MISSING);
    }
    Ok(())
}

/// Rank for a deal (`except`, nil for a new one) placed at `index` of a column. Run under
/// `lock_columns`; renumbers the column once when its gap there has closed.
async fn rank_at(
    tx: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    status: Uuid,
    except: Uuid,
    index: i64,
) -> Result<i64, AppError> {
    for _ in 0..2 {
        let (before, after) = if index == END {
            (
                repo::last_rank(tx, workspace_id, status, Some(except)).await?,
                None,
            )
        } else if index == 0 {
            let ranks = repo::ranks_from(tx, workspace_id, status, except, 0).await?;
            (None, ranks.first().copied())
        } else {
            let ranks = repo::ranks_from(tx, workspace_id, status, except, index - 1).await?;
            match ranks.as_slice() {
                [] => (
                    repo::last_rank(tx, workspace_id, status, Some(except)).await?,
                    None,
                ),
                [b] => (Some(*b), None),
                [b, a, ..] => (Some(*b), Some(*a)),
            }
        };
        if let Some(rank) = rank_between(before, after) {
            return Ok(rank);
        }
        repo::renumber(tx, workspace_id, status, except, STEP).await?;
    }
    Err(AppError::Internal("no free rank after renumbering".into()))
}

/// A rank strictly between two neighbours (either may be absent); None when they touch.
fn rank_between(before: Option<i64>, after: Option<i64>) -> Option<i64> {
    match (before, after) {
        (None, None) => Some(0),
        (Some(b), None) => b.checked_add(STEP),
        (None, Some(a)) => a.checked_sub(STEP),
        (Some(b), Some(a)) => a.checked_sub(b).filter(|g| *g >= 2).map(|g| b + g / 2),
    }
}

async fn one(state: &AppState, workspace_id: Uuid, row: DealRow) -> Result<DealResource, AppError> {
    resources(state, workspace_id, vec![row], true)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// Deals with their embeds: relations, agents, currencies (and, `full`, boards and columns),
/// one batch query each whatever the page size.
async fn resources(
    state: &AppState,
    workspace_id: Uuid,
    rows: Vec<DealRow>,
    full: bool,
) -> Result<Vec<DealResource>, AppError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let relation_ids = unique(rows.iter().filter_map(|d| d.relation_id));
    let relations = relations::service::index_values(state, workspace_id, &relation_ids).await?;
    let agent_ids = unique(rows.iter().filter_map(|d| d.sales_agent_id));
    let mut agents = HashMap::new();
    if !agent_ids.is_empty() {
        for u in users::repo::members_among(&state.db, workspace_id, &agent_ids).await? {
            agents.insert(u.id, to_json(&UserEmailResource::from(&u))?);
        }
    }
    let currency_ids = unique(rows.iter().map(|d| d.currency_id));
    let mut currency = HashMap::new();
    for c in currencies::repo::find_many(&state.db, workspace_id, &currency_ids).await? {
        currency.insert(c.id, to_json(&CurrencyResource::from(c))?);
    }
    let (boards, columns) = if full {
        let ids = unique(rows.iter().map(|d| d.deal_pipeline_id));
        deal_pipelines::service::embeds(state, workspace_id, &ids).await?
    } else {
        Default::default()
    };
    let get = |m: &HashMap<Uuid, Value>, id: Option<Uuid>| {
        id.and_then(|id| m.get(&id).cloned()).unwrap_or(Value::Null)
    };
    Ok(rows
        .into_iter()
        .map(|d| {
            let e = Embeds {
                sales_agent: get(&agents, d.sales_agent_id),
                relation: get(&relations, d.relation_id),
                currency: get(&currency, Some(d.currency_id)),
                board: full.then(|| {
                    (
                        get(&boards, Some(d.deal_pipeline_id)),
                        get(&columns, Some(d.deal_pipeline_status_id)),
                    )
                }),
            };
            DealResource::new(d, e)
        })
        .collect())
}

fn unique(ids: impl Iterator<Item = Uuid>) -> Vec<Uuid> {
    let mut v: Vec<Uuid> = ids.collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Value, AppError> {
    serde_json::to_value(v).map_err(|e| AppError::Internal(e.to_string()))
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_fit_between_or_ask_for_a_renumber() {
        assert_eq!(rank_between(None, None), Some(0));
        assert_eq!(rank_between(Some(10), None), Some(10 + STEP));
        assert_eq!(rank_between(None, Some(10)), Some(10 - STEP));
        assert_eq!(rank_between(Some(0), Some(STEP)), Some(STEP / 2));
        assert_eq!(rank_between(Some(4), Some(6)), Some(5));
        assert_eq!(rank_between(Some(4), Some(5)), None, "touching: renumber");
        assert_eq!(
            rank_between(Some(i64::MAX), None),
            None,
            "overflow: renumber"
        );
        assert_eq!(rank_between(None, Some(i64::MIN)), None);
    }
}
