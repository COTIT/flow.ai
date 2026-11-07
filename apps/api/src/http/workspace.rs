//! Workspace extractors: Bearer + `{workspace}` path segment -> caller's rights there.
//! Outsider, unknown, malformed or deleted workspace -> 404 (existence not leaked);
//! inactive member -> 403. Laravel `AccessToWorkspace` + `MainPolicy`.

use std::{marker::PhantomData, ops::Deref};

use axum::{
    extract::{FromRequestParts, RawPathParams},
    http::request::Parts,
};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{auth::AuthUser, slots::Slot},
    modules::permissions::{model::Granted, repo},
};

/// Laravel AuthorizationException text.
pub const NOT_ALLOWED: &str = "This action is unauthorized.";

/// Active member of the workspace in the path. Take it directly for member-only routes;
/// use `Can<P>` when the route needs a permission.
#[derive(Debug)]
pub struct WorkspaceAccess {
    pub user_id: Uuid,
    /// Current login, binding assistant sessions without retaining bearer tokens.
    pub token_id: Uuid,
    pub workspace_id: Uuid,
    granted: Granted,
    /// This request's slots (per workspace and per user), held until the handler returns.
    _slot: Slot,
}

impl WorkspaceAccess {
    /// True for the workspace creator.
    pub fn is_owner(&self) -> bool {
        self.granted.is_owner()
    }

    /// Owner or holder of `workspace_admin`: may manage teams that hold `workspace_admin`.
    pub fn is_admin(&self) -> bool {
        self.granted.is_admin()
    }

    /// Same check as `Can<P>`, for a rule that only applies to some request bodies.
    pub fn allows(&self, name: &str) -> bool {
        self.granted.allows(name)
    }
}

impl FromRequestParts<AppState> for WorkspaceAccess {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        // Phone-only accounts pass here and fail the membership check: 404, not 403.
        let user = AuthUser::any(parts, state).await?;
        let workspace_id = path_workspace(parts, state).await?;
        let access = repo::access(&state.db, workspace_id, user.user_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if !access.active {
            return Err(AppError::Forbidden(NOT_ALLOWED));
        }
        // After the membership check: only members can take a workspace's slots.
        let slot = state.slots.take(user.user_id, workspace_id).await?;
        Ok(Self {
            user_id: user.user_id,
            token_id: user.token_id,
            workspace_id,
            granted: Granted::new(access.owner, access.permissions),
            _slot: slot,
        })
    }
}

/// The `{workspace}` path segment; malformed -> 404.
pub(crate) async fn path_workspace(parts: &mut Parts, state: &AppState) -> Result<Uuid, AppError> {
    let params = RawPathParams::from_request_parts(parts, state)
        .await
        .map_err(|_| AppError::NotFound)?;
    let raw = params
        .iter()
        .find_map(|(k, v)| (k == "workspace").then_some(v))
        .ok_or_else(|| AppError::Internal("route has no {workspace} segment".into()))?;
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// A permission name as a type, so `Can<P>` can check it while extracting.
pub trait Permission {
    /// `group.action` from the catalog.
    const NAME: &'static str;
    /// A second name the route needs as well (Laravel request + policy checks).
    const ALSO: Option<&'static str> = None;
}

/// `WorkspaceAccess` that also holds permission `P`, else 403. Runs before the body is
/// parsed, so a caller without rights gets 403 before any 422 (Laravel authorize() order).
pub struct Can<P: Permission>(WorkspaceAccess, PhantomData<P>);

impl<P: Permission> Deref for Can<P> {
    type Target = WorkspaceAccess;

    fn deref(&self) -> &WorkspaceAccess {
        &self.0
    }
}

impl<P: Permission + Send> FromRequestParts<AppState> for Can<P> {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let access = WorkspaceAccess::from_request_parts(parts, state).await?;
        if !access.granted.allows(P::NAME) || P::ALSO.is_some_and(|n| !access.granted.allows(n)) {
            return Err(AppError::Forbidden(NOT_ALLOWED));
        }
        Ok(Self(access, PhantomData))
    }
}

/// Declare permission marker types: `permissions!(TeamCreate => "team.create");`.
macro_rules! permissions {
    ($($ty:ident => $name:literal),* $(,)?) => {
        $(
            #[doc = concat!("`", $name, "`")]
            pub struct $ty;
            impl $crate::http::workspace::Permission for $ty {
                const NAME: &'static str = $name;
            }
        )*
    };
}
pub(crate) use permissions;
