//! Farm membership request/response bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::{
        farm::{ADMIN, FIELD_OFFICER, OPS},
        json::{Rules, Validate},
    },
    modules::farm_memberships::repo::MembershipRow,
};

/// Roles this route grants. Farmers get theirs when an officer issues their login.
const GRANTABLE: &[&str] = &[FIELD_OFFICER, OPS, ADMIN];

/// The member a membership belongs to.
#[derive(Serialize, ToSchema)]
pub struct MemberRef {
    id: Uuid,
    name: String,
}

/// A farm role held by one user in this workspace.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A farm role. ops and admin cover every village; farmers and field \
    officers one village."
)]
pub struct MembershipResource {
    id: Uuid,
    user: MemberRef,
    /// `farmer`, `field_officer`, `ops` or `admin`.
    role: String,
    #[schema(required = true)]
    village_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    /// Set once revoked; a revoked role grants nothing.
    #[schema(required = true)]
    revoked_at: Option<DateTime<Utc>>,
}

impl From<MembershipRow> for MembershipResource {
    fn from(m: MembershipRow) -> Self {
        Self {
            id: m.id,
            user: MemberRef {
                id: m.user_id,
                name: m.user_name,
            },
            role: m.role,
            village_id: m.village_id,
            created_at: m.created_at,
            revoked_at: m.revoked_at,
        }
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// POST /farm/memberships.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Grant a staff role to an active member of this workspace.")]
pub struct GrantRequest {
    #[schema(value_type = Uuid)]
    user_id: Option<String>,
    /// `field_officer`, `ops` or `admin`.
    #[schema(value_type = String, example = "field_officer")]
    role: Option<String>,
    /// Required for a field officer; left out for ops and admin.
    #[schema(value_type = Option<Uuid>)]
    village_id: Option<String>,
}

/// Validated grant.
pub struct NewGrant {
    pub user_id: Uuid,
    pub role: String,
    pub village_id: Option<Uuid>,
}

/// Required uuid field; junk is a 422 on that field.
fn uuid(r: &mut Rules, field: &str, value: Option<String>) -> Option<Uuid> {
    let v = r.required(field, value)?;
    let id = Uuid::parse_str(&v).ok();
    r.check(
        field,
        id.is_some(),
        format!("The {} must be a valid UUID.", field.replace('_', " ")),
    );
    id
}

impl Validate for GrantRequest {
    type Valid = NewGrant;

    fn validate(self, r: &mut Rules) -> Option<NewGrant> {
        let user_id = uuid(r, "user_id", self.user_id);
        let role = r.required("role", self.role.clone()).and_then(|_| {
            r.one_of(
                "role",
                self.role,
                GRANTABLE,
                Some(
                    "role must be admin, ops or field_officer (farmers get logins from an officer)",
                ),
            )
        });
        let village_id = match (role.as_deref(), self.village_id) {
            (Some(FIELD_OFFICER), v) => match v.filter(|s| !s.trim().is_empty()) {
                None => {
                    r.add("village_id", "a field officer needs a village");
                    None
                }
                v => uuid(r, "village_id", v),
            },
            (Some(role), Some(v)) if !v.trim().is_empty() => {
                r.add(
                    "village_id",
                    format!("{role} covers the whole workspace: leave village out"),
                );
                None
            }
            _ => None,
        };
        Some(NewGrant {
            user_id: user_id?,
            role: role?,
            village_id,
        })
    }
}
