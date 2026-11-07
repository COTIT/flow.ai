//! Team request/response bodies. Keys follow Laravel TeamResource and team.service.js.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{
    IntoParams, ToSchema,
    openapi::{
        Ref,
        schema::{ArrayBuilder, ObjectBuilder, OneOfBuilder, Schema},
    },
};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, optional, present},
    modules::{
        teams::repo::{TeamRow, TeamUser},
        users::dto::UserEmailResource,
    },
};

const NAME_MAX: usize = 55;
const DESCRIPTION_MAX: usize = 225;
const Q_MAX: usize = 100;
/// Laravel `rules.ValidateUsers` for attribute `user ids`.
pub const BAD_USERS: &str = "user ids must be an array of valid user_ids";
/// Laravel `rules.UserExistInTeam`.
pub const NOT_IN_TEAM: &str = "This user ids does not exist in the team";

// Team in Laravel TeamResource shape.
#[derive(Serialize, ToSchema)]
#[schema(description = "A team: a named group of members sharing a set of permissions.")]
pub struct TeamResource {
    id: Uuid,
    name: String,
    description: String,
    /// Permission names, e.g. `currency.create`.
    permissions: Vec<String>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    #[schema(schema_with = relationships_schema)]
    relationships: Relationships,
}

/// Wire shape of [`Relationships`]: untagged, so derive can't describe it.
fn relationships_schema() -> Schema {
    OneOfBuilder::new()
        .item(ArrayBuilder::new().max_items(Some(0)).build())
        .item(
            ObjectBuilder::new()
                .property(
                    "users",
                    ArrayBuilder::new().items(Ref::from_schema_name("UserEmailResource")),
                )
                .required("users"),
        )
        .description(Some(
            "`[]` when members were not loaded, otherwise `{\"users\": [...]}`",
        ))
        .into()
}

/// Laravel drops `users` when not loaded, leaving `[]`.
#[derive(Serialize)]
#[serde(untagged)]
enum Relationships {
    NotLoaded([(); 0]),
    Loaded { users: Vec<UserEmailResource> },
}

impl TeamResource {
    /// Teams with `relationships.users`, each team's members taken from `users`.
    pub fn page(teams: Vec<TeamRow>, users: Vec<TeamUser>) -> Vec<Self> {
        let mut by_team: HashMap<Uuid, Vec<UserEmailResource>> = HashMap::new();
        for u in users {
            by_team
                .entry(u.team_id)
                .or_default()
                .push(UserEmailResource::new(
                    u.id,
                    &u.first_name,
                    u.middle_name.as_deref(),
                    &u.last_name,
                    u.email,
                ));
        }
        teams
            .into_iter()
            .map(|t| {
                let users = by_team.remove(&t.id).unwrap_or_default();
                Self::build(t, Relationships::Loaded { users })
            })
            .collect()
    }

    fn build(t: TeamRow, relationships: Relationships) -> Self {
        Self {
            id: t.id,
            name: t.name,
            description: t.description,
            permissions: t.permissions,
            created_at: t.created_at,
            last_updated_at: t.updated_at,
            meta: t.meta,
            relationships,
        }
    }
}

/// As /me lists teams: users not loaded.
impl From<TeamRow> for TeamResource {
    fn from(t: TeamRow) -> Self {
        Self::build(t, Relationships::NotLoaded([]))
    }
}

/// Create response: the new team's id.
#[derive(Serialize, ToSchema)]
pub struct TeamCreated {
    pub team_id: Uuid,
    pub permission_changes: PermissionChangeReceipt,
}

/// Whether every requested permission change holds after writing.
#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionChangeStatus {
    Applied,
    Partial,
}

/// Known requested changes only; unrelated grants and raw unknown inputs stay private.
#[derive(Clone, Serialize, ToSchema)]
pub struct PermissionChangeReceipt {
    pub status: PermissionChangeStatus,
    pub applied: Vec<String>,
    pub not_applied: Vec<String>,
    pub ignored_count: u64,
}

/// Additive receipt preserves existing permission action message.
#[derive(Serialize, ToSchema)]
pub struct PermissionChangeReply {
    pub message: &'static str,
    pub permission_changes: PermissionChangeReceipt,
}

/// Validated array retains only strings and count of discarded non-strings.
pub struct PermissionEntries {
    pub names: Vec<String>,
    pub ignored_count: u64,
}

/// GET list query. Unknown params (`tab`, `team_id`, ...) ignored: vimdesk-web spreads its route query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on name, max 100 characters.
    pub q: Option<String>,
    /// `name`, `-name` or `-created_at`; anything else lists oldest first.
    pub sort: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

impl ListQuery {
    /// Trimmed search text, 422 past 100 chars (Laravel ListTeamRequest).
    pub fn search(&self) -> Result<Option<String>, crate::error::AppError> {
        let q = optional(self.q.as_deref().map(str::to_owned));
        let mut r = Rules::default();
        if let Some(q) = &q {
            r.max_chars("q", q, Q_MAX);
        }
        r.into_result()?;
        Ok(q)
    }
}

/// Laravel `nullable|array`: absent/null = empty, anything else but an array = 422.
fn optional_array(r: &mut Rules, field: &str, value: Option<Value>) -> Option<Vec<Value>> {
    match value {
        None | Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(items)) => Some(items),
        Some(_) => {
            r.add(field, format!("The {field} must be an array."));
            None
        }
    }
}

/// Strings among `items`, others dropped (Laravel skips non-strings).
fn strings(items: Vec<Value>) -> PermissionEntries {
    let mut entries = PermissionEntries {
        names: Vec::new(),
        ignored_count: 0,
    };
    for item in items {
        match item {
            Value::String(name) => entries.names.push(name),
            _ => entries.ignored_count += 1,
        }
    }
    entries
}

/// POST /users/teams.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New team.")]
pub struct CreateTeamRequest {
    #[schema(value_type = String, max_length = 55)]
    name: Option<String>,
    #[schema(value_type = String, max_length = 225)]
    description: Option<String>,
    /// Permission names to grant; unknown names are ignored.
    #[schema(value_type = Option<Vec<String>>)]
    permissions: Option<Value>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
}

pub struct NewTeam {
    pub name: String,
    pub description: String,
    /// Raw requested names; service filters them against the catalog.
    pub permissions: PermissionEntries,
    pub meta: Option<Value>,
}

fn capped(r: &mut Rules, field: &str, value: Option<String>, max: usize) -> Option<String> {
    let v = r.required(field, value)?;
    r.max_chars(field, &v, max);
    Some(v)
}

impl Validate for CreateTeamRequest {
    type Valid = NewTeam;

    fn validate(self, r: &mut Rules) -> Option<NewTeam> {
        let name = capped(r, "name", self.name, NAME_MAX);
        let description = capped(r, "description", self.description, DESCRIPTION_MAX);
        let permissions = optional_array(r, "permissions", self.permissions);
        let meta = r.meta(self.meta);
        Some(NewTeam {
            name: name?,
            description: description?,
            permissions: strings(permissions?),
            meta: meta?,
        })
    }
}

/// POST /users/teams/{team}: absent key = unchanged, `null`/blank on name or description = 422.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to a team. Leave a field out to keep it; send null `meta` to clear it."
)]
pub struct UpdateTeamRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = String, required = false, max_length = 55)]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = String, required = false, max_length = 225)]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

pub struct TeamPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    /// Some(None) clears.
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateTeamRequest {
    type Valid = TeamPatch;

    fn validate(self, r: &mut Rules) -> Option<TeamPatch> {
        let mut sent = |field: &str, v: Option<Option<String>>, max| match v {
            None => Some(None),
            Some(v) => capped(r, field, v, max).map(Some),
        };
        let name = sent("name", self.name, NAME_MAX);
        let description = sent("description", self.description, DESCRIPTION_MAX);
        let meta = match self.meta {
            None => Some(None),
            Some(v) => r.meta(v).map(Some),
        };
        Some(TeamPatch {
            name: name?,
            description: description?,
            meta: meta?,
        })
    }
}

/// POST /users/teams/{team}/add-users | remove-users.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Users to add to or remove from the team.")]
pub struct UserIdsRequest {
    #[schema(value_type = Vec<Uuid>, min_items = 1)]
    user_ids: Option<Value>,
}

impl Validate for UserIdsRequest {
    /// Deduped.
    type Valid = Vec<Uuid>;

    fn validate(self, r: &mut Rules) -> Option<Vec<Uuid>> {
        let items = match self.user_ids {
            Some(Value::Array(items)) if !items.is_empty() => items,
            None | Some(Value::Null) | Some(Value::Array(_)) => {
                r.missing("user_ids");
                return None;
            }
            Some(_) => {
                r.add("user_ids", BAD_USERS);
                return None;
            }
        };
        let ids: Option<Vec<Uuid>> = items
            .iter()
            .map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
            .collect();
        let Some(mut ids) = ids else {
            r.add("user_ids", BAD_USERS);
            return None;
        };
        ids.sort_unstable();
        ids.dedup();
        Some(ids)
    }
}

/// POST /users/teams/{team}/permissions.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Permission changes: `name` grants it, `-name` revokes it. Unknown names are ignored."
)]
pub struct PermissionsRequest {
    #[schema(value_type = Vec<String>, example = json!(["team.list", "-team.delete"]))]
    permissions: Option<Value>,
}

impl Validate for PermissionsRequest {
    /// Raw entries; `-x` means remove `x`.
    type Valid = PermissionEntries;

    fn validate(self, r: &mut Rules) -> Option<PermissionEntries> {
        let items = match self.permissions {
            // Laravel `required|array`: an empty array is "required" too, but the web app
            // never sends one and an empty delta is harmless, so it passes.
            Some(Value::Array(items)) => items,
            None | Some(Value::Null) => {
                r.missing("permissions");
                return None;
            }
            Some(_) => {
                r.add("permissions", "The permissions must be an array.");
                return None;
            }
        };
        Some(strings(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check<T: Validate + for<'de> Deserialize<'de>>(body: Value) -> (Option<T::Valid>, Rules) {
        let mut r = Rules::default();
        let v = serde_json::from_value::<T>(body).unwrap().validate(&mut r);
        (v, r)
    }

    #[test]
    fn update_tells_absent_from_null() {
        let (patch, r) = check::<UpdateTeamRequest>(json!({ "meta": null }));
        assert!(r.into_result().is_ok());
        let patch = patch.unwrap();
        assert!(patch.name.is_none() && patch.description.is_none());
        assert_eq!(patch.meta, Some(None));

        let (_, r) = check::<UpdateTeamRequest>(json!({ "name": null }));
        assert!(r.into_result().is_err());
    }

    #[test]
    fn user_ids_deduped() {
        let id = Uuid::from_u128(7).to_string();
        let (ids, _) = check::<UserIdsRequest>(json!({ "user_ids": [id, id] }));
        assert_eq!(ids.unwrap(), [Uuid::from_u128(7)]);
    }
}
