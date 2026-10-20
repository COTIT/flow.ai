//! Invite request/response bodies. Keys match vimdesk-web invitation.service.js.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        auth::dto::{NewPerson, PersonFields, required_email},
        invites::service::StatusView,
    },
};

/// POST /v1/workspace/{workspace}/invites. `team_id` accepted and ignored: teams not ported.
#[derive(Deserialize)]
pub struct SendInviteRequest {
    email: Option<String>,
}

impl Validate for SendInviteRequest {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        required_email(r, "email", self.email)
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
