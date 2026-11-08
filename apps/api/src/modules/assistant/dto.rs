//! Bounded assistant control inputs; business drafts never contain credentials.

use crate::http::json::{Rules, Validate};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

/// New session language; English, Dutch and Swahili share one contract.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionInput {
    pub language: String,
}
impl Validate for SessionInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "language",
            matches!(self.language.as_str(), "en" | "nl" | "sw"),
            "Choose en, nl or sw.",
        );
        Some(self)
    }
}

/// Fixed provider conversation, accepted only from signed bridge requests.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AttachInput {
    pub conversation_id: String,
}
impl Validate for AttachInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        provider_id(r, "conversation_id", &self.conversation_id);
        Some(self)
    }
}

/// Canonical business draft, retained only until dispatch, cancellation or expiry.
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposalInput {
    pub session_id: Uuid,
    pub operation_id: String,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub body: Option<Value>,
    #[serde(default)]
    pub body_sha256: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
}
impl Validate for ProposalInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "method",
            matches!(
                self.method.as_str(),
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
            ),
            "Choose a mutation HTTP method.",
        );
        if self.body_sha256.is_some() || self.content_type.is_some() {
            r.check(
                "body_sha256",
                self.body_sha256.as_deref().is_some_and(valid_sha256),
                "Use a lowercase SHA-256 digest.",
            );
            r.check(
                "content_type",
                self.content_type.as_deref().is_some_and(|s| {
                    s.starts_with("multipart/form-data; boundary=")
                        && s.len() <= 200
                        && s.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
                }),
                "Use the exact multipart content type and boundary.",
            );
        }
        r.max_chars("operation_id", &self.operation_id, 100);
        r.max_chars("path", &self.path, 2048);
        if let Some(q) = &self.query {
            r.max_chars("query", q, 4096);
        }
        if let Some(body) = &self.body {
            r.check(
                "body",
                body.to_string().len() <= 32768,
                "The business draft must not exceed 32768 bytes.",
            );
            r.check(
                "body",
                !contains_secret(body),
                "Credentials require the secure Office form.",
            );
        }
        Some(self)
    }
}

/// Explicit session binding for approval and cancellation.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActionInput {
    pub session_id: Uuid,
    /// Hash of session and final provider user-event id; no utterance stored.
    #[serde(default)]
    pub approval_event_sha256: Option<String>,
}
impl Validate for ActionInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        if let Some(hash) = &self.approval_event_sha256 {
            r.check(
                "approval_event_sha256",
                valid_sha256(hash),
                "Use a lowercase SHA-256 digest.",
            );
        }
        Some(self)
    }
}

/// Upload reservation holds quota without storing file bytes or names.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadInput {
    pub media_type: String,
    pub size_bytes: i64,
}
impl Validate for UploadInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "media_type",
            matches!(
                self.media_type.as_str(),
                "application/pdf" | "image/jpeg" | "image/png" | "image/webp"
            ),
            "This file type is not supported.",
        );
        r.check(
            "size_bytes",
            (1..=10485760).contains(&self.size_bytes),
            "Files must be between 1 byte and 10 MiB.",
        );
        Some(self)
    }
}

/// Provider file reference after upload; ownership comes from signed bridge.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteInput {
    #[serde(alias = "provider_file_id")]
    pub file_id: String,
    #[serde(default)]
    pub size_bytes: Option<i64>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
}
impl Validate for CompleteInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        provider_id(r, "file_id", &self.file_id);
        if self.size_bytes.is_some() || self.media_type.is_some() || self.sha256.is_some() {
            r.check(
                "size_bytes",
                self.size_bytes.is_some_and(|n| (1..=10485760).contains(&n)),
                "Files must be between 1 byte and 10 MiB.",
            );
            r.check(
                "media_type",
                self.media_type.as_deref().is_some_and(|s| {
                    matches!(
                        s,
                        "application/pdf" | "image/jpeg" | "image/png" | "image/webp"
                    )
                }),
                "This file type is not supported.",
            );
            r.check(
                "sha256",
                self.sha256.as_deref().is_some_and(valid_sha256),
                "Use a lowercase SHA-256 digest.",
            );
        }
        Some(self)
    }
}

/// Cursor for newest-first session history.
#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in=Query)]
pub struct HistoryQuery {
    pub before: Option<i64>,
}

/// Session read options; `with=identity` adds the caller's access facts.
#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in=Query)]
pub struct SessionQuery {
    /// Only `identity`.
    pub with: Option<String>,
}
impl SessionQuery {
    /// True for `with=identity`; any other value is a 422.
    pub fn identity(&self) -> Result<bool, crate::AppError> {
        match self.with.as_deref() {
            None => Ok(false),
            Some("identity") => Ok(true),
            Some(_) => Err(crate::AppError::field(
                "with",
                "The selected with is invalid.",
            )),
        }
    }
}

/// Documented list envelope for bounded control records.
#[derive(Serialize, ToSchema)]
pub struct ControlListDoc {
    #[schema(value_type=Vec<Object>)]
    pub data: Vec<Value>,
}

fn provider_id(r: &mut Rules, field: &str, value: &str) {
    r.check(
        field,
        (1..=200).contains(&value.len())
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "Invalid provider identifier.",
    );
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn contains_secret(v: &Value) -> bool {
    match v {
        Value::Object(m) => m.iter().any(|(k, v)| {
            let key = k.to_ascii_lowercase().replace('-', "_");
            if matches!(
                key.as_str(),
                "pin" | "upload" | "file_bytes" | "file_content" | "data_url"
            ) {
                return true;
            }
            [
                "password",
                "secret",
                "token",
                "api_key",
                "authorization",
                "credential",
                "base64",
            ]
            .iter()
            .any(|s| key.contains(s))
                || contains_secret(v)
        }),
        Value::Array(a) => a.iter().any(contains_secret),
        _ => false,
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_pin_fields_are_credentials_without_rejecting_business_names() {
        assert!(contains_secret(&json!({"pin":"4821"})));
        assert!(contains_secret(&json!({"details":[{"PIN":"4821"}]})));
        assert!(!contains_secret(
            &json!({"pipeline_id":"p","shipping":{"name":"Dock"},"fingerprint":"bound"})
        ));
    }
}
