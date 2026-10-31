//! Farm membership rules (RUAHA `membership` policies): admin grants any role, ops grants
//! field officers only, nobody touches their own rows. A workspace admin may do all of it,
//! own rows included: that is how a new workspace gets its first farm admin.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        farm::{FARMER, FIELD_OFFICER, FarmAccess},
        workspace::NOT_ALLOWED,
    },
    modules::farm_memberships::{
        dto::NewGrant,
        repo::{self, MembershipRow},
    },
};

/// May the caller grant or revoke `role` for `user_id`?
fn may_touch(access: &FarmAccess, user_id: Uuid, role: &str) -> bool {
    access.is_workspace_admin()
        || (user_id != access.user_id
            && (access.is_admin() || (access.manages() && [FARMER, FIELD_OFFICER].contains(&role))))
}

/// Managers and workspace admins see every membership; others only their own.
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MembershipRow>, i64), AppError> {
    let only = (!access.manages() && !access.is_workspace_admin()).then_some(access.user_id);
    Ok(repo::list(&state.db, access.workspace_id, only, limit, offset).await?)
}

/// Grant a staff role. True when a new row was created (201), false for a repeat or restore.
pub async fn grant(
    state: &AppState,
    access: &FarmAccess,
    input: NewGrant,
) -> Result<(MembershipRow, bool), AppError> {
    if !may_touch(access, input.user_id, &input.role) {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let ws = access.workspace_id;
    if let Some(v) = input.village_id
        && !repo::village_exists(&state.db, ws, v).await?
    {
        return Err(AppError::field(
            "village_id",
            "no such village in this workspace",
        ));
    }
    if !repo::is_active_member(&state.db, ws, input.user_id).await? {
        return Err(AppError::field(
            "user_id",
            "The user must be an active member of this workspace.",
        ));
    }
    let mut tx = access.begin(&state.db).await?;
    let (id, created) = repo::grant(
        &mut *tx,
        ws,
        input.user_id,
        &input.role,
        input.village_id,
        access.user_id,
    )
    .await?;
    let row = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok((row, created))
}

/// Revoke a membership of this workspace; again is fine.
pub async fn revoke(state: &AppState, access: &FarmAccess, id: &str) -> Result<(), AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let row = repo::find(&state.db, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !may_touch(access, row.user_id, &row.role) {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let mut tx = access.begin(&state.db).await?;
    repo::revoke(&mut *tx, access.workspace_id, id).await?;
    tx.commit().await?;
    Ok(())
}
