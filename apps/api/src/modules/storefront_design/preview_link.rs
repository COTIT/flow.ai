//! Expiring preview links for the current unpublished draft; token shown once, stored as sha256.

use super::{
    dto::*,
    repo::{self, NewLink},
    service::{checked_revision, collections},
};
use crate::{
    AppError, AppState,
    http::workspace::WorkspaceAccess,
    idempotency::{self, Actor, Answer, Key},
    modules::auth::token,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const NOT_DRAFT: &str = "Preview links are only for the current unpublished draft.";
const TOO_MANY: &str = "This draft already has 5 active preview links. Revoke one first.";

fn created(link: repo::LinkRow, token: Option<String>) -> Result<Answer, AppError> {
    let issued = StorefrontPreviewLinkIssued {
        link: link.into(),
        token,
    };
    Ok(Answer {
        status: 201,
        body: serde_json::to_vec(&json!({"data":issued}))
            .map_err(|_| AppError::Internal("preview link serialization failed".into()))?,
    })
}

/// Issue a link; the stored replay answer carries `token: null`, so plaintext never lands in a row.
pub async fn create(
    db: &PgPool,
    access: &WorkspaceAccess,
    input: StorefrontPreviewLinkCreate,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    if key.is_none() {
        return Err(AppError::field(
            "Idempotency-Key",
            "An Idempotency-Key header is required.",
        ));
    }
    let ws = access.workspace_id;
    let mut tx = db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(answer);
    }
    // Page row lock serializes issuers, so the active cap holds under races.
    let page = repo::lock_page(&mut tx, ws).await?;
    repo::revision(&mut *tx, ws, page.id, input.revision_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if page.draft_revision_id != Some(input.revision_id)
        || page.published_revision_id == Some(input.revision_id)
    {
        return Err(AppError::Conflict(NOT_DRAFT));
    }
    if repo::active_links(&mut tx, ws, input.revision_id).await? >= PREVIEW_MAX_ACTIVE {
        return Err(AppError::Conflict(TOO_MANY));
    }
    let (plain, hash) = token::generate()?;
    let row = repo::insert_link(
        &mut tx,
        ws,
        NewLink {
            page: page.id,
            revision: input.revision_id,
            hash: &hash,
            hours: input.hours.unwrap_or(PREVIEW_HOURS),
            actor: access.user_id,
            login: access.token_id,
        },
    )
    .await?;
    tracing::info!(workspace_id = %ws, link_id = %row.link_id, "storefront preview link issued");
    let stored = created(row, None)?;
    idempotency::finish(&mut tx, ws, key.as_ref(), &stored).await?;
    tx.commit().await?;
    // Same row as stored, plus the one-time token.
    let mut body: serde_json::Value = serde_json::from_slice(&stored.body)
        .map_err(|_| AppError::Internal("preview link serialization failed".into()))?;
    body["data"]["token"] = json!(plain.expose());
    Ok(Answer::created(&body))
}

/// Newest links of one revision; another workspace's revision lists nothing.
pub async fn list(
    db: &PgPool,
    ws: Uuid,
    revision: Uuid,
) -> Result<Vec<StorefrontPreviewLink>, AppError> {
    Ok(repo::links(db, ws, revision)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}

/// Revoke once; repeat answers the same row. Effective on the next resolve.
pub async fn revoke(
    db: &PgPool,
    access: &WorkspaceAccess,
    id: Uuid,
) -> Result<StorefrontPreviewLink, AppError> {
    let ws = access.workspace_id;
    let mut tx = db.begin().await?;
    let row = repo::revoke(&mut tx, ws, id, access.user_id, access.token_id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    tracing::info!(workspace_id = %ws, link_id = %id, "storefront preview link revoked");
    Ok(row.into())
}

/// Token to preview page for the app key's workspace. Every failure is the same 404.
pub async fn resolve(
    state: &AppState,
    ws: Uuid,
    plain: &str,
) -> Result<StorefrontPreviewDesign, AppError> {
    if !token::looks_valid(plain) {
        return Err(AppError::NotFound);
    }
    let mut tx = state.db.begin().await?;
    super::repo::snapshot(&mut tx).await?;
    let link = repo::live_link(&mut *tx, ws, &token::hash(plain))
        .await?
        .ok_or(AppError::NotFound)?;
    let revision = match checked_revision(&mut tx, ws, link.page_id, link.revision_id).await {
        Ok(revision) => revision,
        // Stored doc no longer validates: same answer as a dead link.
        Err(AppError::Validation(_)) => return Err(AppError::NotFound),
        Err(e) => return Err(e),
    };
    tx.commit().await?;
    let collections = collections(state, ws, &revision.document).await?;
    Ok(StorefrontPreviewDesign {
        revision_id: link.revision_id,
        expires_at: link.expires_at,
        document: revision.document,
        collections,
    })
}
