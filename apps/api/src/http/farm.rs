//! Farm guard: Bearer + `{workspace}` -> the caller's farm roles there. Staff roles count only
//! while the caller is an active workspace member; farmer roles need no membership, so farmers
//! reach farm routes and nothing else. No live role and no membership -> 404.

use axum::{
    extract::{FromRequestParts, RawPathParams},
    http::request::Parts,
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{auth::AuthUser, slots::Slot},
    modules::{farm_memberships::repo as farm, permissions::repo},
};

/// Farm role names, as stored.
pub const FARMER: &str = "farmer";
pub const FIELD_OFFICER: &str = "field_officer";
pub const OPS: &str = "ops";
pub const ADMIN: &str = "admin";

/// One live role: farmers and field officers carry their village, ops and admin none.
#[derive(Debug)]
pub struct FarmRole {
    pub role: String,
    pub village_id: Option<Uuid>,
}

/// Villages a staff check covers: every village (`all`) or the listed ones. Bind as
/// `($n::bool OR village_id = ANY($m))`.
pub struct Scope {
    pub all: bool,
    pub villages: Vec<Uuid>,
}

/// Caller's standing in the farm of the workspace in the path.
#[derive(Debug)]
pub struct FarmAccess {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    /// The registered person this account is, if any: what a farmer reads is theirs.
    pub person_id: Option<Uuid>,
    roles: Vec<FarmRole>,
    workspace_admin: bool,
    /// This request's slots (per workspace and per user), held until the handler returns.
    _slot: Slot,
}

impl FarmAccess {
    fn has(&self, names: &[&str]) -> bool {
        self.roles.iter().any(|r| names.contains(&r.role.as_str()))
    }

    /// Owner or `workspace_admin` holder: may grant any farm role, own rows included.
    pub fn is_workspace_admin(&self) -> bool {
        self.workspace_admin
    }

    /// Farm admin: grants any role, authors surveys.
    pub fn is_admin(&self) -> bool {
        self.has(&[ADMIN])
    }

    /// ops or admin: whole-workspace staff.
    pub fn manages(&self) -> bool {
        self.has(&[OPS, ADMIN])
    }

    /// Any staff role (field officer, ops, admin).
    pub fn is_staff(&self) -> bool {
        self.has(&[FIELD_OFFICER, OPS, ADMIN])
    }

    /// Villages where the caller acts as staff. A farmer in one village and officer in another
    /// is staff only in the second.
    pub fn staff_scope(&self) -> Scope {
        Scope {
            all: self.manages(),
            villages: self
                .roles
                .iter()
                .filter(|r| r.role == FIELD_OFFICER)
                .filter_map(|r| r.village_id)
                .collect(),
        }
    }

    /// True when the caller is staff in `village`.
    pub fn staff_in(&self, village: Uuid) -> bool {
        let s = self.staff_scope();
        s.all || s.villages.contains(&village)
    }

    /// Write transaction with the caller stamped as `genesy.actor`, which the farm audit
    /// trigger records on every row it logs.
    pub async fn begin(&self, db: &PgPool) -> Result<Transaction<'static, Postgres>, AppError> {
        let mut tx = db.begin().await?;
        farm::stamp_actor(&mut *tx, self.user_id).await?;
        Ok(tx)
    }
}

impl FromRequestParts<AppState> for FarmAccess {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        let params = RawPathParams::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::NotFound)?;
        let raw = params
            .iter()
            .find_map(|(k, v)| (k == "workspace").then_some(v))
            .ok_or_else(|| AppError::Internal("route has no {workspace} segment".into()))?;
        let workspace_id = Uuid::parse_str(raw).map_err(|_| AppError::NotFound)?;
        let member = repo::access(&state.db, workspace_id, user.user_id)
            .await?
            .filter(|a| a.active);
        let mut roles = farm::live_roles(&state.db, workspace_id, user.user_id).await?;
        if member.is_none() {
            roles.retain(|r| r.role == FARMER);
            if roles.is_empty() {
                return Err(AppError::NotFound);
            }
        }
        let person_id = farm::person_of(&state.db, workspace_id, user.user_id).await?;
        let workspace_admin = member.is_some_and(|a| {
            crate::modules::permissions::model::Granted::new(a.owner, a.permissions).is_admin()
        });
        // After the role check: only insiders can take a workspace's slots.
        let slot = state.slots.take(user.user_id, workspace_id).await?;
        Ok(Self {
            user_id: user.user_id,
            workspace_id,
            person_id,
            roles,
            workspace_admin,
            _slot: slot,
        })
    }
}
