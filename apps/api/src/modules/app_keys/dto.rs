//! App key request/response bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::app_keys::{
        repo::KeyRow,
        secret::{KINDS, SCOPES},
    },
};

const NAME_MAX: usize = 100;
const SCOPES_MAX: usize = 20;

/// A key as listed: never the secret, only its first 8 characters.
#[derive(Serialize, ToSchema)]
#[schema(description = "An app key. The secret itself is shown only once, when created.")]
pub struct AppKeyResource {
    id: Uuid,
    name: String,
    /// `shop`, `till`, `field` or `partner`.
    kind: String,
    scopes: Vec<String>,
    /// First 8 characters of the key, to tell keys apart.
    #[schema(example = "gsk_3f9a")]
    prefix: String,
    created_at: DateTime<Utc>,
    #[schema(required = true)]
    last_used_at: Option<DateTime<Utc>>,
    /// Set once revoked; a revoked key is refused.
    #[schema(required = true)]
    revoked_at: Option<DateTime<Utc>>,
}

impl From<KeyRow> for AppKeyResource {
    fn from(k: KeyRow) -> Self {
        Self {
            id: k.id,
            name: k.name,
            kind: k.kind,
            scopes: k.scopes,
            prefix: k.prefix,
            created_at: k.created_at,
            last_used_at: k.last_used_at,
            revoked_at: k.revoked_at,
        }
    }
}

/// Create response: the key's fields plus the secret, shown this once.
#[derive(Serialize, ToSchema)]
#[schema(description = "A new app key. Store `key` now: it is never shown again.")]
pub struct AppKeyCreated {
    id: Uuid,
    name: String,
    kind: String,
    scopes: Vec<String>,
    prefix: String,
    created_at: DateTime<Utc>,
    /// Send as `Authorization: Bearer <key>`.
    #[schema(example = "gsk_3f9a…")]
    key: String,
}

impl AppKeyCreated {
    /// From the stored row and the plaintext secret.
    pub fn new(k: KeyRow, key: String) -> Self {
        Self {
            id: k.id,
            name: k.name,
            kind: k.kind,
            scopes: k.scopes,
            prefix: k.prefix,
            created_at: k.created_at,
            key,
        }
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// POST /app-keys.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New app key for one app of this workspace.")]
pub struct CreateAppKeyRequest {
    #[schema(value_type = String, max_length = 100, example = "Web shop")]
    name: Option<String>,
    #[schema(value_type = String, example = "shop")]
    kind: Option<String>,
    /// At least one of: `catalog:read`.
    #[schema(value_type = Vec<String>, example = json!(["catalog:read"]))]
    scopes: Option<Value>,
}

/// Validated create input.
pub struct NewAppKey {
    pub name: String,
    pub kind: String,
    pub scopes: Vec<String>,
}

/// 1..=20 distinct known scopes.
fn scopes(r: &mut Rules, v: Option<Value>) -> Vec<String> {
    let Some(v) = r.required_value("scopes", v) else {
        return Vec::new();
    };
    let list: Option<Vec<String>> = v
        .as_array()
        .and_then(|a| a.iter().map(|s| s.as_str().map(str::to_owned)).collect());
    let Some(mut list) = list.filter(|l| (1..=SCOPES_MAX).contains(&l.len())) else {
        r.add("scopes", "The scopes field must list 1 to 20 scopes.");
        return Vec::new();
    };
    if let Some(bad) = list.iter().find(|s| !SCOPES.contains(&s.as_str())) {
        r.add("scopes", format!("Unknown scope {bad}."));
    }
    list.sort();
    list.dedup();
    list
}

impl Validate for CreateAppKeyRequest {
    type Valid = NewAppKey;

    fn validate(self, r: &mut Rules) -> Option<NewAppKey> {
        let name = r.text("name", self.name, NAME_MAX);
        let kind = r
            .required("kind", self.kind.clone())
            .and_then(|_| r.one_of("kind", self.kind, KINDS, None));
        Some(NewAppKey {
            name,
            kind: kind.unwrap_or_default(),
            scopes: scopes(r, self.scopes),
        })
    }
}
