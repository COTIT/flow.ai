//! Team rules: CRUD, membership, permission grants. Every call is scoped to one workspace.
//! A team holding `workspace_admin` only changes hands through an admin (owner or
//! `workspace_admin`), so `team.invite` / `invite.create` / `team.permissions` can't become
//! admin rights (Laravel had no such gate). No HTTP types here.

use std::collections::BTreeSet;

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::NOT_ALLOWED,
    modules::{
        permissions::catalog::{WORKSPACE_ADMIN, assignable},
        teams::{
            dto::{
                BAD_USERS, NOT_IN_TEAM, NewTeam, PermissionChangeReceipt, PermissionChangeStatus,
                PermissionEntries, TeamCreated, TeamPatch,
            },
            repo::{self, ListFilter, Patch, TeamRow, TeamUser},
        },
    },
};

/// Who is calling: decides the `workspace_admin` rules.
#[derive(Clone, Copy)]
pub struct Caller {
    pub is_owner: bool,
    /// Owner or `workspace_admin` holder.
    pub is_admin: bool,
}

/// Team row plus the users needed to render it.
pub struct TeamView {
    pub team: TeamRow,
    pub users: Vec<TeamUser>,
}

/// One list page.
pub struct TeamPage {
    pub teams: Vec<TeamRow>,
    pub users: Vec<TeamUser>,
    pub total: i64,
}

/// Grant names a caller may set: exact catalog names, plus `workspace_admin` from the owner only
/// (Laravel PermissionHelper::getExistingPermissions). Lowercased, unknown dropped.
fn grantable(raw: impl IntoIterator<Item = String>, by_owner: bool) -> Vec<String> {
    raw.into_iter()
        .map(|p| p.trim().to_lowercase())
        .filter(|p| assignable(p) || (by_owner && p == WORKSPACE_ADMIN))
        .collect()
}

fn permission_changes(
    entries: &PermissionEntries,
    effective: &[String],
    delta: bool,
) -> PermissionChangeReceipt {
    let mut requested = BTreeSet::new();
    let mut ignored_count = entries.ignored_count;
    for entry in &entries.names {
        let entry = entry.trim();
        let (remove, name) = if delta {
            entry
                .strip_prefix('-')
                .map_or((false, entry), |name| (true, name.trim()))
        } else {
            (false, entry)
        };
        let name = name.to_lowercase();
        if assignable(&name) || name == WORKSPACE_ADMIN {
            requested.insert(if remove { format!("-{name}") } else { name });
        } else {
            ignored_count += 1;
        }
    }
    let (applied, not_applied): (Vec<_>, Vec<_>) = requested.into_iter().partition(|entry| {
        let (remove, name) = entry
            .strip_prefix('-')
            .map_or((false, entry.as_str()), |name| (true, name));
        effective.iter().any(|permission| permission == name) != remove
    });
    PermissionChangeReceipt {
        status: if not_applied.is_empty() && ignored_count == 0 {
            PermissionChangeStatus::Applied
        } else {
            PermissionChangeStatus::Partial
        },
        applied,
        not_applied,
        ignored_count,
    }
}

pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    caller: Caller,
    input: NewTeam,
) -> Result<TeamCreated, AppError> {
    let perms = grantable(input.permissions.names.iter().cloned(), caller.is_owner);
    let mut tx = state.db.begin().await?;
    let id = repo::insert(
        &mut *tx,
        workspace_id,
        &input.name,
        &input.description,
        input.meta.as_ref(),
    )
    .await?;
    repo::grant(&mut *tx, workspace_id, id, &perms).await?;
    let saved = repo::find(&mut *tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let permission_changes = permission_changes(&input.permissions, &saved.permissions, false);
    tx.commit().await?;
    Ok(TeamCreated {
        team_id: id,
        permission_changes,
    })
}

pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    filter: &ListFilter<'_>,
) -> Result<TeamPage, AppError> {
    let (teams, mut total) = repo::list(&state.db, workspace_id, filter).await?;
    if teams.is_empty() && filter.offset > 0 {
        total = repo::count(&state.db, workspace_id, filter.pattern).await?;
    }
    let ids: Vec<Uuid> = teams.iter().map(|t| t.id).collect();
    let users = repo::users_of(&state.db, &ids).await?;
    Ok(TeamPage {
        teams,
        users,
        total,
    })
}

pub async fn get(state: &AppState, workspace_id: Uuid, team: &str) -> Result<TeamView, AppError> {
    let team = repo::find(&state.db, workspace_id, parse(team)?)
        .await?
        .ok_or(AppError::NotFound)?;
    let users = repo::users_of(&state.db, &[team.id]).await?;
    Ok(TeamView { team, users })
}

pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    team: &str,
    patch: TeamPatch,
) -> Result<(), AppError> {
    let p = Patch {
        name: patch.name.as_deref(),
        description: patch.description.as_deref(),
        meta: patch.meta.as_ref().map(Option::as_ref),
    };
    found(repo::update(&state.db, workspace_id, parse(team)?, &p).await?)
}

/// Soft delete. Deleting an admin team would strip every admin but the owner: admins only.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    team: &str,
    caller: Caller,
) -> Result<(), AppError> {
    let team = guarded(state, workspace_id, team, caller).await?;
    found(repo::soft_delete(&state.db, workspace_id, team).await?)
}

/// Every id must be a member of this workspace (Laravel ValidateUsers).
pub async fn add_users(
    state: &AppState,
    workspace_id: Uuid,
    team: &str,
    caller: Caller,
    user_ids: Vec<Uuid>,
) -> Result<(), AppError> {
    let team = guarded(state, workspace_id, team, caller).await?;
    let known = repo::count_workspace_members(&state.db, workspace_id, &user_ids).await?;
    if known != user_ids.len() as i64 {
        return Err(AppError::field("user_ids", BAD_USERS));
    }
    Ok(repo::add_members(&state.db, workspace_id, team, &user_ids).await?)
}

/// Every id must already be in the team (Laravel UserExistInTeamRule).
pub async fn remove_users(
    state: &AppState,
    workspace_id: Uuid,
    team: &str,
    caller: Caller,
    user_ids: Vec<Uuid>,
) -> Result<(), AppError> {
    let team = guarded(state, workspace_id, team, caller).await?;
    let present = repo::count_in_team(&state.db, workspace_id, team, &user_ids).await?;
    if present != user_ids.len() as i64 {
        return Err(AppError::field("user_ids", NOT_IN_TEAM));
    }
    Ok(repo::remove_members(&state.db, workspace_id, team, &user_ids).await?)
}

/// Delta from the permission editor: `x` adds, `-x` removes, removals win (Laravel
/// AssignTeamPermission adds then removes). Malformed entries ignored. Removing
/// `workspace_admin` needs an admin.
pub async fn set_permissions(
    state: &AppState,
    workspace_id: Uuid,
    team: &str,
    caller: Caller,
    entries: PermissionEntries,
) -> Result<PermissionChangeReceipt, AppError> {
    let team = parse(team)?;
    let mut tx = state.db.begin().await?;
    found(repo::lock(&mut *tx, workspace_id, team).await?)?;
    let mut adds = Vec::new();
    let mut removes = Vec::new();
    for entry in &entries.names {
        match entry.trim().strip_prefix('-') {
            Some(name) => removes.push(name.trim().to_lowercase()),
            None => adds.push(entry.clone()),
        }
    }
    if removes.iter().any(|r| r == WORKSPACE_ADMIN) && !caller.is_admin {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let adds = grantable(adds, caller.is_owner);
    repo::grant(&mut *tx, workspace_id, team, &adds).await?;
    repo::revoke(&mut *tx, workspace_id, team, &removes).await?;
    let saved = repo::find(&mut *tx, workspace_id, team)
        .await?
        .ok_or(AppError::NotFound)?;
    let receipt = permission_changes(&entries, &saved.permissions, true);
    tx.commit().await?;
    Ok(receipt)
}

/// Malformed id = unknown id = 404.
fn parse(team: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(team).map_err(|_| AppError::NotFound)
}

fn found(updated: bool) -> Result<(), AppError> {
    updated.then_some(()).ok_or(AppError::NotFound)
}

/// Live team in this workspace: (id, holds `workspace_admin`), else 404.
async fn live(state: &AppState, workspace_id: Uuid, team: &str) -> Result<(Uuid, bool), AppError> {
    let id = parse(team)?;
    let admin = repo::admin_flag(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((id, admin))
}

/// Live team the caller may change members of: admin teams need an admin caller.
async fn guarded(
    state: &AppState,
    workspace_id: Uuid,
    team: &str,
    caller: Caller,
) -> Result<Uuid, AppError> {
    let (id, admin_team) = live(state, workspace_id, team).await?;
    if admin_team && !caller.is_admin {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(raw: &[&str], owner: bool) -> Vec<String> {
        grantable(raw.iter().map(|s| s.to_string()), owner)
    }

    #[test]
    fn grantable_keeps_catalog_names_lowercased() {
        assert_eq!(
            names(&[" Team.List ", "team.fly", "team"], false),
            ["team.list"]
        );
    }

    #[test]
    fn workspace_admin_only_from_owner() {
        assert!(names(&[WORKSPACE_ADMIN], false).is_empty());
        assert_eq!(names(&[WORKSPACE_ADMIN], true), [WORKSPACE_ADMIN]);
    }
}
