//! Email profile bodies (Laravel Store/UpdateEmailProfileRequest: the same rules) and the
//! list query.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::{
        auth::token::PlainSecret,
        pdf_templates::dto::{Conditions, Patch, conditions},
        pricing::body::{Capped, Scalar},
        settings::{credentials::MASK, dto::Change},
    },
};

/// SMTP fields that say where the password goes: the mask keeps a stored password only while
/// they stay as stored.
pub const BOUND: &[&str] = &["host", "port", "username", "encryption"];

/// Body of create and update; unknown keys (`meta`, which Laravel couldn't store) are skipped.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ProfileBody {
    name: Scalar,
    sender_name: Scalar,
    email: Scalar,
    default: Scalar,
    html: Scalar,
    smtp: Capped,
    conditions: Capped,
}

/// `POST /email-profiles` and `POST /email-profiles/{id}`.
#[derive(Deserialize)]
pub struct ProfileRequest(ProfileBody);

/// The profile's own SMTP server: settings without the password, and what to do with it.
pub struct SmtpInput {
    /// `host`, `username`, `port`, `encryption`, `bcc_all_emails` (Laravel dropped
    /// `from_name` and `from_address` here).
    pub settings: Value,
    pub password: Change,
}

/// A validated body; patches keep (None), clear (Some(None)) or set.
pub struct ProfileInput {
    pub name: String,
    pub sender_name: String,
    pub email: String,
    pub is_default: bool,
    pub html: Patch<String>,
    pub smtp: Patch<SmtpInput>,
    pub conditions: Patch<Conditions>,
}

/// `{data:{email_profile_id}}`.
#[derive(Serialize)]
pub struct ProfileCreated {
    pub email_profile_id: Uuid,
}

/// List query (Laravel ListEmailProfileRequest).
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ListQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
    pub relation_type: Option<String>,
    pub language: Option<String>,
    pub relation_custom_type_id: Option<String>,
}

impl Validate for ProfileRequest {
    type Valid = ProfileInput;

    fn validate(self, r: &mut Rules) -> Option<ProfileInput> {
        let b = self.0;
        let name = required(r, "name", &b.name, 80);
        let sender_name = required(r, "sender_name", &b.sender_name, 80);
        let email = required(r, "email", &b.email, 255);
        if !email.is_empty() {
            r.email("email", &email);
        }
        let is_default = match b.default.filled() {
            None => {
                r.missing("default");
                false
            }
            Some(v) => r.boolean("default", &v).unwrap_or_default(),
        };
        let html = match &b.html {
            Scalar::Absent => None,
            v => match v.filled() {
                None => Some(None),
                Some(Value::String(s)) => {
                    r.max_chars("html", &s, 25_000);
                    Some(Some(s))
                }
                Some(_) => {
                    r.add("html", "The html must be a string.");
                    None
                }
            },
        };
        Some(ProfileInput {
            name,
            sender_name,
            email,
            is_default,
            html,
            smtp: smtp(r, &b.smtp),
            conditions: conditions(r, &b.conditions),
        })
    }
}

/// Required trimmed text of at most `max` characters.
fn required(r: &mut Rules, field: &str, v: &Scalar, max: usize) -> String {
    match v.filled() {
        None => {
            r.missing(field);
            String::new()
        }
        Some(Value::String(s)) => {
            let s = s.trim().to_owned();
            r.max_chars(field, &s, max);
            s
        }
        Some(_) => {
            r.add(field, format!("The {} must be a string.", human(field)));
            String::new()
        }
    }
}

/// `smtp`: null clears, an object replaces (its password by `Change`), left out keeps.
fn smtp(r: &mut Rules, v: &Capped) -> Patch<SmtpInput> {
    let obj = match v {
        Capped::Absent => return None,
        Capped::Value(Value::Null) => return Some(None),
        Capped::TooLarge => {
            r.add("smtp", "The smtp is too large.");
            return None;
        }
        Capped::Value(Value::Object(o)) => o,
        Capped::Value(_) => {
            r.add("smtp", "The smtp must be an array.");
            return None;
        }
    };
    let mut settings = Map::new();
    for key in ["host", "username"] {
        let f = format!("smtp.{key}");
        let value = match obj.get(key) {
            None | Some(Value::Null) => Value::Null,
            Some(Value::String(s)) if s.trim().is_empty() => Value::Null,
            Some(Value::String(s)) => {
                r.max_chars(&f, s.trim(), 255);
                Value::from(s.trim())
            }
            Some(_) => {
                r.add(&f, format!("The {} must be a string.", human(&f)));
                Value::Null
            }
        };
        settings.insert(key.to_owned(), value);
    }
    let port = match obj.get("port") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) if s.trim().is_empty() => Value::Null,
        Some(v) => r
            .integer("smtp.port", v, 1, 65_535)
            .map_or(Value::Null, Value::from),
    };
    settings.insert("port".into(), port);
    let encryption = match obj.get("encryption") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) if s.trim().is_empty() => Value::Null,
        Some(Value::String(s)) if matches!(s.trim().to_lowercase().as_str(), "ssl" | "tls") => {
            Value::from(s.trim().to_lowercase())
        }
        Some(_) => {
            r.add(
                "smtp.encryption",
                "Invalid smtp.encryption encryption type.",
            );
            Value::Null
        }
    };
    settings.insert("encryption".into(), encryption);
    let bcc = match obj.get("bcc_all_emails") {
        None | Some(Value::Null) => Value::Null,
        Some(v) => r
            .boolean("smtp.bcc_all_emails", v)
            .map_or(Value::Null, Value::from),
    };
    settings.insert("bcc_all_emails".into(), bcc);
    let password = match obj.get("password") {
        None | Some(Value::Null) => Change::Clear,
        Some(Value::String(s)) if s.trim().is_empty() => Change::Clear,
        Some(Value::String(s)) if s == MASK => Change::Keep,
        Some(Value::String(s)) => {
            r.max_chars("smtp.password", s, 255);
            Change::Set(PlainSecret::new(s.clone()))
        }
        Some(_) => {
            r.add("smtp.password", "The smtp.password must be a string.");
            Change::Clear
        }
    };
    Some(Some(SmtpInput {
        settings: Value::Object(settings),
        password,
    }))
}
