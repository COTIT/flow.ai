//! File upload body (Laravel StoreFileRequest: `upload` as base64 with `document_name`) and
//! FileResource.

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::pricing::body::{Capped, Scalar},
};

/// Largest file: 10 MB (Laravel's limit was nginx's 32 MB; uploads are buffered in memory).
pub const MAX_BYTES: usize = 10 * 1024 * 1024;

/// `POST /files` as JSON.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct UploadRequest {
    upload: Scalar,
    document_name: Scalar,
    description: Scalar,
    public: Scalar,
    meta: Capped,
}

/// A validated upload.
pub struct Upload {
    pub bytes: Vec<u8>,
    pub document_name: String,
    pub description: Option<String>,
    pub meta: Option<Value>,
}

/// `{data:{file_id}}`.
#[derive(Serialize)]
pub struct FileCreated {
    pub file_id: Uuid,
}

impl Validate for UploadRequest {
    type Valid = Upload;

    fn validate(self, r: &mut Rules) -> Option<Upload> {
        let bytes = match &self.upload {
            Scalar::Absent | Scalar::Null => {
                r.missing("upload");
                None
            }
            Scalar::Str(s) => {
                let decoded = (!s.trim_start().starts_with('{'))
                    .then(|| {
                        base64::engine::general_purpose::STANDARD
                            .decode(s.trim())
                            .ok()
                    })
                    .flatten();
                match decoded {
                    None => {
                        r.add("upload", "The upload must be a file or a base64 string.");
                        None
                    }
                    Some(b) => checked_size(r, b),
                }
            }
            _ => {
                r.add("upload", "The upload must be a file or a base64 string.");
                None
            }
        };
        let document_name = match self.document_name.filled() {
            Some(Value::String(s)) => {
                let s = s.trim().to_owned();
                r.max_chars("document_name", &s, 255);
                s
            }
            Some(_) => {
                r.add("document_name", "The document name must be a string.");
                String::new()
            }
            None => {
                r.missing("document_name");
                String::new()
            }
        };
        common_fields(r, &self.description, &self.public, &self.meta).map(|(description, meta)| {
            Upload {
                bytes: bytes.unwrap_or_default(),
                document_name,
                description,
                meta,
            }
        })
    }
}

/// 422 past `MAX_BYTES`, Laravel's `max:` wording in kilobytes.
pub fn checked_size(r: &mut Rules, bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.len() > MAX_BYTES {
        let kb = MAX_BYTES / 1024;
        r.add(
            "upload",
            format!("The upload must not be greater than {kb} kilobytes."),
        );
        return None;
    }
    Some(bytes)
}

/// `description` (≤ 255), `public` (required boolean; files are always private) and `meta`.
pub fn common_fields(
    r: &mut Rules,
    description: &Scalar,
    public: &Scalar,
    meta: &Capped,
) -> Option<(Option<String>, Option<Value>)> {
    let description = match description.filled() {
        None => None,
        Some(Value::String(s)) => {
            let s = s.trim().to_owned();
            r.max_chars("description", &s, 255);
            Some(s)
        }
        Some(_) => {
            r.add("description", "The description must be a string.");
            None
        }
    };
    match public.filled() {
        None => r.missing("public"),
        Some(v) => {
            r.boolean("public", &v);
        }
    }
    let meta = match meta {
        Capped::Absent => None,
        Capped::TooLarge => {
            r.add("meta", format!("The {} is too large.", human("meta")));
            None
        }
        Capped::Value(v) => r.meta(Some(v.clone())).flatten(),
    };
    Some((description, meta))
}

/// Laravel FileResource.
#[derive(Serialize)]
pub struct FileResource {
    pub id: Uuid,
    /// A signed URL valid for a few minutes.
    pub file_path: String,
    pub size: i64,
    pub extension: Option<String>,
    pub document_name: String,
    pub created_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
    pub association_data: [(); 0],
    pub meta: Option<Value>,
    pub relationships: FileRelationships,
}

/// FileResource `relationships`.
#[derive(Serialize)]
pub struct FileRelationships {
    pub uploaded_by: Option<Value>,
}
