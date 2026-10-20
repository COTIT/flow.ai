//! Warehouse rules: slug at create, one default per workspace, people must be members,
//! soft delete without cascade (Laravel parity).

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::{Rules, human},
    modules::{
        users::repo::{self as users, UserRef},
        warehouses::{
            dto::{BAD_PARENT, Fields, NewWarehouse, WarehousePatch},
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
    pub total: i64,
}

/// One list page.
pub async fn list(state: &AppState, workspace_id: Uuid, f: &ListFilter) -> Result<Page, AppError> {
    check_parent(state, workspace_id, f.parent_id).await?;
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, f).await?;
    }
    let mut page = embed(state, workspace_id, rows).await?;
    page.total = total;
    Ok(page)
}

/// One live warehouse with its children and people, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<Page, AppError> {
    let row = repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    embed(state, workspace_id, vec![row]).await
}

/// Children and people of `rows`: one query each, whatever the page size.
async fn embed(
    state: &AppState,
    workspace_id: Uuid,
    rows: Vec<WarehouseRow>,
) -> Result<Page, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|w| w.id).collect();
    let people_ids: Vec<Uuid> = rows.iter().flat_map(WarehouseRow::people).collect();
    let children = repo::children_of(&state.db, workspace_id, &ids).await?;
    let people = if people_ids.is_empty() {
        Vec::new()
    } else {
        users::members_among(&state.db, workspace_id, &people_ids).await?
    };
    Ok(Page {
        total: rows.len() as i64,
        rows,
        children,
        people,
    })
}

/// Create; the first warehouse of a workspace becomes its default unless sent `false`.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewWarehouse,
) -> Result<Uuid, AppError> {
    check_people(state, workspace_id, &input.fields).await?;
    check_parent(state, workspace_id, input.parent_id).await?;
    let slug = slugify(&input.name);
    let mut tx = state.db.begin().await?;
    repo::lock_default(&mut tx, workspace_id).await?;
    if repo::slug_taken(&mut *tx, workspace_id, &slug).await? {
        return Err(AppError::field("slug", SLUG_TAKEN));
    }
    let default = match input.default_warehouse {
        Some(true) => {
            repo::clear_default(&mut *tx, workspace_id, None).await?;
            true
        }
        Some(false) => false,
        None => !repo::has_default(&mut *tx, workspace_id).await?,
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
    tx.commit().await?;
    Ok(id)
}

/// Patch a live warehouse; `default_warehouse: true` moves the default here.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: WarehousePatch,
) -> Result<(), AppError> {
    let id = parse(id)?;
    check_people(state, workspace_id, &patch.fields).await?;
    let mut tx = state.db.begin().await?;
    if patch.default_warehouse == Some(true) {
        repo::lock_default(&mut tx, workspace_id).await?;
        repo::clear_default(&mut *tx, workspace_id, Some(id)).await?;
    }
    let found = repo::update(
        &mut *tx,
        workspace_id,
        id,
        &patch.name,
        patch.default_warehouse,
        &patch.fields,
    )
    .await?;
    if !found {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Laravel SetDefaultWarehouse: unset the old default, set this one.
pub async fn set_default(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    repo::lock_default(&mut tx, workspace_id).await?;
    repo::clear_default(&mut *tx, workspace_id, Some(id)).await?;
    if !repo::set_default(&mut *tx, workspace_id, id).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Soft delete. Children keep their parent id and stay listed via `roots_only=0`.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
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
    match e.as_database_error().and_then(|d| d.constraint()) {
        Some("warehouses_slug_unique") => AppError::field("slug", SLUG_TAKEN),
        _ => AppError::Database(e),
    }
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
