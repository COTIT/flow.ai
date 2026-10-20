//! "Who am I" rules: profile, per-workspace permissions, invites waiting for this user. No HTTP types.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        invites::repo::{self as invites, PendingInvite},
        users::repo::{self as users, Profile},
        workspaces::repo::{self as workspaces, MemberWorkspace},
    },
};

/// Everything GET /v1/me shows.
pub struct Me {
    pub profile: Profile,
    pub workspaces: Vec<MemberWorkspace>,
    pub pending_invites: Vec<PendingInvite>,
}

pub async fn me(state: &AppState, user_id: Uuid) -> Result<Me, AppError> {
    let profile = users::find_profile(&state.db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let workspaces = workspaces::active_for_user(&state.db, user_id).await?;
    // Unverified accounts may be squatting someone else's address: show them no invites.
    let pending_invites = if profile.email_verified {
        invites::pending_for_email(&state.db, &profile.email).await?
    } else {
        Vec::new()
    };
    Ok(Me {
        profile,
        workspaces,
        pending_invites,
    })
}

pub async fn workspaces(state: &AppState, user_id: Uuid) -> Result<Vec<MemberWorkspace>, AppError> {
    Ok(workspaces::active_for_user(&state.db, user_id).await?)
}

/// Permission names for this user in `ws`.
// shortcut: owner-only until roles/teams are ported; members get none, same as a Laravel
// invite without a team.
pub fn permissions(user_id: Uuid, ws: &MemberWorkspace) -> Vec<&'static str> {
    if ws.created_by_user_id == user_id {
        vec!["workspace_owner"]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn ws(owner: Uuid) -> MemberWorkspace {
        MemberWorkspace {
            id: Uuid::nil(),
            name: "acme".into(),
            workspace_type: "retail".into(),
            created_by_user_id: owner,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn owner_gets_workspace_owner_member_gets_nothing() {
        let owner = Uuid::from_u128(1);
        let member = Uuid::from_u128(2);

        assert_eq!(permissions(owner, &ws(owner)), vec!["workspace_owner"]);
        assert!(permissions(member, &ws(owner)).is_empty());
    }
}
