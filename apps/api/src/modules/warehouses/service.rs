//! Warehouse rules: slug at create, one default per workspace, people must be members,
//! soft delete without cascade (Laravel parity).

use std::collections::HashMap;

use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::{Rules, human},
        workspace::NOT_ALLOWED,
    },
    modules::{
        stock::{repo as stock, sweep},
        users::repo::{self as users, UserRef},
        warehouses::{
            dto::{BAD_PARENT, Fields, NewWarehouse, WarehousePatch, WarehouseResource},
            repo::{self, ChildRow, ListFilter, WarehouseRow},
        },
    },
};

/// Laravel `warehouse.response.slug_not_unique`.
const SLUG_TAKEN: &str = "a Warehouse with this slug already exists";

/// Rows plus what their resources embed: children and people.
pub struct Page {
    pub rows: Vec<WarehouseRow>,
    pub children: Vec<ChildRow>,
    pub people: Vec<UserRef>,
    /// Occupied bins per warehouse, for `utilization`.
    pub occupied: HashMap<Uuid, i64>,
    pub total: i64,
}

/// One list page.
pub async fn list(state: &AppState, workspace_id: Uuid, f: &ListFilter) -> Result<Page, AppError> {
    check_parent(state, workspace_id, f.parent_id).await?;
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, f).await?;
    }
    let mut page = embed(state, workspace_id, rows, true).await?;
    page.total = total;
    Ok(page)
}

/// One live warehouse with its children and people, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<Page, AppError> {
    let row = repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    embed(state, workspace_id, vec![row], true).await
}

/// Children and people of `rows`: one query each, whatever the page size.
/// `occupancy`: count occupied bins (warehouse GET/list). Embedded warehouses skip it, as
/// Laravel's plain `with('warehouse')` did (utilization 0 there).
async fn embed(
    state: &AppState,
    workspace_id: Uuid,
    rows: Vec<WarehouseRow>,
    occupancy: bool,
) -> Result<Page, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|w| w.id).collect();
    let people_ids: Vec<Uuid> = rows.iter().flat_map(WarehouseRow::people).collect();
    let children = if ids.is_empty() {
        Vec::new()
    } else {
        repo::children_of(&state.db, workspace_id, &ids).await?
    };
    let people = if people_ids.is_empty() {
        Vec::new()
    } else {
        users::members_among(&state.db, workspace_id, &people_ids).await?
    };
    let occupied = if ids.is_empty() || !occupancy {
        HashMap::new()
    } else {
        stock::occupied_by_warehouse(&state.db, workspace_id, &ids)
            .await?
            .into_iter()
            .collect()
    };
    Ok(Page {
        total: rows.len() as i64,
        rows,
        children,
        people,
        occupied,
    })
}

/// Full WarehouseResource JSON of the live warehouses among `ids`, in one batch, for embedding
/// (no occupancy count).
pub async fn resources(
    state: &AppState,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Value>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = repo::find_many(&state.db, workspace_id, ids).await?;
    let order: Vec<Uuid> = rows.iter().map(|w| w.id).collect();
    let p = embed(state, workspace_id, rows, false).await?;
    let built = WarehouseResource::build(p.rows, p.children, &p.people, &p.occupied);
    order
        .into_iter()
        .zip(built)
        .map(|(id, w)| {
            let v = serde_json::to_value(w).map_err(|e| AppError::Internal(e.to_string()))?;
            Ok((id, v))
        })
        .collect()
}

/// Create; the first warehouse of a workspace becomes its default unless sent `false`.
/// `may_set_default`: caller holds `warehouse.set_default`, needed to take the default
/// from another warehouse.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewWarehouse,
    may_set_default: bool,
) -> Result<Uuid, AppError> {
    check_people(state, workspace_id, &input.fields).await?;
    check_parent(state, workspace_id, input.parent_id).await?;
    let slug = slugify(&input.name);
    let mut tx = state.db.begin().await?;
    repo::lock_default(&mut tx, workspace_id).await?;
    if repo::slug_taken(&mut *tx, workspace_id, &slug).await? {
        return Err(AppError::field("slug", SLUG_TAKEN));
    }
    let has_default = repo::has_default(&mut *tx, workspace_id).await?;
    let default = match input.default_warehouse {
        Some(true) if has_default && !may_set_default => {
            return Err(AppError::Forbidden(NOT_ALLOWED));
        }
        Some(true) => {
            repo::clear_default(&mut *tx, workspace_id, None).await?;
            true
        }
        Some(false) => false,
        None => !has_default,
    };
    let id = repo::insert(
        &mut *tx,
        workspace_id,
        &input.name,
        &slug,
        input.parent_id,
        default,
        &input.fields,
    )
    .await
    .map_err(slug_race)?;
    if default || input.fields.is_store == Some(Some(true)) {
        sweep::enqueue(&mut tx, workspace_id).await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// Patch a live warehouse; `default_warehouse: true` moves the default here. Changing the
/// flag needs `may_set_default` (`warehouse.set_default`); echoing the stored value doesn't,
/// since vimdesk-web sends it on every save.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: WarehousePatch,
    may_set_default: bool,
) -> Result<(), AppError> {
    let id = parse(id)?;
    check_people(state, workspace_id, &patch.fields).await?;
    if let Some(wanted) = patch.default_warehouse
        && !may_set_default
    {
        let stored = repo::default_flag(&state.db, workspace_id, id)
            .await?
            .ok_or(AppError::NotFound)?;
        if stored != wanted {
            return Err(AppError::Forbidden(NOT_ALLOWED));
        }
    }
    let to_default = patch.default_warehouse == Some(true);
    let to_store = patch.fields.is_store == Some(Some(true));
    let found = if to_default || to_store {
        // Lock order: the default lock, then this row.
        let mut tx = state.db.begin().await?;
        if to_default {
            repo::lock_default(&mut tx, workspace_id).await?;
        }
        let (was_store, was_default) = repo::flags_locked(&mut tx, workspace_id, id)
            .await?
            .ok_or(AppError::NotFound)?;
        // Re-checked under the lock: the default may have moved since the check above.
        if to_default && !was_default && !may_set_default {
            return Err(AppError::Forbidden(NOT_ALLOWED));
        }
        if to_default {
            repo::clear_default(&mut *tx, workspace_id, Some(id)).await?;
        }
        let found = update_row(&mut *tx, workspace_id, id, &patch).await?;
        // A new main or store needs its stock rows; re-saving the same flag (the SPA sends
        // it on every save) doesn't.
        if (to_store && !was_store) || (to_default && !was_default) {
            sweep::enqueue(&mut tx, workspace_id).await?;
        }
        tx.commit().await?;
        found
    } else {
        update_row(&state.db, workspace_id, id, &patch).await?
    };
    found.then_some(()).ok_or(AppError::NotFound)
}

async fn update_row(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &WarehousePatch,
) -> Result<bool, AppError> {
    Ok(repo::update(
        db,
        workspace_id,
        id,
        &p.name,
        p.default_warehouse,
        &p.fields,
    )
    .await?)
}

/// Laravel SetDefaultWarehouse: unset the old default, set this one.
pub async fn set_default(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    repo::lock_default(&mut tx, workspace_id).await?;
    let (_, was_default) = repo::flags_locked(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    repo::clear_default(&mut *tx, workspace_id, Some(id)).await?;
    if !repo::set_default(&mut *tx, workspace_id, id).await? {
        return Err(AppError::NotFound);
    }
    if !was_default {
        sweep::enqueue(&mut tx, workspace_id).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Soft delete. Children keep their parent id and stay listed via `roots_only=0`. 409
/// `warehouse_holds_stock` while any stock level or hold here is not zero.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    // Waits for open stock writes here; the check below then sees them.
    if !repo::lock_for_delete(&mut tx, workspace_id, id).await? {
        return Err(AppError::NotFound);
    }
    if !repo::soft_delete(&mut *tx, workspace_id, id).await? {
        return Err(AppError::Conflict("warehouse_holds_stock"));
    }
    tx.commit().await?;
    Ok(())
}

/// A sent parent must be a live warehouse of this workspace (Laravel ValidateId).
async fn check_parent(
    state: &AppState,
    workspace_id: Uuid,
    parent: Option<Uuid>,
) -> Result<(), AppError> {
    match parent {
        Some(id) if !repo::exists(&state.db, workspace_id, id).await? => {
            Err(AppError::field("parent_id", BAD_PARENT))
        }
        _ => Ok(()),
    }
}

/// user/manager/assistant ids must be members of this workspace. Laravel accepted any user
/// row, which would embed another tenant's name and email in this resource.
async fn check_people(state: &AppState, workspace_id: Uuid, f: &Fields) -> Result<(), AppError> {
    let wanted = f.people();
    if wanted.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = wanted.iter().map(|(_, id)| *id).collect();
    let found = users::members_among(&state.db, workspace_id, &ids).await?;
    let mut r = Rules::default();
    for (field, id) in wanted {
        if !found.iter().any(|u| u.id == id) {
            let f = human(field);
            r.add(field, format!("The selected {f} is invalid."));
        }
    }
    r.into_result()
}

/// Laravel `Str::slug`: `_` and `@` handled like Laravel, then only letters, digits,
/// spaces and dashes kept, lowercased, runs of spaces/dashes become one `-`, ends trimmed.
// shortcut: no ASCII transliteration ("café" stays "café", Laravel gave "cafe"); add a
// transliteration table if non-ASCII names show up.
fn slugify(name: &str) -> String {
    let replaced = name.replace('_', "-").replace('@', "-at-");
    let mut out = String::with_capacity(replaced.len());
    let mut dash = false;
    for c in replaced.chars().flat_map(char::to_lowercase) {
        if c == '-' || c.is_whitespace() {
            dash = !out.is_empty();
        } else if c.is_alphanumeric() {
            if dash {
                out.push('-');
                dash = false;
            }
            out.push(c);
        }
    }
    out
}

/// The live-slug unique index caught a race the pre-check missed: same 422.
fn slug_race(e: sqlx::Error) -> AppError {
    AppError::unique(e, "warehouses_slug_unique", "slug", SLUG_TAKEN)
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slug_matches_laravel_str_slug() {
        for (name, slug) in [
            ("North West Storage", "north-west-storage"),
            ("north-west", "north-west"),
            ("  North  West! ", "north-west"),
            ("A_b  --  c", "a-b-c"),
            ("Info@Shop", "info-at-shop"),
            ("Hal 3", "hal-3"),
            ("!!!", ""),
        ] {
            assert_eq!(slugify(name), slug, "{name}");
        }
    }
}
