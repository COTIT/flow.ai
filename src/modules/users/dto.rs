//! /v1/me response bodies. Keys follow Laravel UserResource / WorkspaceResource / PendingInviteResource,
//! limited to what vimdesk-web reads plus the profile-form round trip fields.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::modules::{
    users::service::{Me, permissions},
    workspaces::repo::MemberWorkspace,
};

#[derive(Serialize)]
pub struct MeResponse {
    user: UserResource,
}

#[derive(Serialize)]
struct UserResource {
    id: Uuid,
    /// Laravel sent the int PK; we have only the uuid. Security.vue skips uuid-looking values.
    user_id: Uuid,
    name: String,
    first_name: String,
    middle_name: Option<String>,
    last_name: String,
    email: String,
    initials: String,
    /// PIN login not ported yet.
    has_pin_code: bool,
    color: Option<String>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    permissions: Vec<WorkspacePermissions>,
    avatar_path: String,
    meta: Option<Value>,
    /// No column yet; vimdesk-web falls back to `nl`.
    interface_language: Option<String>,
    active: bool,
    relationships: Relationships,
}

#[derive(Serialize)]
struct WorkspacePermissions {
    /// Same uuid as /me/workspaces[].id (auth.store.js matches on it).
    workspace: Uuid,
    permissions: Vec<&'static str>,
}

#[derive(Serialize)]
struct Relationships {
    pending_invites: Vec<PendingInviteResource>,
    /// Teams and employment groups not ported yet.
    teams: Vec<Value>,
    employment_group: Option<Value>,
}

#[derive(Serialize)]
struct PendingInviteResource {
    id: Uuid,
    status: &'static str,
    workspace_name: String,
    /// Invite id: POST /v1/invites accepts it from a verified user with the invited email.
    invite_code: String,
    invited_on: DateTime<Utc>,
    invited_by: Inviter,
}

#[derive(Serialize)]
struct Inviter {
    email: String,
    name: String,
}

/// One /v1/me/workspaces item.
#[derive(Serialize)]
pub struct WorkspaceResource {
    id: Uuid,
    workspace_name: String,
    workspace_type: String,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    relationships: [(); 0],
}

impl From<MemberWorkspace> for WorkspaceResource {
    fn from(ws: MemberWorkspace) -> Self {
        Self {
            id: ws.id,
            workspace_name: ws.name,
            workspace_type: ws.workspace_type,
            created_at: ws.created_at,
            last_updated_at: ws.updated_at,
            relationships: [],
        }
    }
}

impl From<Me> for MeResponse {
    fn from(me: Me) -> Self {
        let p = me.profile;
        let name = full_name(&p.first_name, p.middle_name.as_deref(), &p.last_name);
        let permissions = me
            .workspaces
            .iter()
            .map(|ws| WorkspacePermissions {
                workspace: ws.id,
                permissions: permissions(p.id, ws),
            })
            .collect();
        let pending_invites = me
            .pending_invites
            .into_iter()
            .map(|i| PendingInviteResource {
                id: i.id,
                status: "pending",
                workspace_name: i.workspace_name,
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

        Self {
            user: UserResource {
                id: p.id,
                user_id: p.id,
                initials: initials(&p.first_name, &p.last_name),
                // Laravel parity: third-party avatar from the display name.
                avatar_path: format!("https://eu.ui-avatars.com/api/?name={}", form_encode(&name)),
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
                active: true,
                relationships: Relationships {
                    pending_invites,
                    teams: Vec::new(),
                    employment_group: None,
                },
            },
        }
    }
}

/// `first [middle ]last`, Laravel User::getNameAttribute.
fn full_name(first: &str, middle: Option<&str>, last: &str) -> String {
    match middle {
        Some(m) => format!("{first} {m} {last}"),
        None => format!("{first} {last}"),
    }
}

fn initials(first: &str, last: &str) -> String {
    first
        .chars()
        .take(1)
        .chain(last.chars().take(1))
        .flat_map(char::to_uppercase)
        .collect()
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
