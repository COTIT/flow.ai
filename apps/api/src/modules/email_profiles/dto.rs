//! Email profile bodies (Laravel Store/UpdateEmailProfileRequest: the same rules) and the
//! list query.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::{IntoParams, ToSchema};
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

// Body of create and update; unknown keys (`meta`, which Laravel couldn't store) are skipped.
/// Body of create and update; unknown keys are skipped.
#[derive(Deserialize, Default, ToSchema)]
#[schema(description = "An email profile (create and full update take the same body).")]
pub struct ProfileBody {
    #[schema(value_type = String, max_length = 80, required = true)]
    #[serde(default)]
    name: Scalar,
    #[schema(value_type = String, max_length = 80, required = true)]
    #[serde(default)]
    sender_name: Scalar,
    #[schema(value_type = String, format = Email, max_length = 255, required = true)]
    #[serde(default)]
    email: Scalar,
    #[schema(value_type = bool, required = true)]
    #[serde(default)]
    default: Scalar,
    /// Signature HTML; left out keeps it, null clears it.
    #[schema(value_type = Option<String>, max_length = 25000)]
    #[serde(default)]
    html: Scalar,
    /// Own SMTP server; left out keeps it, null clears it.
    #[schema(value_type = Option<SmtpBody>)]
    #[serde(default)]
    smtp: Capped,
    /// When this profile is picked; left out keeps them, null clears them.
    #[schema(value_type = Option<Conditions>)]
    #[serde(default)]
    conditions: Capped,
}

/// `POST /email-profiles` and `POST /email-profiles/{id}`.
#[derive(Deserialize, ToSchema)]
pub struct ProfileRequest(ProfileBody);

// Doc-only: the `smtp` object `smtp()` reads key by key.
/// A profile's SMTP server as sent.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct SmtpBody {
    /// Host name or public IP.
    host: Option<String>,
    #[schema(max_length = 255)]
    username: Option<String>,
    /// Send the mask back to keep the stored password; null or blank clears it.
    #[schema(max_length = 255)]
    password: Option<String>,
    #[schema(minimum = 1, maximum = 65535)]
    port: Option<u16>,
    /// `ssl` or `tls`.
    encryption: Option<String>,
    bcc_all_emails: Option<bool>,
}

// Doc-only: `smtp` as `service::get` prints it with `json!`.
/// A profile's SMTP server as shown; the password is masked.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct SmtpView {
    #[schema(required = true)]
    host: Option<String>,
    #[schema(required = true)]
    username: Option<String>,
    /// The mask when a password is stored, else null.
    #[schema(required = true)]
    password: Option<String>,
    #[schema(required = true)]
    port: Option<i64>,
    #[schema(required = true)]
    encryption: Option<String>,
    /// Always null.
    #[schema(value_type = Option<String>, required = true)]
    from_name: Option<String>,
    /// Always null.
    #[schema(value_type = Option<String>, required = true)]
    from_address: Option<String>,
    #[schema(required = true)]
    bcc_all_emails: Option<bool>,
}

// Doc-only: one list row as `service::summary` prints it with `json!`.
/// An email profile in a list.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct ProfileSummary {
    id: Uuid,
    name: String,
    sender_name: String,
    email: String,
    #[schema(rename = "default")]
    is_default: bool,
    #[schema(required = true)]
    html: Option<String>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

// Doc-only: one profile as `service::get` prints it with `json!`.
/// An email profile.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct ProfileResource {
    id: Uuid,
    name: String,
    sender_name: String,
    email: String,
    #[schema(required = true)]
    html: Option<String>,
    #[schema(rename = "default")]
    is_default: bool,
    smtp: SmtpView,
    #[schema(required = true)]
    conditions: Option<Conditions>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

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
#[derive(Serialize, ToSchema)]
pub struct ProfileCreated {
    pub email_profile_id: Uuid,
}

/// List query (Laravel ListEmailProfileRequest).
#[derive(Deserialize, Default, IntoParams)]
#[serde(default)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// Only profiles for `business` or `personal` relations.
    pub relation_type: Option<String>,
    /// Only profiles for this language.
    pub language: Option<String>,
    /// Only profiles for this relation custom type.
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
            Some(Value::String(s)) if key == "host" => {
                host(r, s.trim()).map_or(Value::Null, Value::from)
            }
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
            // A stored password must always have a server to go to.
            let no_host = obj
                .get("host")
                .is_none_or(|h| h.as_str().is_none_or(|s| s.trim().is_empty()));
            if no_host {
                r.add(
                    "smtp.host",
                    "The smtp.host field is required when smtp.password is present.",
                );
            }
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

/// A host name or public IP literal, lowercase and without a trailing dot. Private, loopback,
/// link-local and metadata literals are refused here; names are checked when dialled.
fn host(r: &mut Rules, sent: &str) -> Option<String> {
    let h = sent.trim_end_matches('.').to_lowercase();
    if h.chars().count() > 255 {
        r.max_chars("smtp.host", &h, 255);
        return None;
    }
    let literal = h.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = literal.parse::<std::net::IpAddr>() {
        if !crate::outbound::is_public(ip) {
            r.add("smtp.host", "The smtp.host must be a public address.");
            return None;
        }
        return Some(literal.to_owned());
    }
    let label_ok = |l: &str| {
        (1..=63).contains(&l.len())
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    if h.len() > 253 || !h.split('.').all(label_ok) {
        r.add("smtp.host", "The smtp.host must be a valid host name.");
        return None;
    }
    Some(h)
}
