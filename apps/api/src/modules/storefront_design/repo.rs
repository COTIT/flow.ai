//! Workspace/page-filtered design SQL; service owns transactions and validation.

use super::dto::StorefrontDesignRevisionSummary;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor, types::Json};
use uuid::Uuid;

/// Mutable page pointers protected by one row lock on writes.
pub struct PageRow {
    pub id: Uuid,
    pub version: i64,
    pub draft_revision_id: Option<Uuid>,
    pub published_revision_id: Option<Uuid>,
}
/// Immutable stored presentation payload.
pub struct RevisionRow {
    pub id: Uuid,
    pub document: Json<Value>,
}

/// Existing home state, without creating empty records for reads.
pub async fn page(db: impl PgExecutor<'_>, ws: Uuid) -> Result<Option<PageRow>, sqlx::Error> {
    sqlx::query_as!(PageRow,"SELECT id,version,draft_revision_id,published_revision_id FROM storefront_pages WHERE workspace_id=$1 AND page_key='home'",ws).fetch_optional(db).await
}
/// Create missing identity/page then lock home for optimistic concurrency.
pub async fn lock_page(db: &mut PgConnection, ws: Uuid) -> Result<PageRow, sqlx::Error> {
    sqlx::query!(
        "INSERT INTO storefronts(workspace_id) VALUES($1) ON CONFLICT(workspace_id) DO NOTHING",
        ws
    )
    .execute(&mut *db)
    .await?;
    sqlx::query!("INSERT INTO storefront_pages(workspace_id,storefront_id,page_key) SELECT $1,id,'home' FROM storefronts WHERE workspace_id=$1 ON CONFLICT(workspace_id,page_key) DO NOTHING",ws).execute(&mut *db).await?;
    sqlx::query_as!(PageRow,"SELECT id,version,draft_revision_id,published_revision_id FROM storefront_pages WHERE workspace_id=$1 AND page_key='home' FOR UPDATE",ws).fetch_one(db).await
}
/// Owned revision; both page and workspace constrain lookup.
pub async fn revision(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    page: Uuid,
    id: Uuid,
) -> Result<Option<RevisionRow>, sqlx::Error> {
    sqlx::query_as!(RevisionRow,"SELECT id,document AS \"document: Json<Value>\" FROM storefront_design_revisions WHERE workspace_id=$1 AND page_id=$2 AND id=$3",ws,page,id).fetch_optional(db).await
}
/// Category binding must reference a live category in same workspace.
pub async fn categories_exist(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!("SELECT count(*) = cardinality($2::uuid[]) AS \"valid!\" FROM product_categories WHERE workspace_id=$1 AND id=ANY($2) AND deleted_at IS NULL",ws,ids).fetch_one(db).await
}
/// New immutable revision metadata.
pub struct NewRevision<'a> {
    pub parent: Option<Uuid>,
    pub restored: Option<Uuid>,
    pub document: &'a Value,
    pub actor: Uuid,
    pub login: Uuid,
}
/// Append validated document; never update revision content.
pub async fn insert_revision(
    db: &mut PgConnection,
    ws: Uuid,
    page: Uuid,
    new: NewRevision<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!("INSERT INTO storefront_design_revisions(workspace_id,page_id,parent_revision_id,restored_from_revision_id,schema_version,document,created_by,login_id) VALUES($1,$2,$3,$4,1,$5,$6,$7) RETURNING id",ws,page,new.parent,new.restored,new.document,new.actor,new.login).fetch_one(db).await
}
/// Advance draft pointer/version on locked page.
pub async fn draft(
    db: &mut PgConnection,
    ws: Uuid,
    page: Uuid,
    revision: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE storefront_pages SET draft_revision_id=$3,version=version+1,updated_at=clock_timestamp() WHERE workspace_id=$1 AND id=$2",ws,page,revision).execute(db).await?;
    Ok(())
}
/// Advance published pointer/version on locked page.
pub async fn publish(
    db: &mut PgConnection,
    ws: Uuid,
    page: Uuid,
    revision: Uuid,
    actor: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE storefront_pages SET published_revision_id=$3,published_at=clock_timestamp(),published_by=$4,version=version+1,updated_at=clock_timestamp() WHERE workspace_id=$1 AND id=$2",ws,page,revision,actor).execute(db).await?;
    Ok(())
}
/// Append operation audit atomically with changed pointer.
pub async fn event(
    db: &mut PgConnection,
    ws: Uuid,
    page: &PageRow,
    revision: Uuid,
    kind: &str,
    actor: Uuid,
    login: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("INSERT INTO storefront_design_events(workspace_id,page_id,revision_id,previous_draft_revision_id,previous_published_revision_id,page_version,kind,actor_id,login_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",ws,page.id,revision,page.draft_revision_id,page.published_revision_id,page.version+1,kind,actor,login).execute(db).await?;
    Ok(())
}
/// Bounded page history (history index) includes whether revision ever published.
pub async fn history(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    page: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<StorefrontDesignRevisionSummary>, sqlx::Error> {
    sqlx::query_as!(StorefrontDesignRevisionSummary,r#"SELECT r.id AS revision_id,r.created_at,r.created_by,r.restored_from_revision_id,EXISTS(SELECT 1 FROM storefront_design_events e WHERE e.workspace_id=$1 AND e.page_id=r.page_id AND e.revision_id=r.id AND e.kind='published') AS "was_published!" FROM storefront_design_revisions r WHERE r.workspace_id=$1 AND r.page_id=$2 ORDER BY r.created_at DESC,r.id DESC LIMIT $3 OFFSET $4"#,ws,page,limit,offset).fetch_all(db).await
}
/// Total history rows for pagination; same filter as `history`, history index.
pub async fn history_count(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    page: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!("SELECT count(*) AS \"total!\" FROM storefront_design_revisions WHERE workspace_id=$1 AND page_id=$2",ws,page).fetch_one(db).await
}
