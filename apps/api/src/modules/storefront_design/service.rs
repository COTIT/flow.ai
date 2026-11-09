//! Design validation and atomic draft/publish transitions; catalog data stays external.

use super::{
    dto::*,
    repo::{self, NewRevision, PageRow},
};
use crate::{
    AppError, AppState,
    http::workspace::WorkspaceAccess,
    idempotency::{self, Actor, Answer, Key},
    modules::storefront,
};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

const STALE: &str = "The design changed. Reload before saving or publishing.";

async fn checked_revision(
    db: &mut PgConnection,
    ws: Uuid,
    page: Uuid,
    id: Uuid,
) -> Result<StorefrontDesignRevision, AppError> {
    let row = repo::revision(db, ws, page, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(StorefrontDesignRevision {
        revision_id: row.id,
        document: StorefrontDesignDocument::from_value(row.document.0)?,
    })
}
async fn state_on(db: &mut PgConnection, ws: Uuid) -> Result<StorefrontDesignState, AppError> {
    let Some(p) = repo::page(&mut *db, ws).await? else {
        return Ok(StorefrontDesignState::default());
    };
    let draft = match p.draft_revision_id {
        Some(id) => Some(checked_revision(db, ws, p.id, id).await?),
        None => None,
    };
    Ok(StorefrontDesignState {
        version: p.version,
        draft_revision_id: p.draft_revision_id,
        published_revision_id: p.published_revision_id,
        draft,
    })
}
/// Staff state; read never creates a storefront.
pub async fn state(db: &PgPool, ws: Uuid) -> Result<StorefrontDesignState, AppError> {
    state_on(&mut *db.acquire().await?, ws).await
}

/// Bounded history metadata with pagination total.
pub async fn history(
    db: &PgPool,
    ws: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<StorefrontDesignRevisionSummary>, i64), AppError> {
    let Some(page) = repo::page(db, ws).await? else {
        return Ok((Vec::new(), 0));
    };
    let rows = repo::history(db, ws, page.id, limit, offset).await?;
    let total = repo::history_count(db, ws, page.id).await?;
    Ok((rows, total))
}

async fn validate_bindings(
    db: &mut PgConnection,
    ws: Uuid,
    doc: &StorefrontDesignDocument,
) -> Result<(), AppError> {
    doc.check()?;
    let mut categories: Vec<_> = doc
        .sections
        .iter()
        .filter_map(|s| match s {
            StorefrontDesignSection::ProductGrid { props, .. } => props.category_id,
            _ => None,
        })
        .collect();
    categories.sort_unstable();
    categories.dedup();
    if !repo::categories_exist(db, ws, &categories).await? {
        return Err(AppError::field(
            "document.sections",
            "Category must belong to this workspace.",
        ));
    }
    Ok(())
}
fn require_key(key: Option<&Key>) -> Result<(), AppError> {
    if key.is_none() {
        Err(AppError::field(
            "Idempotency-Key",
            "An Idempotency-Key header is required.",
        ))
    } else {
        Ok(())
    }
}
fn version(page: &PageRow, expected: i64) -> Result<(), AppError> {
    if page.version != expected {
        Err(AppError::Conflict(STALE))
    } else {
        Ok(())
    }
}
async fn finish(
    db: &mut PgConnection,
    ws: Uuid,
    key: Option<&Key>,
    status: u16,
) -> Result<Answer, AppError> {
    let state = state_on(db, ws).await?;
    let answer = Answer {
        status,
        body: serde_json::to_vec(&json!({"data":state}))
            .map_err(|_| AppError::Internal("design response serialization failed".into()))?,
    };
    idempotency::finish(db, ws, key, &answer).await?;
    Ok(answer)
}
/// Save complete phase-one document as new immutable draft.
pub async fn save(
    db: &PgPool,
    access: &WorkspaceAccess,
    input: StorefrontDesignSave,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    require_key(key.as_ref())?;
    let ws = access.workspace_id;
    let mut tx = db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(answer);
    }
    let page = repo::lock_page(&mut tx, ws).await?;
    version(&page, input.expected_version)?;
    validate_bindings(&mut tx, ws, &input.document).await?;
    let document = serde_json::to_value(&input.document)
        .map_err(|_| AppError::Internal("design serialization failed".into()))?;
    let id = repo::insert_revision(
        &mut tx,
        ws,
        page.id,
        NewRevision {
            parent: page.draft_revision_id,
            restored: None,
            document: &document,
            actor: access.user_id,
            login: access.token_id,
        },
    )
    .await?;
    repo::draft(&mut tx, ws, page.id, id).await?;
    repo::event(
        &mut tx,
        ws,
        &page,
        id,
        "saved",
        access.user_id,
        access.token_id,
    )
    .await?;
    let answer = finish(&mut tx, ws, key.as_ref(), 201).await?;
    tx.commit().await?;
    Ok(answer)
}
/// Restore owned historical document into new draft; publishing remains separate.
pub async fn restore(
    db: &PgPool,
    access: &WorkspaceAccess,
    input: StorefrontDesignSelect,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    require_key(key.as_ref())?;
    let ws = access.workspace_id;
    let mut tx = db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(answer);
    }
    let page = repo::lock_page(&mut tx, ws).await?;
    version(&page, input.expected_version)?;
    let selected = checked_revision(&mut tx, ws, page.id, input.revision_id).await?;
    validate_bindings(&mut tx, ws, &selected.document).await?;
    let document = serde_json::to_value(selected.document)
        .map_err(|_| AppError::Internal("design serialization failed".into()))?;
    let id = repo::insert_revision(
        &mut tx,
        ws,
        page.id,
        NewRevision {
            parent: page.draft_revision_id,
            restored: Some(input.revision_id),
            document: &document,
            actor: access.user_id,
            login: access.token_id,
        },
    )
    .await?;
    repo::draft(&mut tx, ws, page.id, id).await?;
    repo::event(
        &mut tx,
        ws,
        &page,
        id,
        "restored",
        access.user_id,
        access.token_id,
    )
    .await?;
    let answer = finish(&mut tx, ws, key.as_ref(), 201).await?;
    tx.commit().await?;
    Ok(answer)
}
/// Publish exactly current draft after full validation in same pointer/audit transaction.
pub async fn publish(
    db: &PgPool,
    access: &WorkspaceAccess,
    input: StorefrontDesignSelect,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    require_key(key.as_ref())?;
    let ws = access.workspace_id;
    let mut tx = db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(answer);
    }
    let page = repo::lock_page(&mut tx, ws).await?;
    version(&page, input.expected_version)?;
    if page.draft_revision_id != Some(input.revision_id) {
        return Err(AppError::Conflict(
            "Only the current draft can be published.",
        ));
    }
    if page.published_revision_id == Some(input.revision_id) {
        return Err(AppError::Conflict("This revision is already published."));
    }
    let selected = checked_revision(&mut tx, ws, page.id, input.revision_id).await?;
    validate_bindings(&mut tx, ws, &selected.document).await?;
    repo::publish(&mut tx, ws, page.id, input.revision_id, access.user_id).await?;
    repo::event(
        &mut tx,
        ws,
        &page,
        input.revision_id,
        "published",
        access.user_id,
        access.token_id,
    )
    .await?;
    let answer = finish(&mut tx, ws, key.as_ref(), 200).await?;
    tx.commit().await?;
    Ok(answer)
}
async fn collections(
    state: &AppState,
    ws: Uuid,
    doc: &StorefrontDesignDocument,
) -> Result<Vec<StorefrontDesignCollection>, AppError> {
    let mut collections = Vec::new();
    for section in &doc.sections {
        if let StorefrontDesignSection::ProductGrid { id, props } = section {
            let (products, _) = storefront::service::products(
                state,
                ws,
                None,
                props.category_id,
                i64::from(props.limit),
                0,
            )
            .await?;
            collections.push(StorefrontDesignCollection {
                section_id: id.clone(),
                products,
            });
        }
    }
    Ok(collections)
}
/// Authorized preview of explicitly selected owned revision; no preview credentials generated.
pub async fn preview(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
) -> Result<StorefrontDesignPreview, AppError> {
    let mut db = state.db.acquire().await?;
    let page = repo::page(&mut *db, ws).await?.ok_or(AppError::NotFound)?;
    let revision = checked_revision(&mut db, ws, page.id, id).await?;
    drop(db);
    let collections = collections(state, ws, &revision.document).await?;
    Ok(StorefrontDesignPreview {
        version: page.version,
        revision_id: id,
        document: revision.document,
        collections,
    })
}
/// Only published pointer can supply public storefront design; invalid stored doc reads as none.
pub async fn published(
    state: &AppState,
    ws: Uuid,
) -> Result<Option<StorefrontPublishedDesign>, AppError> {
    let mut db = state.db.acquire().await?;
    let Some(page) = repo::page(&mut *db, ws).await? else {
        return Ok(None);
    };
    let Some(id) = page.published_revision_id else {
        return Ok(None);
    };
    let revision = match checked_revision(&mut db, ws, page.id, id).await {
        Ok(revision) => revision,
        // Stale stored doc must not break Shop; ids only, no document content in logs.
        Err(AppError::Validation(_)) => {
            tracing::warn!(workspace_id = %ws, revision_id = %id, "published storefront design no longer validates; serving none");
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    drop(db);
    let collections = collections(state, ws, &revision.document).await?;
    Ok(Some(StorefrontPublishedDesign {
        revision_id: id,
        document: revision.document,
        collections,
    }))
}
