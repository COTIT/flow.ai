//! Design validation and atomic draft/publish transitions; catalog data stays external.

use super::{
    dto::*,
    edit,
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

pub(super) async fn checked_revision(
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
/// New draft child of current draft plus audit; `restored` marks restore over save.
async fn append(
    tx: &mut PgConnection,
    access: &WorkspaceAccess,
    page: &PageRow,
    document: &StorefrontDesignDocument,
    restored: Option<Uuid>,
) -> Result<(), AppError> {
    let ws = access.workspace_id;
    let document = serde_json::to_value(document)
        .map_err(|_| AppError::Internal("design serialization failed".into()))?;
    let id = repo::insert_revision(
        &mut *tx,
        ws,
        page.id,
        NewRevision {
            parent: page.draft_revision_id,
            restored,
            document: &document,
            actor: access.user_id,
            login: access.token_id,
        },
    )
    .await?;
    repo::draft(&mut *tx, ws, page.id, id).await?;
    let kind = if restored.is_some() {
        "restored"
    } else {
        "saved"
    };
    repo::event(tx, ws, page, id, kind, access.user_id, access.token_id).await?;
    Ok(())
}
/// Category ids on review rows become live same-workspace names.
async fn named(
    db: &mut PgConnection,
    ws: Uuid,
    mut rows: Vec<StorefrontDesignChangeRow>,
) -> Result<Vec<StorefrontDesignChangeRow>, AppError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .filter(|r| r.field == "category")
        .flat_map(|r| [r.before.as_deref(), Some(r.after.as_str())])
        .flatten()
        .filter_map(|v| Uuid::parse_str(v).ok())
        .collect();
    if ids.is_empty() {
        return Ok(rows);
    }
    let names = repo::category_names(&mut *db, ws, &ids).await?;
    let name = |v: &str| {
        names
            .iter()
            .find(|(id, _)| id.to_string() == v)
            .map(|(_, n)| n.clone())
    };
    for r in rows.iter_mut().filter(|r| r.field == "category") {
        if let Some(n) = r.before.as_deref().and_then(name) {
            r.before = Some(n);
        }
        if let Some(n) = name(&r.after) {
            r.after = n;
        }
    }
    Ok(rows)
}
/// Edit base: current draft, or starter while page has none; any other base is stale.
async fn edit_base(
    db: &mut PgConnection,
    ws: Uuid,
    page: Option<&PageRow>,
    input: &StorefrontDesignEdit,
) -> Result<StorefrontDesignDocument, AppError> {
    let draft = page.and_then(|p| p.draft_revision_id);
    if page.map_or(0, |p| p.version) != input.expected_version || draft != input.base_revision_id {
        return Err(AppError::Conflict(STALE));
    }
    match (page, draft) {
        (Some(p), Some(id)) => Ok(checked_revision(db, ws, p.id, id).await?.document),
        _ => Ok(starter()),
    }
}
/// Guarded apply plus workspace bindings; shared by write, dry run and review facts.
pub async fn plan_edit(
    db: &mut PgConnection,
    ws: Uuid,
    page: Option<&PageRow>,
    input: &StorefrontDesignEdit,
) -> Result<(StorefrontDesignDocument, Vec<StorefrontDesignChangeRow>), AppError> {
    let base = edit_base(db, ws, page, input).await?;
    let (document, rows) = edit::apply(&base, &input.changes)?;
    validate_bindings(db, ws, &document).await?;
    Ok((document, named(db, ws, rows).await?))
}
/// Review rows for restore (draft → selected) or publish (published → draft), same guards as writes.
pub async fn selection_rows(
    db: &mut PgConnection,
    ws: Uuid,
    page: Option<&PageRow>,
    input: &StorefrontDesignSelect,
    publish: bool,
) -> Result<Vec<StorefrontDesignChangeRow>, AppError> {
    let page = page.ok_or(AppError::NotFound)?;
    version(page, input.expected_version)?;
    if publish {
        if page.draft_revision_id != Some(input.revision_id) {
            return Err(AppError::Conflict(
                "Only the current draft can be published.",
            ));
        }
        if page.published_revision_id == Some(input.revision_id) {
            return Err(AppError::Conflict("This revision is already published."));
        }
    }
    let selected = checked_revision(db, ws, page.id, input.revision_id).await?;
    validate_bindings(db, ws, &selected.document).await?;
    let base_id = if publish {
        page.published_revision_id
    } else {
        page.draft_revision_id
    };
    let base = match base_id {
        // Base that no longer validates reads as none: review shows whole selected design.
        Some(id) => checked_revision(db, ws, page.id, id).await.ok(),
        None => None,
    };
    named(
        db,
        ws,
        edit::diff(base.as_ref().map(|b| &b.document), &selected.document),
    )
    .await
}
/// Targeted edit of current draft saved as new draft; replays like other writes.
pub async fn edit(
    db: &PgPool,
    access: &WorkspaceAccess,
    input: StorefrontDesignEdit,
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
    let (document, _) = plan_edit(&mut tx, ws, Some(&page), &input).await?;
    append(&mut tx, access, &page, &document, None).await?;
    let answer = finish(&mut tx, ws, key.as_ref(), 201).await?;
    tx.commit().await?;
    Ok(answer)
}
/// Dry run of targeted edit; one read-only snapshot, stores nothing.
pub async fn edit_preview(
    state: &AppState,
    ws: Uuid,
    input: StorefrontDesignEdit,
) -> Result<StorefrontDesignEditPreview, AppError> {
    let mut tx = state.db.begin().await?;
    repo::snapshot(&mut tx).await?;
    let page = repo::page(&mut *tx, ws).await?;
    let (document, changes) = plan_edit(&mut tx, ws, page.as_ref(), &input).await?;
    tx.rollback().await?;
    let collections = collections(state, ws, &document).await?;
    Ok(StorefrontDesignEditPreview {
        version: page.as_ref().map_or(0, |p| p.version),
        base_revision_id: input.base_revision_id,
        document,
        collections,
        changes,
    })
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
    append(&mut tx, access, &page, &input.document, None).await?;
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
    append(
        &mut tx,
        access,
        &page,
        &selected.document,
        Some(input.revision_id),
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
pub(super) async fn collections(
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
