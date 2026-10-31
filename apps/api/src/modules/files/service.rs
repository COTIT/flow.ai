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
            sniff::{TYPES, sniff},
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
    // Scanning and hashing up to 10 MB is CPU work: off the async workers.
    let name = f.document_name.to_owned();
    let (bytes, kind, sha256) = tokio::task::spawn_blocking(move || {
        let kind = sniff(&f.bytes, &name);
        let sha256 = Sha256::digest(&f.bytes);
        (f.bytes, kind, sha256)
    })
    .await
    .map_err(|e| AppError::Internal(format!("file scan: {e}")))?;
    let (extension, content_type) = kind.ok_or_else(|| AppError::field("upload", TYPES))?;
    let document_name = display_name(f.document_name);
    let id = Uuid::now_v7();
    let key = format!("genesy/{workspace_id}/{id}");
    let size = i64::try_from(bytes.len()).map_err(|_| AppError::Internal("file size".into()))?;
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
    let saved_as = download_name(&document_name, Some(extension));
    bucket
        .put(&key, bytes, content_type, &saved_as, inline(content_type))
        .await?;
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
                    &download_name(&r.document_name, r.extension.as_deref()),
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

/// A client file name made safe to show: no folders, no control characters, ≤ 255 chars.
pub(crate) fn display_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = base.chars().filter(|c| !hidden(*c)).take(255).collect();
    match clean.trim() {
        "" | "." | ".." => "upload".to_owned(),
        t => t.to_owned(),
    }
}

/// Characters that hide or reorder what a name shows: controls, bidi overrides and isolates,
/// zero-width marks, line and paragraph separators.
fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

/// The name a download saves as: the display name ending in the stored (detected) extension,
/// so a PDF named `x.html` saves as `x.pdf`.
fn download_name(name: &str, extension: Option<&str>) -> String {
    let Some(ext) = extension else {
        return name.to_owned();
    };
    if name.to_ascii_lowercase().ends_with(&format!(".{ext}")) {
        return name.to_owned();
    }
    // Drop a short trailing extension the client chose (`x.html`), keep dots inside a name.
    let stem = match name.rsplit_once('.') {
        Some((stem, tail)) if !stem.is_empty() && tail.len() <= 10 && !tail.contains(' ') => stem,
        _ => name,
    };
    format!("{stem}.{ext}")
}

/// Opened in the browser (PDFs and raster images), else downloaded: an SVG or HTML file opened
/// inline could run script on the bucket's origin.
pub(crate) fn inline(content_type: &str) -> bool {
    matches!(
        content_type,
        "application/pdf" | "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned_for_display() {
        assert_eq!(display_name("../../etc/Report 1.pdf"), "Report 1.pdf");
        assert_eq!(display_name("C:\\fakepath\\a.png"), "a.png");
        assert_eq!(display_name("a\u{7}b\nc.pdf"), "abc.pdf");
        assert_eq!(display_name("dir/"), "upload");
        assert_eq!(display_name(&"é".repeat(300)).chars().count(), 255);
        assert_eq!(display_name("in\u{202E}fdp.exe"), "infdp.exe");
        assert_eq!(display_name("a\u{200B}b\u{FEFF}\u{2066}c.pdf"), "abc.pdf");
    }

    /// Downloads carry the detected extension, whatever the client called the file.
    #[test]
    fn download_names_end_in_the_stored_extension() {
        assert_eq!(download_name("Scan.PDF", Some("pdf")), "Scan.PDF");
        assert_eq!(download_name("evil.html", Some("pdf")), "evil.pdf");
        assert_eq!(download_name("report", Some("pdf")), "report.pdf");
        assert_eq!(
            download_name("v1.2 notes.html", Some("png")),
            "v1.2 notes.png"
        );
        assert_eq!(download_name("old", None), "old");
    }
}
