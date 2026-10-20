//! Invite request/response bodies. Keys match vimdesk-web invitation.service.js.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        auth::dto::{NewPerson, PersonFields, required_email},
        invites::service::StatusView,
    },
};

/// Laravel `rules.ValidateId` for attribute `team id`.
pub const BAD_TEAM: &str = "The team id is an invalid id for the team domain.";

/// POST /v1/workspace/{workspace}/invites. `team_id`: team the invitee joins on accept.
#[derive(Deserialize)]
pub struct SendInviteRequest {
    email: Option<String>,
    /// Any JSON: a number or junk must be a `team_id` error, not a body parse error.
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
#[derive(Deserialize)]
pub struct RegisterRequest {
    #[serde(flatten)]
    person: PersonFields,
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
#[derive(Deserialize)]
pub struct AnswerRequest {
    accept: Option<bool>,
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

#[derive(Serialize)]
pub struct InviteCreated {
    pub invite_id: Uuid,
}

/// GET /v1/invites/get-invite-status/{code} body (Laravel InviteStatusResource keys).
#[derive(Serialize)]
pub struct InviteStatusResponse {
    id: Uuid,
    /// `pending` | `accepted` | `denied` | `expired`.
    status: &'static str,
    /// True when no account uses the invited email yet (frontend shows signup form).
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
