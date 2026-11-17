//! Connection bodies (contracts §5.2): input shape, per-provider config rules (§4.5) with the
//! address guard, and the masked resource.

use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, present},
    integrations::{capability::CapabilityCheck, connections::repo::ConnectionRow},
    modules::settings::credentials::MASK,
    outbound::is_public,
};

pub const PROVIDERS: &[&str] = &["netsuite", "odoo"];
const LABEL_MAX: usize = 100;
const SECRET_MAX: usize = 4000;
const TEXT_MAX: usize = 200;
const BASE_URL: &str = "The base url must be a public https address without a path.";
const REST_DOMAIN: &str = "The rest domain must be a public host name.";

/// The one secret a provider stores.
pub fn secret_name(provider: &str) -> &'static str {
    if provider == "odoo" {
        "api_key"
    } else {
        "private_key"
    }
}

/// Config key that says where the secret is sent.
pub fn bound_key(provider: &str) -> &'static str {
    if provider == "odoo" {
        "base_url"
    } else {
        "rest_domain"
    }
}

#[derive(Serialize, ToSchema)]
#[schema(
    description = "A connection to a customer system. Secrets are write-only: GET prints `********` for a stored one, null for none."
)]
pub struct ConnectionResource {
    id: Uuid,
    provider: String,
    label: String,
    status: String,
    #[schema(required = true)]
    status_reason: Option<String>,
    #[schema(value_type = Object)]
    config: Value,
    #[schema(value_type = Object, example = json!({"api_key": "********"}))]
    secrets: Value,
    #[schema(required = true)]
    credential_expires_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    last_probe: Option<Vec<CapabilityCheck>>,
    #[schema(required = true)]
    last_probe_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ConnectionResource {
    /// Resource with the mask for a stored secret.
    pub fn new(row: ConnectionRow, stored: bool) -> Self {
        let name = secret_name(&row.provider);
        let mut secrets = Map::new();
        secrets.insert(name.into(), if stored { MASK.into() } else { Value::Null });
        Self {
            id: row.id,
            secrets: Value::Object(secrets),
            last_probe: row.last_probe.and_then(|v| serde_json::from_value(v).ok()),
            provider: row.provider,
            label: row.label,
            status: row.status,
            status_reason: row.status_reason,
            config: row.config,
            credential_expires_at: row.credential_expires_at,
            last_probe_at: row.last_probe_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ConnectionCreated {
    pub connection_id: Uuid,
}

/// What happens to one secret. No Debug: `Set` holds plaintext.
pub enum SecretChange {
    /// The mask: keep the stored value.
    Keep,
    Set(String),
    Clear,
}

#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Create (all of provider, label, config) or patch (send only changed fields; provider is create-only). A secret sent as `********` keeps the stored one."
)]
pub struct ConnectionInput {
    #[schema(example = "odoo")]
    provider: Option<String>,
    #[schema(max_length = 100)]
    label: Option<String>,
    #[schema(value_type = Option<Object>)]
    config: Option<Value>,
    #[schema(value_type = Option<Object>, example = json!({"api_key": "plain or ********"}))]
    secrets: Option<Map<String, Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, format = DateTime)]
    credential_expires_at: Option<Option<String>>,
}

/// Shape-checked input; per-provider rules run in the service.
pub struct Input {
    pub provider: Option<String>,
    pub label: Option<String>,
    pub config: Option<Map<String, Value>>,
    pub secrets: Vec<(String, SecretChange)>,
    /// Absent = keep; Some(None) = clear.
    pub expires_at: Option<Option<DateTime<Utc>>>,
}

impl Validate for ConnectionInput {
    type Valid = Input;

    fn validate(self, r: &mut Rules) -> Option<Input> {
        let provider = self.provider.map(|p| p.trim().to_owned());
        if let Some(p) = &provider {
            r.check(
                "provider",
                PROVIDERS.contains(&p.as_str()),
                "The selected provider is invalid.",
            );
        }
        let label = self.label.map(|l| r.text("label", Some(l), LABEL_MAX));
        let config = match self.config {
            None => None,
            Some(Value::Object(m)) => Some(m),
            Some(_) => {
                r.add("config", "The config must be an object.");
                None
            }
        };
        let mut secrets = Vec::new();
        for (name, value) in self.secrets.unwrap_or_default() {
            let field = format!("secrets.{name}");
            let change = match value {
                Value::Null => SecretChange::Clear,
                Value::String(s) if s == MASK => SecretChange::Keep,
                Value::String(s) if !s.trim().is_empty() && s.chars().count() <= SECRET_MAX => {
                    SecretChange::Set(s)
                }
                _ => {
                    r.add(
                        &field,
                        "The secret must be text of at most 4000 characters.",
                    );
                    continue;
                }
            };
            secrets.push((name, change));
        }
        let expires_at = self.credential_expires_at.map(|v| {
            v.filter(|s| !s.trim().is_empty()).and_then(|s| {
                let at = DateTime::parse_from_rfc3339(s.trim())
                    .ok()
                    .map(|d| d.to_utc());
                r.check(
                    "credential_expires_at",
                    at.is_some(),
                    "The credential expires at must be a valid date.",
                );
                at
            })
        });
        Some(Input {
            provider,
            label,
            config,
            secrets,
            expires_at,
        })
    }
}

/// Check a full config for `provider`; returns it normalized. Unknown keys are refused.
pub fn check_config(provider: &str, config: &Map<String, Value>, r: &mut Rules) -> Value {
    let allowed: &[&str] = if provider == "odoo" {
        &["base_url", "database"]
    } else {
        &[
            "alg",
            "certificate_id",
            "client_id",
            "environment",
            "rest_domain",
        ]
    };
    for key in config.keys().filter(|k| !allowed.contains(&k.as_str())) {
        r.add(
            &format!("config.{key}"),
            format!("The config.{key} field is not allowed."),
        );
    }
    let text = |r: &mut Rules, key: &str| -> Option<String> {
        let field = format!("config.{key}");
        match config.get(key) {
            Some(Value::String(s)) if !s.trim().is_empty() => {
                r.max_chars(&field, s.trim(), TEXT_MAX);
                Some(s.trim().to_owned())
            }
            None | Some(Value::Null) => {
                r.missing(&field);
                None
            }
            Some(_) => {
                r.add(&field, format!("The config.{key} must be text."));
                None
            }
        }
    };
    let mut out = Map::new();
    if provider == "odoo" {
        let base = text(r, "base_url").and_then(|u| {
            let url = base_url(&u);
            r.check("config.base_url", url.is_some(), BASE_URL);
            url
        });
        out.insert("base_url".into(), base.map_or(Value::Null, Value::String));
        let database = match config.get("database") {
            None | Some(Value::Null) => Value::Null,
            Some(Value::String(s)) if !s.trim().is_empty() => {
                r.max_chars("config.database", s.trim(), TEXT_MAX);
                Value::String(s.trim().to_owned())
            }
            Some(_) => {
                r.add(
                    "config.database",
                    "The config.database must be text or null.",
                );
                Value::Null
            }
        };
        out.insert("database".into(), database);
    } else {
        let domain = text(r, "rest_domain").inspect(|d| {
            r.check("config.rest_domain", public_host(d), REST_DOMAIN);
        });
        let env = text(r, "environment");
        if let Some(e) = &env {
            r.check(
                "config.environment",
                ["production", "sandbox"].contains(&e.as_str()),
                "The selected environment is invalid.",
            );
        }
        let alg = text(r, "alg");
        if let Some(a) = &alg {
            r.check("config.alg", a == "ES256", "The selected alg is invalid.");
        }
        for (key, v) in [
            ("rest_domain", domain.map(|d| d.to_ascii_lowercase())),
            ("environment", env),
            ("client_id", text(r, "client_id")),
            ("certificate_id", text(r, "certificate_id")),
            ("alg", alg),
        ] {
            out.insert(key.into(), v.map_or(Value::Null, Value::String));
        }
    }
    Value::Object(out)
}

/// `https://<public host>[:port]` with nothing after it (a trailing `/` is dropped).
fn base_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw).ok()?;
    let bare = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == "/";
    let host = url.host_str()?;
    (bare && public_host(host)).then(|| raw.trim_end_matches('/').to_owned())
}

/// A host the outbound guard would let through by name: a public IP literal, or a dotted
/// name that isn't local. DNS is still guarded at send time (`Outbound`).
fn public_host(host: &str) -> bool {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_public(ip);
    }
    let valid = !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host
            .split('.')
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let local = [
        "localhost",
        ".localhost",
        ".local",
        ".internal",
        ".lan",
        ".home.arpa",
    ]
    .iter()
    .any(|s| host == s.trim_start_matches('.') || host.ends_with(s));
    valid && !local
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_host_rules() {
        for ok in [
            "acme.odoo.com",
            "1234567.suitetalk.api.netsuite.com",
            "8.8.8.8",
        ] {
            assert!(public_host(ok), "{ok}");
        }
        for bad in [
            "localhost",
            "a.localhost",
            "intranet",
            "10.0.0.1",
            "[::1]",
            "x.local",
            "a..b",
            "a/b.com",
            "x.com:1",
        ] {
            assert!(!public_host(bad), "{bad}");
        }
        assert_eq!(
            base_url("https://acme.odoo.com/").as_deref(),
            Some("https://acme.odoo.com")
        );
        assert!(base_url("https://acme.odoo.com/web").is_none());
    }
}
