//! File upload body (Laravel StoreFileRequest: `upload` as a multipart file part, or base64
//! with `document_name`) and FileResource.

use std::collections::HashMap;

use axum::{
    extract::multipart::{Multipart, MultipartError},
    http::StatusCode,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::AppError,
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

/// Parts a multipart upload may carry: FilePond sends `upload`, `public` and, on entity file
/// tabs, the two `associated_with` keys.
const MAX_PARTS: usize = 8;
/// Longest text part (a 255-character description is at most 1020 bytes).
const MAX_TEXT: usize = 2048;

/// `POST /files` as multipart (vimdesk-web's FilePond): exactly one file part `upload` plus
/// the known text fields, anything else a 422. The file is read in chunks up to `MAX_BYTES`
/// (413 past it); its name is only shown and its declared type is ignored.
/// `associated_with` is checked, not stored: associations aren't ported.
pub async fn from_multipart(mut form: Multipart) -> Result<Upload, AppError> {
    let mut r = Rules::default();
    let mut file: Option<(Vec<u8>, String)> = None;
    let mut text: HashMap<String, String> = HashMap::new();
    let mut parts = 0;
    while let Some(mut field) = form.next_field().await.map_err(multipart_error)? {
        parts += 1;
        if parts > MAX_PARTS {
            return Err(AppError::Unprocessable(
                "The upload has too many fields.".into(),
            ));
        }
        let name = match field.name().unwrap_or_default() {
            "associated_with[parent_domain]" => "associated_with.parent_domain",
            "associated_with[parent_id]" => "associated_with.parent_id",
            "upload" => "upload",
            "public" => "public",
            "description" => "description",
            "document_name" => "document_name",
            // Not echoed: the name is client-sized.
            _ => {
                return Err(AppError::Unprocessable(
                    "The upload has an unknown field.".into(),
                ));
            }
        };
        let file_name = field.file_name().unwrap_or_default();
        if file_name.len() > MAX_TEXT {
            return Err(AppError::field(
                "upload",
                "The upload file name is too long.",
            ));
        }
        let file_name = file_name.to_owned();
        let cap = if name == "upload" {
            MAX_BYTES
        } else {
            MAX_TEXT
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
            if bytes.len() + chunk.len() > cap {
                return Err(AppError::PayloadTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        if name == "upload" {
            if file.is_some() {
                r.add("upload", "Only one file may be uploaded.");
            }
            file = Some((bytes, file_name));
        } else if text.contains_key(name) {
            r.add(name, format!("The {} field may be sent once.", human(name)));
        } else {
            match String::from_utf8(bytes) {
                Ok(s) => drop(text.insert(name.to_owned(), s)),
                Err(_) => r.add(name, format!("The {} must be a string.", human(name))),
            }
        }
    }
    let scalar = |k: &str| {
        text.get(k)
            .map_or(Scalar::Absent, |s| Scalar::Str(s.clone()))
    };
    let parent = text.get("associated_with.parent_domain").map(|d| d.trim());
    if let Some(id) = text.get("associated_with.parent_id") {
        if Uuid::parse_str(id.trim()).is_err() {
            let f = "associated_with.parent_id";
            r.add(f, format!("The {} must be a valid UUID.", human(f)));
        } else if parent.is_none_or(str::is_empty) {
            r.missing("associated_with.parent_domain");
        }
    }
    if let Some(d) = parent {
        r.max_chars("associated_with.parent_domain", d, 255);
    }
    let document_name = match text.get("document_name").map(|n| n.trim()) {
        Some(n) if !n.is_empty() => n.to_owned(),
        _ => file.as_ref().map(|(_, n)| n.clone()).unwrap_or_default(),
    };
    r.max_chars("document_name", &document_name, 255);
    if file.is_none() {
        r.missing("upload");
    }
    let common = common_fields(
        &mut r,
        &scalar("description"),
        &scalar("public"),
        &Capped::Absent,
    );
    r.into_result()?;
    let (description, meta) = common.unwrap_or_default();
    Ok(Upload {
        bytes: file.map(|(b, _)| b).unwrap_or_default(),
        document_name,
        description,
        meta,
    })
}

/// A broken or oversized form: 413 past the body limit, 408 past the body deadline, else 422.
fn multipart_error(e: MultipartError) -> AppError {
    match e.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
        _ if crate::http::timeout::is_body_timeout(&e) => AppError::BodyTimeout,
        _ => AppError::field("upload", e.body_text()),
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
