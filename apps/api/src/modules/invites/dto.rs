//! Invite request/response bodies. Keys match vimdesk-web invitation.service.js.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        auth::dto::{NewPerson, PersonFields, required_email},
        invites::{repo::WorkspaceInvite, service::StatusView},
        teams::dto::TeamResource,
    },
};

/// Laravel `rules.ValidateId` for attribute `team id`.
pub const BAD_TEAM: &str = "The team id is an invalid id for the team domain.";

/// POST /v1/workspace/{workspace}/invites. `team_id`: team the invitee joins on accept.
#[derive(Deserialize, ToSchema)]
/// Invite someone by email, optionally straight into a team.
pub struct SendInviteRequest {
    #[schema(value_type = String, format = Email, max_length = 255)]
    email: Option<String>,
    // Any JSON: a number or junk must be a `team_id` error, not a body parse error.
    /// Team the invitee joins on accepting.
    #[schema(value_type = Option<uuid::Uuid>)]
    team_id: Option<Value>,
}

pub struct SendInvite {
    pub email: String,
    pub team_id: Option<Uuid>,
}

impl Validate for SendInviteRequest {
    type Valid = SendInvite;

    fn validate(self, r: &mut Rules) -> Option<SendInvite> {
        let email = required_email(r, "email", self.email);
        let team_id = match self.team_id {
            None | Some(Value::Null) => Some(None),
            // Laravel ConvertEmptyStringsToNull: blank = no team.
            Some(Value::String(s)) if s.trim().is_empty() => Some(None),
            Some(Value::String(s)) => Uuid::parse_str(s.trim()).ok().map(Some),
            Some(_) => None,
        };
        r.check("team_id", team_id.is_some(), BAD_TEAM);
        Some(SendInvite {
            email: email?,
            team_id: team_id?,
        })
    }
}

/// POST /v1/register: new account through an invite.
#[derive(Deserialize, ToSchema)]
/// Create an account through an invite link.
pub struct RegisterRequest {
    #[serde(flatten)]
    person: PersonFields,
    /// Code from the invite link.
    #[schema(value_type = String)]
    invite_code: Option<String>,
}

pub struct NewInvitee {
    pub person: NewPerson,
    pub invite_code: String,
}

impl Validate for RegisterRequest {
    type Valid = NewInvitee;

    fn validate(self, r: &mut Rules) -> Option<NewInvitee> {
        let person = self.person.validate(r);
        let invite_code = r.required("invite_code", self.invite_code);
        Some(NewInvitee {
            person: person?,
            invite_code: invite_code?,
        })
    }
}

/// POST /v1/invites: logged-in user accepts or declines.
#[derive(Deserialize, ToSchema)]
/// Accept (`true`) or decline (`false`) an invite as the logged-in user.
pub struct AnswerRequest {
    #[schema(value_type = bool)]
    accept: Option<bool>,
    /// Code from the invite link.
    #[schema(value_type = String)]
    invite_code: Option<String>,
}

pub struct Answer {
    pub accept: bool,
    pub invite_code: String,
}

impl Validate for AnswerRequest {
    type Valid = Answer;

    fn validate(self, r: &mut Rules) -> Option<Answer> {
        if self.accept.is_none() {
            r.missing("accept");
        }
        let invite_code = r.required("invite_code", self.invite_code);
        Some(Answer {
            accept: self.accept?,
            invite_code: invite_code?,
        })
    }
}

#[derive(Serialize, ToSchema)]
pub struct InviteCreated {
    pub invite_id: Uuid,
}

// GET /v1/invites/get-invite-status/{code} body (Laravel InviteStatusResource keys).
#[derive(Serialize, ToSchema)]
/// What an invite link points to, for the accept or sign-up screen.
pub struct InviteStatusResponse {
    id: Uuid,
    /// `pending` | `accepted` | `denied` | `expired`.
    #[schema(value_type = String, example = "pending")]
    status: &'static str,
    // True when no account uses the invited email yet (frontend shows signup form).
    /// No account uses this email yet: show the sign-up form.
    new_user: bool,
    workspace_name: String,
    email: String,
    invited_on: DateTime<Utc>,
}

impl From<StatusView> for InviteStatusResponse {
    fn from(view: StatusView) -> Self {
        Self {
            id: view.invite.id,
            status: view.status,
            new_user: view.new_user,
            workspace_name: view.invite.workspace_name,
            email: view.invite.email,
            invited_on: view.invite.created_at,
        }
    }
}

// One settings pending-invites row (Laravel InviteResource). `invite_code` is left out: only
// its hash is stored, and the link goes to the invitee alone.
#[derive(Serialize, ToSchema)]
/// A sent invite, as listed in workspace settings.
pub struct InviteResource {
    invite_id: Uuid,
    email: String,
    #[schema(example = "pending")]
    invite_status: String,
    // Inviter email (Laravel sent the email string).
    /// Email of whoever sent the invite.
    invited_by: String,
    expired_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    relationships: InviteRelationships,
}

#[derive(Serialize, ToSchema)]
struct InviteRelationships {
    /// Team the invitee joins on accepting.
    #[schema(required = true)]
    team: Option<TeamResource>,
}

impl From<WorkspaceInvite> for InviteResource {
    fn from(i: WorkspaceInvite) -> Self {
        Self {
            invite_id: i.id,
            email: i.email,
            invite_status: i.status,
            invited_by: i.inviter_email,
            expired_at: i.expires_at,
            meta: None,
            created_at: i.created_at,
            last_updated_at: i.updated_at,
            relationships: InviteRelationships {
                team: i.team.map(Into::into),
            },
        }
    }
}

/// GET /v1/workspace/{workspace}/invites query. Unknown params ignored.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InvitesQuery {
    /// Only invites with this status, e.g. `pending`.
    #[serde(rename = "filter[invite_status]")]
    pub status: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}
