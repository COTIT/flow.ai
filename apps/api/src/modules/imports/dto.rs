//! Import wire payloads; envelopes (`Create`/`Action`) come from accounting.
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};

use crate::http::json::{Rules, Validate};

/// Most rows one file may hold.
pub const MAX_ROWS: i64 = 500;
/// Most rows one chunk may hold.
pub const CHUNK_ROWS: usize = 20;
/// Largest canonical encoding of one chunk's rows (inside the 64 KiB body).
pub const CHUNK_BYTES: usize = 60 * 1024;
/// Highest chunk index (500 rows / 20).
pub const MAX_INDEX: i64 = 24;
/// Longest cell or external key.
pub const MAX_CELL: usize = 255;
/// Product columns matched exactly on update; `name` is left out (not an identity).
pub const MATCH_KEYS: &[&str] = &["product_nr_formatted", "barcode", "ean13", "reference"];

/// POST /imports payload.
#[derive(Deserialize, ToSchema)]
#[schema(as = ImportCreatePayload)]
pub struct CreatePayload {
    /// `products` or `relations`.
    pub profile: Option<String>,
    pub filename: Option<String>,
    /// SHA-256 of the source file, lowercase hex.
    pub file_hash: Option<String>,
    #[schema(value_type = i64, minimum = 1, maximum = 500)]
    pub expected_rows: Option<Value>,
    /// products: `{operation: create|update, match_key}`; relations: `{}`.
    #[schema(value_type = Object)]
    pub mapping: Option<Value>,
}

/// A validated new batch.
pub struct NewBatch {
    pub profile: String,
    pub filename: String,
    pub file_hash: String,
    pub expected_rows: i32,
    pub mapping: Value,
}

impl Validate for CreatePayload {
    type Valid = NewBatch;

    fn validate(self, r: &mut Rules) -> Option<NewBatch> {
        let profile = r.required("profile", self.profile);
        if let Some(p) = &profile {
            r.check(
                "profile",
                ["products", "relations"].contains(&p.as_str()),
                "The profile must be products or relations.",
            );
        }
        let filename = r.text("filename", self.filename, MAX_CELL);
        let file_hash = r.required("file_hash", self.file_hash).unwrap_or_default();
        r.check(
            "file_hash",
            file_hash.len() == 64
                && file_hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "The file hash must be a lowercase SHA-256 hex string.",
        );
        let rows = match self.expected_rows {
            None | Some(Value::Null) => {
                r.missing("expected_rows");
                None
            }
            Some(v) => r.integer("expected_rows", &v, 1, MAX_ROWS),
        };
        let mapping = self.mapping.unwrap_or_else(|| json!({}));
        let mapping = match profile.as_deref() {
            Some("products") => {
                let op = mapping["operation"].as_str().unwrap_or_default();
                let key = mapping["match_key"].as_str().unwrap_or_default();
                r.check(
                    "mapping.operation",
                    ["create", "update"].contains(&op),
                    "The operation must be create or update.",
                );
                r.check(
                    "mapping.match_key",
                    MATCH_KEYS.contains(&key),
                    "The match key must be product_nr_formatted, barcode, ean13 or reference.",
                );
                Some(json!({"operation": op, "match_key": key}))
            }
            Some("relations") => {
                r.check(
                    "mapping",
                    mapping.as_object().is_some_and(|m| m.is_empty()),
                    "Relations imports take no mapping options.",
                );
                Some(json!({}))
            }
            _ => None,
        };
        Some(NewBatch {
            profile: profile?,
            filename,
            file_hash,
            expected_rows: i32::try_from(rows?).ok()?,
            mapping: mapping?,
        })
    }
}

/// PUT /imports/{id}/chunks/{index} payload; rows are checked against the batch's profile.
#[derive(Deserialize, ToSchema)]
#[schema(as = ImportChunkPayload)]
pub struct ChunkPayload {
    /// `[{row_number, external_key?, fields: {column: text}}]`, 1 to 20 rows.
    #[schema(value_type = Vec<Object>)]
    pub rows: Option<Vec<Value>>,
}

impl Validate for ChunkPayload {
    type Valid = Vec<Value>;

    fn validate(self, r: &mut Rules) -> Option<Vec<Value>> {
        let Some(rows) = self.rows else {
            r.missing("rows");
            return None;
        };
        r.check(
            "rows",
            (1..=CHUNK_ROWS).contains(&rows.len()),
            "A chunk holds 1 to 20 rows.",
        );
        Some(rows)
    }
}

/// POST /imports/{id}/validate payload.
#[derive(Deserialize, ToSchema)]
#[schema(as = ImportValidatePayload)]
pub struct ValidatePayload {
    #[schema(value_type = i64, minimum = 1, maximum = 25)]
    pub expected_chunks: Option<Value>,
}

impl Validate for ValidatePayload {
    type Valid = i32;

    fn validate(self, r: &mut Rules) -> Option<i32> {
        match self.expected_chunks {
            None | Some(Value::Null) => {
                r.missing("expected_chunks");
                None
            }
            Some(v) => r
                .integer("expected_chunks", &v, 1, MAX_INDEX + 1)
                .and_then(|n| i32::try_from(n).ok()),
        }
    }
}

/// POST /imports/{id}/apply payload.
#[derive(Deserialize, ToSchema)]
#[schema(as = ImportApplyPayload)]
pub struct ApplyPayload {
    /// From the validated batch; a changed batch refuses with 409.
    pub validation_fingerprint: Option<String>,
}

impl Validate for ApplyPayload {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        r.required("validation_fingerprint", self.validation_fingerprint)
    }
}

/// List and rows query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
    /// Rows only: staged, valid, invalid, applied or failed.
    pub status: Option<String>,
}
