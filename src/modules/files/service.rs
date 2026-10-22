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
    let (extension, content_type) =
        sniff(&f.bytes).ok_or_else(|| AppError::field("upload", TYPES))?;
    let document_name = display_name(f.document_name);
    let id = Uuid::now_v7();
    let key = format!("genesy/{workspace_id}/{id}");
    let sha256 = Sha256::digest(&f.bytes);
    let size = i64::try_from(f.bytes.len()).map_err(|_| AppError::Internal("file size".into()))?;
    repo::insert_pending(
        &state.db,
        &NewRow {
            id,
            workspace_id,
            key: &key,
            document_name: &document_name,
            description: f.description,
            extension: Some(extension),
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
    if !repo::mark_stored(&state.db, workspace_id, id).await? {
        // The sweep claimed the row meanwhile; its delete may have run before this upload landed.
        if let Err(err) = bucket.delete(&key).await {
            tracing::warn!(file = %id, error = %err, "swept upload: delete failed");
        }
        return Err(AppError::Internal(format!(
            "file {id} was swept during its upload"
        )));
    }
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
    repo::mark_deleted(&state.db, id).await?;
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

/// Finish rows a crash left pending or deleting: remove their objects and mark them deleted,
/// in batches of 100 (at most 10 a run). A row whose object can't be removed is retried next
/// run; three failures in a row end the run (the bucket is likely down). Returns how many it
/// finished.
pub async fn sweep(db: &sqlx::PgPool, bucket: &Store) -> Result<u64, AppError> {
    const BATCH: i64 = 100;
    let (mut done, mut failures) = (0, 0);
    for _ in 0..10 {
        let rows = repo::claim_unfinished(db, BATCH).await?;
        let claimed = rows.len();
        for row in rows {
            match bucket.delete(&row.key).await {
                Ok(()) => {
                    repo::mark_deleted(db, row.id).await?;
                    done += 1;
                    failures = 0;
                }
                Err(err) => {
                    tracing::warn!(file = %row.id, error = %err, "file sweep: delete failed");
                    failures += 1;
                    if failures == 3 {
                        return Ok(done);
                    }
                }
            }
        }
        if claimed < BATCH as usize {
            break;
        }
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

/// 422 for bytes of any other type.
pub const TYPES: &str = "The upload must be a file of type: png, jpeg, gif, webp, pdf.";

/// Extension and content type from the file's first bytes; the client's name and type are
/// never trusted. None for any other type (SVG, HTML and executables included).
fn sniff(b: &[u8]) -> Option<(&'static str, &'static str)> {
    Some(match b {
        [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', ..] => ("png", "image/png"),
        [0xff, 0xd8, 0xff, ..] => ("jpg", "image/jpeg"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => ("gif", "image/gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => ("webp", "image/webp"),
        [b'%', b'P', b'D', b'F', b'-', ..] => ("pdf", "application/pdf"),
        _ => return None,
    })
}

/// A client file name made safe to show: no folders, no control characters, ≤ 255 chars.
fn display_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = base.chars().filter(|c| !c.is_control()).take(255).collect();
    match clean.trim() {
        "" | "." | ".." => "upload".to_owned(),
        t => t.to_owned(),
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
    fn types_come_from_the_bytes() {
        assert_eq!(sniff(b"%PDF-1.7\n"), Some(("pdf", "application/pdf")));
        assert_eq!(sniff(b"\xff\xd8\xff\xe0"), Some(("jpg", "image/jpeg")));
        assert_eq!(sniff(b"GIF89a.."), Some(("gif", "image/gif")));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(("webp", "image/webp")));
        assert_eq!(sniff(b"RIFF\0\0\0\0WAVEfmt "), None);
        assert_eq!(sniff(b"<svg/>"), None);
        assert_eq!(sniff(b"%PDF"), None);
        assert_eq!(sniff(b""), None);
        assert!(inline("application/pdf") && !inline("image/svg+xml") && !inline("text/csv"));
    }

    #[test]
    fn names_are_cleaned_for_display() {
        assert_eq!(display_name("../../etc/Report 1.pdf"), "Report 1.pdf");
        assert_eq!(display_name("C:\\fakepath\\a.png"), "a.png");
        assert_eq!(display_name("a\u{7}b\nc.pdf"), "abc.pdf");
        assert_eq!(display_name("dir/"), "upload");
        assert_eq!(display_name(&"é".repeat(300)).chars().count(), 255);
    }
}
