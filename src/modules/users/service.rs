//! "Who am I" rules: profile, per-workspace permissions, invites waiting for this user. No HTTP types.

use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        invites::repo::{self as invites, PendingInvite},
        permissions::model::Granted,
        teams::repo::{self as teams, TeamRow},
        users::repo::{self as users, Member, MemberFilter, Profile},
        workspaces::repo::{self as workspaces, MemberWorkspace},
    },
};

/// Everything GET /v1/me shows.
pub struct Me {
    pub profile: Profile,
    pub workspaces: Vec<MemberWorkspace>,
    pub teams: Vec<TeamRow>,
    pub pending_invites: Vec<PendingInvite>,
}

impl Me {
    /// Effective permissions in `ws`, from the teams already loaded (no extra query).
    pub fn granted(&self, ws: &MemberWorkspace) -> Granted {
        Granted::new(
            ws.created_by_user_id == self.profile.id,
            self.teams
                .iter()
                .filter(|t| t.workspace_id == ws.id)
                .flat_map(|t| t.permissions.iter().cloned()),
        )
    }
}

pub async fn me(state: &AppState, user_id: Uuid) -> Result<Me, AppError> {
    let profile = users::find_profile(&state.db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let workspaces = workspaces::active_for_user(&state.db, user_id).await?;
    let teams = teams::for_member(&state.db, user_id).await?;
    // Unverified accounts may be squatting someone else's address: show them no invites.
    let pending_invites = if profile.email_verified {
        invites::pending_for_email(&state.db, &profile.email).await?
    } else {
        Vec::new()
    };
    Ok(Me {
        profile,
        workspaces,
        teams,
        pending_invites,
    })
}

pub async fn workspaces(state: &AppState, user_id: Uuid) -> Result<Vec<MemberWorkspace>, AppError> {
    Ok(workspaces::active_for_user(&state.db, user_id).await?)
}

/// Member plus their teams in one workspace.
pub struct MemberView {
    pub member: Member,
    pub teams: Vec<TeamRow>,
}

impl MemberView {
    /// Effective permissions in this workspace.
    pub fn granted(&self) -> Granted {
        Granted::new(
            self.member.owner,
            self.teams
                .iter()
                .flat_map(|t| t.permissions.iter().cloned()),
        )
    }
}

/// One page of the users tab: 2 queries whatever the page size (3 past the end).
pub async fn members(
    state: &AppState,
    workspace_id: Uuid,
    filter: &MemberFilter<'_>,
) -> Result<(Vec<MemberView>, i64), AppError> {
    let (members, mut total) = users::workspace_members(&state.db, workspace_id, filter).await?;
    if members.is_empty() && filter.offset > 0 {
        total = users::count_members(&state.db, workspace_id, filter).await?;
    }
    let ids: Vec<Uuid> = members.iter().map(|m| m.id).collect();
    let mut by_user: HashMap<Uuid, Vec<TeamRow>> = HashMap::new();
    for t in teams::of_users_in(&state.db, workspace_id, &ids).await? {
        by_user.entry(t.user_id).or_default().push(t.team);
    }
    let views = members
        .into_iter()
        .map(|member| MemberView {
            teams: by_user.remove(&member.id).unwrap_or_default(),
            member,
        })
        .collect();
    Ok((views, total))
}

/// One member of this workspace with their teams there. Malformed or foreign id = 404.
pub async fn member(
    state: &AppState,
    workspace_id: Uuid,
    user: &str,
) -> Result<MemberView, AppError> {
    let user_id = Uuid::parse_str(user).map_err(|_| AppError::NotFound)?;
    let member = users::find_member(&state.db, workspace_id, user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let teams = teams::of_users_in(&state.db, workspace_id, &[user_id])
        .await?
        .into_iter()
        .map(|t| t.team)
        .collect();
    Ok(MemberView { member, teams })
}
