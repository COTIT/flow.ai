//! /v1/me and workspace user bodies. Keys follow Laravel UserResource / WorkspaceUserResource /
//! WorkspaceResource / PendingInviteResource, limited to what vimdesk-web reads plus the
//! profile-form round trip fields.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, optional},
    modules::{
        teams::dto::TeamResource,
        users::{
            repo::{Member, UserRef},
            service::{Me, MemberView},
        },
        workspaces::{dto::Profile, repo::MemberWorkspace},
    },
};

/// GET /v1/me body.
#[derive(Serialize, ToSchema)]
#[schema(description = "The logged-in user.")]
pub struct MeResponse {
    user: UserResource,
}

#[derive(Serialize, ToSchema)]
#[schema(description = "A user with their permissions, teams and pending invites.")]
struct UserResource {
    id: Uuid,
    // Laravel sent the int PK; we have only the uuid. Security.vue skips uuid-looking values.
    /// Same as `id`.
    user_id: Uuid,
    name: String,
    first_name: String,
    #[schema(required = true)]
    middle_name: Option<String>,
    last_name: String,
    #[schema(required = true)]
    email: Option<String>,
    initials: String,
    // PIN login not ported yet.
    /// Always `false` for now.
    has_pin_code: bool,
    /// Display color; not stored yet, always null.
    #[schema(required = true)]
    color: Option<String>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    permissions: Vec<WorkspacePermissions>,
    avatar_path: String,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    // No column yet; vimdesk-web falls back to `nl`.
    /// Not stored yet, always null.
    #[schema(required = true)]
    interface_language: Option<String>,
    active: bool,
    relationships: Relationships,
}

#[derive(Serialize, ToSchema)]
#[schema(description = "Effective permission names in one workspace.")]
struct WorkspacePermissions {
    // Same uuid as /me/workspaces[].id (auth.store.js matches on it).
    /// Workspace id.
    workspace: Uuid,
    permissions: Vec<String>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = UserRelationships)]
struct Relationships {
    pending_invites: Vec<PendingInviteResource>,
    teams: Vec<TeamResource>,
    // Employment groups not ported yet.
    /// Not supported yet, always null.
    #[schema(value_type = Option<Object>, required = true)]
    employment_group: Option<Value>,
}

#[derive(Serialize, ToSchema)]
#[schema(description = "An invite waiting for the user to accept or decline.")]
struct PendingInviteResource {
    id: Uuid,
    #[schema(value_type = String, example = "pending")]
    status: &'static str,
    /// The workspace slug.
    workspace_name: String,
    /// The workspace's profile `company_name`, or the slug when it has none. Show this.
    display_name: String,
    // Invite id: POST /v1/invites accepts it from a verified user with the invited email.
    /// Code to send to `POST /invites` to accept or decline.
    invite_code: String,
    invited_on: DateTime<Utc>,
    invited_by: Inviter,
}

#[derive(Serialize, ToSchema)]
struct Inviter {
    #[schema(required = true)]
    email: Option<String>,
    name: String,
}

/// One /v1/me/workspaces item.
#[derive(Serialize, ToSchema)]
#[schema(description = "A workspace the user belongs to.")]
pub struct WorkspaceResource {
    id: Uuid,
    /// The slug.
    workspace_name: String,
    /// The profile's `company_name`, or the slug when it has none. Show this.
    display_name: String,
    workspace_type: String,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    // Omitted until a profile is saved (Laravel `when`). POS receipts read it.
    /// Company profile; left out until one is saved. Bank details (`account`) are never
    /// included here.
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_profile: Option<Profile>,
    /// Always empty.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    relationships: [(); 0],
}

impl From<MemberWorkspace> for WorkspaceResource {
    fn from(ws: MemberWorkspace) -> Self {
        Self {
            id: ws.id,
            workspace_name: ws.name,
            display_name: ws.display_name,
            workspace_type: ws.workspace_type,
            created_at: ws.created_at,
            last_updated_at: ws.updated_at,
            workspace_profile: ws.profile.map(|p| Profile::from_stored(Some(p)).public()),
            relationships: [],
        }
    }
}

impl From<Me> for MeResponse {
    fn from(me: Me) -> Self {
        let permissions = me
            .workspaces
            .iter()
            .map(|ws| WorkspacePermissions {
                workspace: ws.id,
                permissions: me.granted(ws).to_list(),
            })
            .collect();
        let p = me.profile;
        let name = full_name(&p.first_name, p.middle_name.as_deref(), &p.last_name);
        let pending_invites = me
            .pending_invites
            .into_iter()
            .map(|i| PendingInviteResource {
                id: i.id,
                status: "pending",
                workspace_name: i.workspace_name,
                display_name: i.display_name,
                invite_code: i.id.to_string(),
                invited_on: i.created_at,
                invited_by: Inviter {
                    name: full_name(
                        &i.inviter_first_name,
                        i.inviter_middle_name.as_deref(),
                        &i.inviter_last_name,
                    ),
                    email: i.inviter_email,
                },
            })
            .collect();

        let person = Person {
            id: p.id,
            email: p.email,
            first_name: p.first_name,
            middle_name: p.middle_name,
            last_name: p.last_name,
            created_at: p.created_at,
            updated_at: p.updated_at,
        };
        let relationships = Relationships {
            pending_invites,
            teams: me.teams.into_iter().map(Into::into).collect(),
            employment_group: None,
        };
        Self {
            user: UserResource::new(person, name, true, permissions, relationships),
        }
    }
}

/// Fields UserResource needs from a users row.
struct Person {
    id: Uuid,
    email: Option<String>,
    first_name: String,
    middle_name: Option<String>,
    last_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl UserResource {
    fn new(
        p: Person,
        name: String,
        active: bool,
        permissions: Vec<WorkspacePermissions>,
        relationships: Relationships,
    ) -> Self {
        Self {
            id: p.id,
            user_id: p.id,
            initials: initials(&p.first_name, &p.last_name),
            avatar_path: avatar_path(&name),
            name,
            first_name: p.first_name,
            middle_name: p.middle_name,
            last_name: p.last_name,
            email: p.email,
            has_pin_code: false,
            color: None,
            created_at: p.created_at,
            last_updated_at: p.updated_at,
            permissions,
            meta: None,
            interface_language: None,
            active,
            relationships,
        }
    }
}

fn person(m: Member) -> (Person, String) {
    let name = full_name(&m.first_name, m.middle_name.as_deref(), &m.last_name);
    let p = Person {
        id: m.id,
        email: m.email,
        first_name: m.first_name,
        middle_name: m.middle_name,
        last_name: m.last_name,
        created_at: m.created_at,
        updated_at: m.updated_at,
    };
    (p, name)
}

// GET /v1/workspace/{ws}/users/{id} body: UserResource scoped to this workspace. Permissions,
// teams and invites of the user's other workspaces stay hidden (Laravel showed them all).
/// A member as seen from one workspace: only that workspace's permissions and teams.
#[derive(Serialize, ToSchema)]
#[serde(transparent)]
pub struct UserDetail(UserResource);

impl UserDetail {
    pub fn new(view: MemberView, workspace: Uuid) -> Self {
        let permissions = vec![WorkspacePermissions {
            workspace,
            permissions: view.granted().to_list(),
        }];
        let active = view.member.active;
        let (p, name) = person(view.member);
        let relationships = Relationships {
            pending_invites: Vec::new(),
            teams: view.teams.into_iter().map(Into::into).collect(),
            employment_group: None,
        };
        Self(UserResource::new(
            p,
            name,
            active,
            permissions,
            relationships,
        ))
    }
}

// One users-tab row (Laravel WorkspaceUserResource).
/// One row of the workspace members list.
#[derive(Serialize, ToSchema)]
pub struct MemberResource {
    id: Uuid,
    name: String,
    #[schema(required = true)]
    email: Option<String>,
    // Not ported.
    /// Not supported yet, always null.
    #[schema(required = true)]
    user_type: Option<String>,
    avatar_path: String,
    initials: String,
    /// Display color; not stored yet, always null.
    #[schema(required = true)]
    color: Option<String>,
    /// Membership flag in this workspace.
    active: bool,
    /// Exactly one entry: this workspace.
    #[schema(value_type = Vec<WorkspacePermissions>, min_items = 1, max_items = 1)]
    permissions: [WorkspacePermissions; 1],
    // Top-level here (users.vue reads `teams.0.name`), under `relationships` on the detail.
    /// The member's teams in this workspace.
    teams: Vec<MemberTeamResource>,
    // Employment group not loaded, so Laravel leaves `[]`.
    /// Always empty.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    relationships: [(); 0],
    created_at: DateTime<Utc>,
}

// Laravel WorkspaceTeamResource.
/// A team as listed on a member row.
#[derive(Serialize, ToSchema)]
struct MemberTeamResource {
    id: Uuid,
    name: String,
    description: String,
    permissions: Vec<String>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
}

impl MemberResource {
    pub fn new(view: MemberView, workspace: Uuid) -> Self {
        let permissions = view.granted().to_list();
        let m = view.member;
        let name = full_name(&m.first_name, m.middle_name.as_deref(), &m.last_name);
        Self {
            id: m.id,
            initials: initials(&m.first_name, &m.last_name),
            avatar_path: avatar_path(&name),
            name,
            email: m.email,
            user_type: None,
            color: None,
            active: m.active,
            permissions: [WorkspacePermissions {
                workspace,
                permissions,
            }],
            teams: view
                .teams
                .into_iter()
                .map(|t| MemberTeamResource {
                    id: t.id,
                    name: t.name,
                    description: t.description,
                    permissions: t.permissions,
                    meta: t.meta,
                })
                .collect(),
            relationships: [],
            created_at: m.created_at,
        }
    }
}

/// GET /v1/workspace/{ws}/users query. Unknown params ignored.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MembersQuery {
    /// Search text, max 100 characters.
    pub q: Option<String>,
    /// Only members of this team.
    #[param(value_type = Option<Uuid>)]
    pub team_id: Option<String>,
    /// `true` = only members with a PIN code.
    #[param(value_type = Option<bool>)]
    pub has_pin_code: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// Validated users-tab filter values.
pub struct MembersFilter {
    pub q: Option<String>,
    pub team_id: Option<Uuid>,
    pub has_pin_code: bool,
}

impl MembersQuery {
    /// Laravel ListWorkspaceUserRequest: q max 100, team_id uuid, has_pin_code boolean.
    pub fn filter(&self) -> Result<MembersFilter, AppError> {
        let mut r = Rules::default();
        let q = optional(self.q.as_deref().map(str::to_owned));
        if let Some(q) = &q {
            r.max_chars("q", q, 100);
        }
        let team_id =
            optional(self.team_id.as_deref().map(str::to_owned)).map(|t| Uuid::parse_str(&t));
        if matches!(team_id, Some(Err(_))) {
            r.add("team_id", "The team id must be a valid UUID.");
        }
        // Laravel `boolean` accepts these four (query values are strings).
        let has_pin_code = match self.has_pin_code.as_deref().map(str::trim) {
            None | Some("") | Some("0" | "false") => false,
            Some("1" | "true") => true,
            Some(_) => {
                r.add(
                    "has_pin_code",
                    "The has pin code field must be true or false.",
                );
                false
            }
        };
        r.into_result()?;
        Ok(MembersFilter {
            q,
            team_id: team_id.and_then(Result::ok),
            has_pin_code,
        })
    }
}

// Laravel UserEmailResource: a user embedded in another resource.
#[derive(Serialize, ToSchema)]
/// A user shown inside another record (team member, inviter, ...).
pub struct UserEmailResource {
    id: Uuid,
    name: String,
    initials: String,
    #[schema(required = true)]
    email: Option<String>,
    avatar_path: String,
    /// Display color; not stored yet, always null.
    #[schema(required = true)]
    color: Option<String>,
}

impl UserEmailResource {
    /// From the user's name parts.
    pub fn new(
        id: Uuid,
        first: &str,
        middle: Option<&str>,
        last: &str,
        email: Option<String>,
    ) -> Self {
        let name = full_name(first, middle, last);
        Self {
            id,
            initials: initials(first, last),
            avatar_path: avatar_path(&name),
            name,
            email,
            color: None,
        }
    }
}

impl From<&UserRef> for UserEmailResource {
    fn from(u: &UserRef) -> Self {
        Self::new(
            u.id,
            &u.first_name,
            u.middle_name.as_deref(),
            &u.last_name,
            u.email.clone(),
        )
    }
}

/// `first [middle ]last`, Laravel User::getNameAttribute.
pub fn full_name(first: &str, middle: Option<&str>, last: &str) -> String {
    match middle {
        Some(m) => format!("{first} {m} {last}"),
        None => format!("{first} {last}"),
    }
}

/// First letters of first and last name, uppercased.
pub fn initials(first: &str, last: &str) -> String {
    first
        .chars()
        .take(1)
        .chain(last.chars().take(1))
        .flat_map(char::to_uppercase)
        .collect()
}

/// Laravel parity: third-party avatar from the display name.
pub fn avatar_path(name: &str) -> String {
    format!("https://eu.ui-avatars.com/api/?name={}", form_encode(name))
}

/// application/x-www-form-urlencoded value: space -> `+`, unreserved kept, rest %XX.
fn form_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b' ' => out.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_name_with_and_without_middle() {
        assert_eq!(full_name("Ada", None, "Lovelace"), "Ada Lovelace");
        assert_eq!(
            full_name("Ada", Some("King"), "Lovelace"),
            "Ada King Lovelace"
        );
    }

    #[test]
    fn initials_uppercase_unicode_safe() {
        assert_eq!(initials("ada", "lovelace"), "AL");
        assert_eq!(initials("élodie", "ørsted"), "ÉØ");
    }

    #[test]
    fn form_encode_escapes_everything_unsafe() {
        assert_eq!(form_encode("Ada Lovelace"), "Ada+Lovelace");
        assert_eq!(form_encode("A&B=C"), "A%26B%3DC");
        assert_eq!(form_encode("Zoë"), "Zo%C3%AB");
    }
}
