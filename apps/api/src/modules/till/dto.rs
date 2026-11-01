//! Till request/response bodies: registers, till users, login and the session.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        auth::password::Password,
        till::repo::{RegisterRow, SessionRow, UserRow},
    },
};

const NAME_MAX: usize = 100;
const BAD_PIN: &str = "The pin must be 4 to 8 digits.";

/// Malformed = unknown: nil matches no row, so the service gives the one message.
fn id(r: &mut Rules, field: &str, v: Option<String>) -> Uuid {
    r.required(field, v)
        .map(|v| Uuid::parse_str(&v).unwrap_or(Uuid::nil()))
        .unwrap_or_default()
}

/// 4 to 8 ASCII digits, never trimmed.
fn pin(r: &mut Rules, v: Option<Password>) -> Option<Password> {
    let Some(v) = v else {
        r.missing("pin");
        return None;
    };
    let s = v.expose();
    r.check(
        "pin",
        (4..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()),
        BAD_PIN,
    );
    Some(v)
}

/// A register: one till device with its own cash drawer, selling from a store warehouse.
#[derive(Serialize, ToSchema)]
pub struct RegisterResource {
    id: Uuid,
    name: String,
    warehouse_id: Uuid,
    currency_id: Uuid,
    /// First 8 characters of the live device key; null when it was revoked.
    #[schema(required = true, example = "gsk_3f9a")]
    key_prefix: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<RegisterRow> for RegisterResource {
    fn from(r: RegisterRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            warehouse_id: r.warehouse_id,
            currency_id: r.currency_id,
            key_prefix: r.key_prefix,
            created_at: r.created_at,
        }
    }
}

/// A register with its device key, shown this once.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A register and its device key. Store `key` on the device now: it is never shown again."
)]
pub struct RegisterKeyed {
    #[serde(flatten)]
    pub register: RegisterResource,
    /// The device sends it as `Authorization: Bearer <key>` to log a till user in.
    #[schema(example = "gsk_3f9a…")]
    pub key: String,
}

impl RegisterKeyed {
    /// From the register row and the plaintext key.
    pub fn new(r: RegisterRow, key: String) -> Self {
        Self {
            register: r.into(),
            key,
        }
    }
}

/// POST /till/registers.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "New register. The warehouse must be a store; the currency defaults to the main one."
)]
pub struct CreateRegisterRequest {
    #[schema(value_type = String, max_length = 100, example = "Till 1")]
    name: Option<String>,
    #[schema(value_type = Uuid)]
    warehouse_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    currency_id: Option<String>,
}

/// Validated register input.
pub struct NewRegister {
    pub name: String,
    pub warehouse_id: Uuid,
    pub currency_id: Option<Uuid>,
}

impl Validate for CreateRegisterRequest {
    type Valid = NewRegister;

    fn validate(self, r: &mut Rules) -> Option<NewRegister> {
        Some(NewRegister {
            name: r.text("name", self.name, NAME_MAX),
            warehouse_id: id(r, "warehouse_id", self.warehouse_id),
            currency_id: crate::http::json::optional(self.currency_id)
                .map(|c| Uuid::parse_str(&c).unwrap_or(Uuid::nil())),
        })
    }
}

/// A till user (shop staff); never the PIN.
#[derive(Serialize, ToSchema)]
pub struct TillUserResource {
    id: Uuid,
    name: String,
    /// The workspace member this till user is, when linked.
    #[schema(required = true)]
    user_id: Option<Uuid>,
    active: bool,
    /// Locked after every 5th wrong PIN: 15 minutes, then twice as long each time.
    locked: bool,
    created_at: DateTime<Utc>,
}

impl From<UserRow> for TillUserResource {
    fn from(u: UserRow) -> Self {
        Self {
            id: u.id,
            name: u.name,
            user_id: u.user_id,
            active: u.active,
            locked: u.locked,
            created_at: u.created_at,
        }
    }
}

/// POST /till/users.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New till user with a PIN of 4 to 8 digits.")]
pub struct CreateTillUserRequest {
    #[schema(value_type = String, max_length = 100, example = "Mary")]
    name: Option<String>,
    #[schema(value_type = String, pattern = "^[0-9]{4,8}$", example = "4821")]
    pin: Option<Password>,
    /// Link to a workspace member.
    #[schema(value_type = Option<Uuid>)]
    user_id: Option<String>,
}

/// Validated till user input.
pub struct NewTillUser {
    pub name: String,
    pub pin: Password,
    pub user_id: Option<Uuid>,
}

impl Validate for CreateTillUserRequest {
    type Valid = NewTillUser;

    fn validate(self, r: &mut Rules) -> Option<NewTillUser> {
        let name = r.text("name", self.name, NAME_MAX);
        let pin = pin(r, self.pin)?;
        Some(NewTillUser {
            name,
            pin,
            user_id: crate::http::json::optional(self.user_id)
                .map(|u| Uuid::parse_str(&u).unwrap_or(Uuid::nil())),
        })
    }
}

/// PATCH /till/users/{user}: leave a field out to keep it.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to a till user. A new PIN clears a lock; inactive users can't log in and their sessions end."
)]
pub struct UpdateTillUserRequest {
    #[schema(value_type = Option<String>, max_length = 100)]
    name: Option<String>,
    #[schema(value_type = Option<String>, pattern = "^[0-9]{4,8}$")]
    pin: Option<Password>,
    active: Option<bool>,
}

/// Validated patch.
pub struct TillUserPatch {
    pub name: Option<String>,
    pub pin: Option<Password>,
    pub active: Option<bool>,
}

impl Validate for UpdateTillUserRequest {
    type Valid = TillUserPatch;

    fn validate(self, r: &mut Rules) -> Option<TillUserPatch> {
        Some(TillUserPatch {
            name: self.name.map(|n| r.text("name", Some(n), NAME_MAX)),
            pin: match self.pin {
                Some(p) => pin(r, Some(p)),
                None => None,
            },
            active: self.active,
        })
    }
}

/// POST /till/login.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Till user and PIN, sent with the register's device key.")]
pub struct TillLoginRequest {
    #[schema(value_type = Uuid)]
    till_user_id: Option<String>,
    #[schema(value_type = String, example = "4821")]
    pin: Option<Password>,
}

/// Validated login input.
pub struct Login {
    pub till_user_id: Uuid,
    pub pin: Password,
}

impl Validate for TillLoginRequest {
    type Valid = Login;

    fn validate(self, r: &mut Rules) -> Option<Login> {
        let till_user_id = id(r, "till_user_id", self.till_user_id);
        let Some(pin) = self.pin else {
            r.missing("pin");
            return None;
        };
        Some(Login { till_user_id, pin })
    }
}

/// A till user in the login picker or a session.
#[derive(Serialize, ToSchema)]
pub struct TillUserBrief {
    pub id: Uuid,
    pub name: String,
}

/// The register a session sells on.
#[derive(Serialize, ToSchema)]
pub struct RegisterBrief {
    id: Uuid,
    name: String,
    /// Store warehouse the till takes stock from.
    warehouse_id: Uuid,
    currency_id: Uuid,
    currency: RegisterCurrency,
    /// Notes and coins the drawer takes (`cash_register.accepted_cash`), in minor units,
    /// ascending; values that aren't a whole minor unit are left out.
    #[schema(example = json!([5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000]))]
    accepted_cash: Vec<i64>,
}

/// The register's currency.
#[derive(Serialize, ToSchema)]
pub struct RegisterCurrency {
    /// ISO 4217.
    #[schema(example = "EUR")]
    code: String,
    /// Where the decimal point goes in minor units.
    #[schema(example = 2)]
    decimals: i16,
}

/// GET /till/me: who is logged in on which register, until when.
#[derive(Serialize, ToSchema)]
pub struct SessionResource {
    register: RegisterBrief,
    user: TillUserBrief,
    expires_at: DateTime<Utc>,
}

impl SessionResource {
    /// The session `s`; `accepted_cash` in minor units of its register's currency.
    pub fn new(s: &SessionRow, accepted_cash: Vec<i64>) -> Self {
        Self {
            register: RegisterBrief {
                id: s.register_id,
                name: s.register_name.clone(),
                warehouse_id: s.warehouse_id,
                currency_id: s.currency_id,
                currency: RegisterCurrency {
                    code: s.currency_code.clone(),
                    decimals: s.currency_decimals,
                },
                accepted_cash,
            },
            user: TillUserBrief {
                id: s.till_user_id,
                name: s.user_name.clone(),
            },
            expires_at: s.expires_at,
        }
    }
}

/// POST /till/login reply: the session token (8 hours, no refresh) and the session.
#[derive(Serialize, ToSchema)]
pub struct TillLoginResource {
    /// Send as `Authorization: Bearer <token>` on till routes.
    pub token: String,
    #[serde(flatten)]
    pub session: SessionResource,
}
