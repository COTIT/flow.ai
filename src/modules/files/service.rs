//! Store, read and delete files. A write commits a pending row first, then uploads, then marks
//! it stored; a delete marks it deleting, removes the object, then marks it deleted. Nothing
//! talks to the bucket inside a transaction; the sweep finishes what a crash left half done.

use std::{collections::HashMap, time::Duration};

use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        files::{
            dto::{FileRelationships, FileResource},
            repo::{self, FileRow, NewRow},
        },
        users::{self, dto::UserEmailResource},
    },
    storage::Store,
};

/// 503 text without a bucket.
pub const NO_STORAGE: &str = "File storage isn't configured.";
/// How long a `file_path` URL works (opened right after the response).
const LINK_TTL: Duration = Duration::from_secs(10 * 60);

/// A file to store.
pub struct NewFile<'a> {
    pub document_name: &'a str,
    pub description: Option<&'a str>,
    pub meta: Option<&'a Value>,
    pub bytes: Vec<u8>,
}

/// The bucket, else 503.
pub fn store(state: &AppState) -> Result<&Store, AppError> {
    state
        .files
        .as_ref()
        .ok_or(AppError::Unavailable(NO_STORAGE))
}

/// Store `f` for `workspace_id`; its id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    f: NewFile<'_>,
) -> Result<Uuid, AppError> {
    let bucket = store(state)?;
    let id = Uuid::now_v7();
    let extension = extension(f.document_name);
    let content_type = content_type(extension.as_deref());
    let key = match &extension {
        Some(ext) => format!("genesy/{workspace_id}/{id}.{ext}"),
        None => format!("genesy/{workspace_id}/{id}"),
    };
    let sha256 = Sha256::digest(&f.bytes);
    let size = i64::try_from(f.bytes.len()).map_err(|_| AppError::Internal("file size".into()))?;
    repo::insert_pending(
        &state.db,
        &NewRow {
            id,
            workspace_id,
            key: &key,
            document_name: f.document_name,
            description: f.description,
            extension: extension.as_deref(),
            content_type,
            size,
            sha256: &sha256,
            meta: f.meta,
            uploaded_by: user_id,
        },
    )
    .await?;
    // A failed upload leaves the row pending: the sweep removes it.
    bucket.put(&key, f.bytes, content_type).await?;
    repo::set_status(&state.db, id, "pending", "stored").await?;
    Ok(id)
}

/// Delete the stored file `id`, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    let bucket = store(state)?;
    let id = parse(id)?;
    let key = repo::start_delete(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    // A failed removal leaves the row deleting: the sweep retries it.
    bucket.delete(&key).await?;
    repo::set_status(&state.db, id, "deleting", "deleted").await?;
    Ok(())
}

/// One stored file, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<FileResource, AppError> {
    let bucket = store(state)?;
    let row = repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut out = render(state, bucket, workspace_id, vec![row]).await?;
    out.pop().ok_or(AppError::NotFound)
}

/// One page of stored files plus the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<FileResource>, i64), AppError> {
    let bucket = store(state)?;
    let (rows, total) = repo::list(&state.db, workspace_id, limit, offset).await?;
    Ok((render(state, bucket, workspace_id, rows).await?, total))
}

/// Stored files among `ids` as FileResource values, by id (resources that embed files).
pub async fn values(
    state: &AppState,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Value>, AppError> {
    let Some(bucket) = state.files.as_ref() else {
        return Ok(HashMap::new());
    };
    let rows = repo::find_many(&state.db, workspace_id, ids).await?;
    render(state, bucket, workspace_id, rows)
        .await?
        .into_iter()
        .map(|f| {
            let v = serde_json::to_value(&f).map_err(|e| AppError::Internal(e.to_string()))?;
            Ok((f.id, v))
        })
        .collect()
}

/// Finish rows a crash left pending or deleting: remove their objects, then drop pending rows
/// and mark deleting ones deleted. Returns how many it finished.
pub async fn sweep(db: &sqlx::PgPool, bucket: &Store) -> Result<u64, AppError> {
    let mut done = 0;
    for row in repo::claim_unfinished(db, 100).await? {
        bucket.delete(&row.key).await?;
        match row.status.as_str() {
            "pending" => repo::remove_pending(db, row.id).await?,
            _ => {
                repo::set_status(db, row.id, "deleting", "deleted").await?;
            }
        }
        done += 1;
    }
    Ok(done)
}

async fn render(
    state: &AppState,
    bucket: &Store,
    workspace_id: Uuid,
    rows: Vec<FileRow>,
) -> Result<Vec<FileResource>, AppError> {
    let user_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.uploaded_by).collect();
    let users: HashMap<Uuid, Value> =
        users::repo::members_among(&state.db, workspace_id, &user_ids)
            .await?
            .iter()
            .map(|u| {
                let v = serde_json::to_value(UserEmailResource::from(u))
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                Ok((u.id, v))
            })
            .collect::<Result<_, AppError>>()?;
    rows.into_iter()
        .map(|r| {
            let inline = inline(&r.content_type);
            Ok(FileResource {
                file_path: bucket.download_url(
                    &r.key,
                    &r.document_name,
                    &r.content_type,
                    inline,
                    LINK_TTL,
                )?,
                id: r.id,
                size: r.size,
                extension: r.extension,
                document_name: r.document_name,
                created_at: r.created_at,
                last_updated_at: r.updated_at,
                association_data: [],
                meta: r.meta,
                relationships: FileRelationships {
                    uploaded_by: r.uploaded_by.and_then(|u| users.get(&u).cloned()),
                },
            })
        })
        .collect()
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Lowercase extension of `name` (≤ 20 ASCII alphanumerics), if any.
fn extension(name: &str) -> Option<String> {
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    (!ext.is_empty() && ext.len() <= 20 && ext.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then_some(ext)
}

/// Content type of an extension; unknown ones are generic bytes.
fn content_type(ext: Option<&str>) -> &'static str {
    match ext.unwrap_or_default() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "csv" => "text/csv",
        "txt" => "text/plain",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "doc" => "application/msword",
        _ => "application/octet-stream",
    }
}

/// Opened in the browser (PDFs and raster images), else downloaded: an SVG or HTML file opened
/// inline could run script on the bucket's origin.
fn inline(content_type: &str) -> bool {
    matches!(
        content_type,
        "application/pdf" | "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_types() {
        assert_eq!(extension("Invoice 1.PDF").as_deref(), Some("pdf"));
        assert_eq!(extension("noext"), None);
        assert_eq!(extension("a.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension("a.p d f"), None);
        assert_eq!(content_type(Some("jpeg")), "image/jpeg");
        assert_eq!(content_type(None), "application/octet-stream");
        assert!(inline("application/pdf") && !inline("image/svg+xml") && !inline("text/csv"));
    }
}
