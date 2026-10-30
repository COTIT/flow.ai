//! Request/response bodies for auth endpoints. Field names match vimdesk-web payloads.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, optional},
    modules::{
        auth::{
            password::{self, Password},
            service::Session,
        },
        users::repo::NewUser,
        workspaces::model::{WorkspaceType, is_slug},
    },
};

/// Laravel `max:55` on names and workspace name.
const NAME_MAX: usize = 55;
pub const EMAIL_MAX: usize = 255;
const SLUG_MESSAGE: &str =
    "The workspace name field must be lowercase letters, numbers and single dashes.";

/// New-account fields shared by signup and invite register. `#[serde(flatten)]` it into a request.
/// Every field `Option`, so a missing one is a 422 field error instead of a parse error.
#[derive(Deserialize)]
pub struct PersonFields {
    first_name: Option<String>,
    middle_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    password: Option<Password>,
}

/// Validated new-account input. Text trimmed, password not.
pub struct NewPerson {
    pub first_name: String,
    pub middle_name: Option<String>,
    pub last_name: String,
    pub email: String,
    pub password: Password,
}

impl NewPerson {
    /// Insert payload once password is hashed.
    pub fn as_new_user<'a>(&'a self, password_hash: &'a str) -> NewUser<'a> {
        NewUser {
            email: &self.email,
            first_name: &self.first_name,
            middle_name: self.middle_name.as_deref(),
            last_name: &self.last_name,
            password_hash,
        }
    }
}

impl PersonFields {
    pub fn validate(self, r: &mut Rules) -> Option<NewPerson> {
        let first_name = r.required("first_name", self.first_name);
        let middle_name = optional(self.middle_name);
        let last_name = r.required("last_name", self.last_name);
        for (field, value) in [
            ("first_name", &first_name),
            ("middle_name", &middle_name),
            ("last_name", &last_name),
        ] {
            if let Some(v) = value {
                r.max_chars(field, v, NAME_MAX);
            }
        }
        let email = required_email(r, "email", self.email);
        let password = password::check(r, "password", self.password);

        Some(NewPerson {
            first_name: first_name?,
            middle_name,
            last_name: last_name?,
            email: email?,
            password: password?,
        })
    }
}

/// Required + email shape + max 255.
pub fn required_email(r: &mut Rules, field: &str, value: Option<String>) -> Option<String> {
    let email = r.required(field, value);
    if let Some(e) = &email {
        r.email(field, e);
        r.max_chars(field, e, EMAIL_MAX);
    }
    email
}

/// POST /v1/create-workspace body. Unknown fields (`meta`) ignored.
#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    #[serde(flatten)]
    person: PersonFields,
    workspace_name: Option<String>,
    workspace_type: Option<String>,
}

/// Validated signup input.
pub struct NewSignup {
    pub person: NewPerson,
    pub workspace_name: String,
    pub workspace_type: WorkspaceType,
}

impl Validate for CreateWorkspaceRequest {
    type Valid = NewSignup;

    fn validate(self, r: &mut Rules) -> Option<NewSignup> {
        let person = self.person.validate(r);

        let workspace_name = r.required("workspace_name", self.workspace_name);
        if let Some(n) = &workspace_name {
            r.max_chars("workspace_name", n, NAME_MAX);
            r.check("workspace_name", is_slug(n), SLUG_MESSAGE);
        }

        let workspace_type = match optional(self.workspace_type) {
            None => Some(WorkspaceType::default()),
            Some(raw) => {
                let parsed = WorkspaceType::parse(&raw);
                r.check(
                    "workspace_type",
                    parsed.is_some(),
                    "The selected workspace type is invalid.",
                );
                parsed
            }
        };

        Some(NewSignup {
            person: person?,
            workspace_name: workspace_name?,
            workspace_type: workspace_type?,
        })
    }
}

/// POST /v1/login body.
#[derive(Deserialize)]
pub struct LoginRequest {
    email: Option<String>,
    password: Option<Password>,
}

/// Validated login input. No strength policy: it applies when setting a password, not checking one.
pub struct LoginInput {
    pub email: String,
    pub password: Password,
}

impl Validate for LoginRequest {
    type Valid = LoginInput;

    fn validate(self, r: &mut Rules) -> Option<LoginInput> {
        let email = r.required("email", self.email);
        if let Some(e) = &email {
            r.email("email", e);
        }
        let password = self.password.filter(|p| !p.expose().is_empty());
        match &password {
            None => r.missing("password"),
            // No stored password can be longer, so skip the argon2 run on oversized input.
            Some(p) => r.max_chars("password", p.expose(), password::MAX_CHARS),
        }
        Some(LoginInput {
            email: email?,
            password: password?,
        })
    }
}

/// `{data:{access_token, token_type, workspace_ids}}`, same keys as Laravel login/register.
#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub workspace_ids: Vec<Uuid>,
}

impl From<Session> for TokenResponse {
    fn from(session: Session) -> Self {
        Self {
            access_token: session.token.expose().to_owned(),
            token_type: "Bearer",
            workspace_ids: session.workspace_ids,
        }
    }
}
