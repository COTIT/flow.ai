//! Plot document rules: staff of the plot's village only (farmers and other villages get 404);
//! type from the bytes; each upload writes its own object, so a retry or a race can never
//! replace a filed file; links live 10 minutes.

use std::time::Duration;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::farm::FarmAccess,
    modules::{
        farm_documents::{
            dto::{PlotDocumentDownload, Upload},
            repo::{self, DocRow, NewDoc},
        },
        farm_records::service::{one, path_id},
        files::{
            service::{display_name, inline, store},
            sniff::sniff,
        },
    },
};

const TYPES: &str = "The file must be a JPEG, PNG, WebP or HEIC photo, or a PDF.";
/// Download links stay well under the 15-minute ceiling.
const LINK_TTL: Duration = Duration::from_secs(10 * 60);

/// Folder of one plot's files in the bucket.
fn plot_folder(ws: Uuid, village: Uuid, plot: Uuid) -> String {
    format!("genesy/{ws}/farm/{village}/{plot}/")
}

/// HEIC photos (iPhone default) start with an `ftyp` box naming a HEIF brand.
fn is_heic(b: &[u8]) -> bool {
    b.len() >= 12
        && &b[4..8] == b"ftyp"
        && matches!(
            &b[8..12],
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1"
        )
}

/// Content type from the bytes; the client's claim and the file name are ignored.
fn content_type(bytes: &[u8]) -> Option<&'static str> {
    if is_heic(bytes) {
        return Some("image/heic");
    }
    // The file name only breaks ties for text formats, which are refused here anyway.
    sniff(bytes, "").map(|(_, ct)| ct).filter(|ct| {
        matches!(
            *ct,
            "image/jpeg" | "image/png" | "image/webp" | "application/pdf"
        )
    })
}

/// The village of a plot the caller is staff of, else 404 (farmers included).
async fn plot_village(state: &AppState, access: &FarmAccess, plot: Uuid) -> Result<Uuid, AppError> {
    let scope = access.staff_scope();
    repo::plot_village(
        &state.db,
        access.workspace_id,
        scope.all,
        &scope.villages,
        plot,
    )
    .await?
    .ok_or(AppError::NotFound)
}

/// One plot's documents.
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    plot: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DocRow>, i64), AppError> {
    let plot = path_id(plot)?;
    plot_village(state, access, plot).await?;
    Ok(repo::list(&state.db, access.workspace_id, plot, None, limit, offset).await?)
}

/// One document and a fresh download link.
pub async fn get(
    state: &AppState,
    access: &FarmAccess,
    plot: &str,
    id: &str,
) -> Result<PlotDocumentDownload, AppError> {
    let (plot, id) = (path_id(plot)?, path_id(id)?);
    let village = plot_village(state, access, plot).await?;
    let doc = one(repo::list(&state.db, access.workspace_id, plot, Some(id), 1, 0).await?)?;
    // Belt and braces with the table CHECK: never sign a key outside this plot's folder.
    if !doc
        .storage_key
        .starts_with(&plot_folder(access.workspace_id, village, plot))
    {
        return Err(AppError::NotFound);
    }
    let url = store(state)?.download_url(
        &doc.storage_key,
        &doc.file_name,
        &doc.content_type,
        inline(&doc.content_type),
        LINK_TTL,
    )?;
    let expires = chrono::Utc::now()
        + chrono::Duration::from_std(LINK_TTL).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(PlotDocumentDownload::new(doc, url, expires))
}

/// File a document. False = this id was filed before (a retry): the earlier document stands.
pub async fn create(
    state: &AppState,
    access: &FarmAccess,
    plot: &str,
    upload: Upload,
) -> Result<(DocRow, bool), AppError> {
    let plot = path_id(plot)?;
    let ws = access.workspace_id;
    let village = plot_village(state, access, plot).await?;
    let id = upload.id.unwrap_or_else(Uuid::now_v7);
    if let Some(done) = repo::list(&state.db, ws, plot, Some(id), 1, 0)
        .await?
        .0
        .pop()
    {
        return Ok((done, false));
    }
    let bucket = store(state)?;
    // Sniffing and hashing up to 10 MB is CPU work: off the async workers.
    let (bytes, kind, sha256) = tokio::task::spawn_blocking(move || {
        let kind = content_type(&upload.bytes);
        let sha256 = Sha256::digest(&upload.bytes);
        (upload.bytes, kind, sha256)
    })
    .await
    .map_err(|e| AppError::Internal(format!("file scan: {e}")))?;
    let content_type = kind.ok_or_else(|| AppError::field("file", TYPES))?;
    let file_name = display_name(&upload.file_name);
    // A fresh object per attempt: two racing retries never write the same key.
    let key = format!("{}{id}-{}", plot_folder(ws, village, plot), Uuid::now_v7());
    let size = i64::try_from(bytes.len()).map_err(|_| AppError::Internal("file size".into()))?;
    bucket
        .put(&key, bytes, content_type, &file_name, inline(content_type))
        .await?;
    let new = NewDoc {
        id,
        ws,
        village,
        plot,
        storage_key: &key,
        file_name: &file_name,
        content_type,
        size_bytes: size,
        sha256: &sha256,
        uploaded_by: access.user_id,
    };
    let inserted = async {
        let mut tx = access.begin(&state.db).await?;
        let inserted = repo::insert(&mut *tx, &new).await?;
        tx.commit().await?;
        Ok::<_, AppError>(inserted)
    }
    .await;
    if !matches!(inserted, Ok(true)) {
        // Lost a race or failed: this attempt's object is nobody's file.
        if let Err(err) = bucket.delete(&key).await {
            tracing::warn!(document = %id, error = %err, "orphan plot document object: delete failed");
        }
    }
    if inserted? {
        return Ok((
            one(repo::list(&state.db, ws, plot, Some(id), 1, 0).await?)?,
            true,
        ));
    }
    // The id is taken: by this plot (a racing retry) or by another plot (not ours to show).
    let earlier = repo::list(&state.db, ws, plot, Some(id), 1, 0)
        .await?
        .0
        .pop();
    earlier
        .map(|d| (d, false))
        .ok_or(AppError::Conflict("This document id is already used."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_come_from_the_bytes() {
        assert_eq!(content_type(b"%PDF-1.7\n"), Some("application/pdf"));
        assert_eq!(content_type(b"\x89PNG\r\n\x1a\n0000"), Some("image/png"));
        assert_eq!(
            content_type(b"\x00\x00\x00\x18ftypheic0000"),
            Some("image/heic")
        );
        assert_eq!(
            content_type(b"GIF89a......"),
            None,
            "gif is not a title document type"
        );
        assert_eq!(content_type(b"plain text"), None);
    }
}
