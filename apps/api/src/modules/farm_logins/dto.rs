//! Farmer login request/response bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::auth::password::Password,
};

/// A temporary password, shown this once.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "Read the phone and temporary password to the farmer now: the password \
    is never shown again. They must choose their own at first sign-in."
)]
pub struct IssuedLogin {
    /// The login name: the person's phone, normalised.
    #[schema(example = "+255712345678")]
    pub phone: String,
    #[schema(example = "k7m2q9xd")]
    pub temp_password: String,
    /// `initial` (new login) or `reset` (new password; every session of the farmer ended).
    pub kind: String,
}

/// Who issued a login.
#[derive(Serialize, ToSchema)]
pub struct IssuerRef {
    pub id: Uuid,
    pub name: String,
}

/// One issue in a person's login history.
#[derive(Serialize, ToSchema)]
pub struct LoginIssue {
    pub issued_at: DateTime<Utc>,
    /// `initial` or `reset`.
    pub kind: String,
    pub issued_by: IssuerRef,
    /// The account still has to replace its temporary password.
    pub must_change_password: bool,
}

/// One farm role of the signed-in account.
#[derive(Serialize, ToSchema)]
pub struct FarmMembershipRef {
    /// The membership id.
    id: Uuid,
    workspace_id: Uuid,
    workspace_name: String,
    /// `farmer`, `field_officer`, `ops` or `admin`.
    role: String,
    #[schema(required = true)]
    village_id: Option<Uuid>,
    #[schema(required = true)]
    village_name: Option<String>,
    /// The registered person this account is in that workspace, if any.
    #[schema(required = true)]
    person_id: Option<Uuid>,
}

/// A farmer session.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "Send `access_token` as `Authorization: Bearer <token>`. While \
    `must_change_password` is true, send the farmer to choose a password first."
)]
pub struct FarmSession {
    pub access_token: String,
    #[schema(value_type = String, example = "Bearer")]
    pub token_type: &'static str,
    pub must_change_password: bool,
    /// Where this account has farm roles.
    #[schema(value_type = Vec<FarmMembershipRef>)]
    pub memberships: Value,
}

/// POST /v1/farm/login.
#[derive(Deserialize, ToSchema)]
pub struct FarmLoginRequest {
    /// Any spelling of the registered phone.
    #[schema(value_type = String, example = "0712 345 678")]
    phone: Option<String>,
    #[schema(value_type = String, format = Password)]
    password: Option<Password>,
}

/// Validated sign-in.
pub struct FarmLogin {
    pub phone: String,
    pub password: Password,
}

impl Validate for FarmLoginRequest {
    type Valid = FarmLogin;

    fn validate(self, r: &mut Rules) -> Option<FarmLogin> {
        let phone = r.required("phone", self.phone);
        let password = match self.password.filter(|p| !p.expose().is_empty()) {
            Some(p) => Some(p),
            None => {
                r.missing("password");
                None
            }
        };
        Some(FarmLogin {
            phone: phone?,
            password: password?,
        })
    }
}

/// POST /v1/farm/password.
#[derive(Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// The password signed in with (the temporary one at first sign-in).
    #[schema(value_type = String, format = Password)]
    current_password: Option<Password>,
    /// 8 to 128 characters. Not the temporary password, not a straight run or one repeated
    /// character, not an obvious word, not the phone number.
    #[schema(value_type = String, format = Password)]
    password: Option<Password>,
}

/// Validated change.
pub struct PasswordChange {
    pub current: Password,
    pub new: Password,
}

impl Validate for ChangePasswordRequest {
    type Valid = PasswordChange;

    fn validate(self, r: &mut Rules) -> Option<PasswordChange> {
        let mut take = |field: &str, v: Option<Password>| match v.filter(|p| !p.expose().is_empty())
        {
            Some(p) => Some(p),
            None => {
                r.missing(field);
                None
            }
        };
        let current = take("current_password", self.current_password);
        let new = take("password", self.password);
        Some(PasswordChange {
            current: current?,
            new: new?,
        })
    }
}

/// The signed-in account as the farm apps need it.
#[derive(Serialize, ToSchema)]
pub struct MeUser {
    pub id: Uuid,
    /// Full name.
    pub name: String,
    #[schema(required = true)]
    pub email: Option<String>,
    #[schema(required = true)]
    pub phone: Option<String>,
}

/// GET /v1/farm/me: the account, its language and every farm role, for phone and email accounts.
#[derive(Serialize, ToSchema)]
pub struct FarmMe {
    pub user: MeUser,
    /// Interface language: `sw`, `en` or `nl`.
    #[schema(required = true)]
    pub locale: Option<String>,
    /// The account still has to replace a temporary password.
    pub must_change_password: bool,
    #[schema(value_type = Vec<FarmMembershipRef>)]
    pub memberships: Value,
}

/// PATCH /v1/farm/me.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MePatch {
    /// `sw`, `en` or `nl`.
    #[schema(value_type = String, example = "sw")]
    locale: Option<String>,
}

impl Validate for MePatch {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        r.required("locale", self.locale.clone())
            .and_then(|_| r.one_of("locale", self.locale, &["sw", "en", "nl"], None))
    }
}
