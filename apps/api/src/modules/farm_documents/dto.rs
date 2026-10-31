//! Plot document bodies and the multipart form.

use axum::extract::Multipart;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{error::AppError, modules::farm_documents::repo::DocRow};

/// Largest file: 10 MB.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;

/// Who filed a document.
#[derive(Serialize, ToSchema)]
pub struct UploaderRef {
    id: Uuid,
    name: String,
}

/// A filed title document.
#[derive(Serialize, ToSchema)]
#[schema(description = "A plot's title document. Never changed or deleted.")]
pub struct PlotDocumentResource {
    id: Uuid,
    village_id: Uuid,
    plot_id: Uuid,
    file_name: String,
    /// `image/jpeg`, `image/png`, `image/webp`, `image/heic` or `application/pdf`, read from the bytes.
    content_type: String,
    size_bytes: i64,
    uploaded_by: UploaderRef,
    uploaded_at: DateTime<Utc>,
}

impl From<DocRow> for PlotDocumentResource {
    fn from(d: DocRow) -> Self {
        Self {
            id: d.id,
            village_id: d.village_id,
            plot_id: d.plot_id,
            file_name: d.file_name,
            content_type: d.content_type,
            size_bytes: d.size_bytes,
            uploaded_by: UploaderRef {
                id: d.uploaded_by,
                name: d.uploaded_by_name,
            },
            uploaded_at: d.uploaded_at,
        }
    }
}

/// One document plus a short-lived link to the file.
#[derive(Serialize, ToSchema)]
pub struct PlotDocumentDownload {
    #[serde(flatten)]
    document: PlotDocumentResource,
    /// Works until `download_expires_at` (at most 15 minutes).
    download_url: String,
    download_expires_at: DateTime<Utc>,
}

impl PlotDocumentDownload {
    pub fn new(d: DocRow, download_url: String, download_expires_at: DateTime<Utc>) -> Self {
        Self {
            document: d.into(),
            download_url,
            download_expires_at,
        }
    }
}

/// Multipart form: `file` (required) and `id` (optional uuid; send the same one on a retry).
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct PlotDocumentForm {
    /// Client-made id, so a retried upload lands on the same document.
    id: Option<Uuid>,
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

/// Parsed form.
pub struct Upload {
    pub id: Option<Uuid>,
    pub file_name: String,
    pub bytes: Vec<u8>,
}

fn bad(message: &str) -> AppError {
    AppError::field("file", message)
}

/// Read the form: `id` and `file` only, the file capped at 10 MB while streaming.
pub async fn from_multipart(mut form: Multipart) -> Result<Upload, AppError> {
    let (mut id, mut file) = (None, None);
    while let Some(mut field) = form.next_field().await.map_err(|e| bad(&e.body_text()))? {
        match field.name().unwrap_or_default() {
            "id" if id.is_none() => {
                let text = field.text().await.map_err(|e| bad(&e.body_text()))?;
                id = Some(
                    Uuid::parse_str(text.trim())
                        .map_err(|_| AppError::field("id", "The id must be a valid UUID."))?,
                );
            }
            "file" if file.is_none() => {
                let name = field.file_name().unwrap_or_default().to_owned();
                let mut bytes = Vec::new();
                while let Some(chunk) = field.chunk().await.map_err(|e| bad(&e.body_text()))? {
                    if bytes.len() + chunk.len() > MAX_BYTES {
                        return Err(bad("The file must not be greater than 10 MB."));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                file = Some((name, bytes));
            }
            // Not echoed: the name is client-made.
            _ => {
                return Err(AppError::Unprocessable(
                    "The upload has an unknown field.".into(),
                ));
            }
        }
    }
    let (file_name, bytes) = file
        .filter(|(_, b)| !b.is_empty())
        .ok_or_else(|| AppError::field("file", "The file field is required."))?;
    Ok(Upload {
        id,
        file_name,
        bytes,
    })
}
