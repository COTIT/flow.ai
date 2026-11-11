//! Workspace/page-filtered design SQL; service owns transactions and validation.

use super::dto::{
    StorefrontDesignRevisionSummary, StorefrontPreviewLink, StorefrontPreviewLinkStatus,
};
use chrono::{DateTime, Utc};
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
/// Live same-workspace category names for review rows; ids absent here are omitted.
pub async fn category_names(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query!("SELECT id,name FROM product_categories WHERE workspace_id=$1 AND id=ANY($2) AND deleted_at IS NULL",ws,ids).fetch_all(db).await.map(|rows| rows.into_iter().map(|r| (r.id, r.name)).collect())
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
    sqlx::query_as!(StorefrontDesignRevisionSummary,r#"SELECT r.id AS revision_id,r.parent_revision_id,r.created_at,r.created_by,r.restored_from_revision_id,EXISTS(SELECT 1 FROM storefront_design_events e WHERE e.workspace_id=$1 AND e.page_id=r.page_id AND e.revision_id=r.id AND e.kind='published') AS "was_published!" FROM storefront_design_revisions r WHERE r.workspace_id=$1 AND r.page_id=$2 ORDER BY r.created_at DESC,r.id DESC LIMIT $3 OFFSET $4"#,ws,page,limit,offset).fetch_all(db).await
}
/// Total history rows for pagination; same filter as `history`, history index.
pub async fn history_count(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    page: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!("SELECT count(*) AS \"total!\" FROM storefront_design_revisions WHERE workspace_id=$1 AND page_id=$2",ws,page).fetch_one(db).await
}
/// Read-only snapshot: page pointer and revision come from one moment. Call first in a transaction.
pub async fn snapshot(db: &mut PgConnection) -> Result<(), sqlx::Error> {
    // SET cannot be a prepared statement; fixed text, no input.
    sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(db)
        .await?;
    Ok(())
}

/// Link row with status from the database clock and live page pointers.
pub struct LinkRow {
    pub link_id: Uuid,
    pub revision_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub status: String,
}
impl From<LinkRow> for StorefrontPreviewLink {
    fn from(r: LinkRow) -> Self {
        let status = match r.status.as_str() {
            "active" => StorefrontPreviewLinkStatus::Active,
            "revoked" => StorefrontPreviewLinkStatus::Revoked,
            "expired" => StorefrontPreviewLinkStatus::Expired,
            _ => StorefrontPreviewLinkStatus::Ended,
        };
        Self {
            link_id: r.link_id,
            revision_id: r.revision_id,
            created_at: r.created_at,
            created_by: r.created_by,
            expires_at: r.expires_at,
            revoked_at: r.revoked_at,
            status,
        }
    }
}
/// New link for `revision`; lifetime from the database clock.
pub struct NewLink<'a> {
    pub page: Uuid,
    pub revision: Uuid,
    pub hash: &'a [u8],
    pub hours: i32,
    pub actor: Uuid,
    pub login: Uuid,
}
/// Store hash only; plaintext never reaches SQL.
pub async fn insert_link(
    db: &mut PgConnection,
    ws: Uuid,
    new: NewLink<'_>,
) -> Result<LinkRow, sqlx::Error> {
    sqlx::query_as!(LinkRow,r#"INSERT INTO storefront_preview_links(workspace_id,page_id,revision_id,token_hash,created_by,login_id,expires_at) VALUES($1,$2,$3,$4,$5,$6,now()+make_interval(hours=>$7)) RETURNING id AS link_id,revision_id,created_at,created_by,expires_at,revoked_at,'active' AS "status!""#,ws,new.page,new.revision,new.hash,new.actor,new.login,new.hours).fetch_one(db).await
}
/// Unrevoked, unexpired links of one revision (revision index).
pub async fn active_links(
    db: &mut PgConnection,
    ws: Uuid,
    revision: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM storefront_preview_links WHERE workspace_id=$1 AND revision_id=$2 AND revoked_at IS NULL AND expires_at>now()"#,ws,revision).fetch_one(db).await
}
/// Newest 50 links of one revision; status precedence revoked, expired, ended.
pub async fn links(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    revision: Uuid,
) -> Result<Vec<LinkRow>, sqlx::Error> {
    sqlx::query_as!(LinkRow,r#"SELECT l.id AS link_id,l.revision_id,l.created_at,l.created_by,l.expires_at,l.revoked_at,CASE WHEN l.revoked_at IS NOT NULL THEN 'revoked' WHEN l.expires_at<=now() THEN 'expired' WHEN p.draft_revision_id IS DISTINCT FROM l.revision_id OR p.published_revision_id IS NOT DISTINCT FROM l.revision_id THEN 'ended' ELSE 'active' END AS "status!" FROM storefront_preview_links l JOIN storefront_pages p ON p.workspace_id=l.workspace_id AND p.id=l.page_id WHERE l.workspace_id=$1 AND l.revision_id=$2 ORDER BY l.created_at DESC,l.id DESC LIMIT 50"#,ws,revision).fetch_all(db).await
}
/// Revoke once; already revoked or foreign rows stay as they are. Returns the row.
pub async fn revoke(
    db: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    actor: Uuid,
    login: Uuid,
) -> Result<Option<LinkRow>, sqlx::Error> {
    sqlx::query!("UPDATE storefront_preview_links SET revoked_at=now(),revoked_by=$3,revoked_login_id=$4 WHERE workspace_id=$1 AND id=$2 AND revoked_at IS NULL",ws,id,actor,login).execute(&mut *db).await?;
    sqlx::query_as!(LinkRow,r#"SELECT l.id AS link_id,l.revision_id,l.created_at,l.created_by,l.expires_at,l.revoked_at,CASE WHEN l.revoked_at IS NOT NULL THEN 'revoked' WHEN l.expires_at<=now() THEN 'expired' WHEN p.draft_revision_id IS DISTINCT FROM l.revision_id OR p.published_revision_id IS NOT DISTINCT FROM l.revision_id THEN 'ended' ELSE 'active' END AS "status!" FROM storefront_preview_links l JOIN storefront_pages p ON p.workspace_id=l.workspace_id AND p.id=l.page_id WHERE l.workspace_id=$1 AND l.id=$2"#,ws,id).fetch_optional(db).await
}
/// Live link target: every rule in one statement (hash index). Any miss is None.
pub struct LiveLink {
    pub page_id: Uuid,
    pub revision_id: Uuid,
    pub expires_at: DateTime<Utc>,
}
/// Hash match, same workspace, not revoked, not expired, still the unpublished current draft.
pub async fn live_link(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    hash: &[u8],
) -> Result<Option<LiveLink>, sqlx::Error> {
    sqlx::query_as!(LiveLink,"SELECT l.page_id,l.revision_id,l.expires_at FROM storefront_preview_links l JOIN storefront_pages p ON p.workspace_id=l.workspace_id AND p.id=l.page_id WHERE l.token_hash=$2 AND l.workspace_id=$1 AND l.revoked_at IS NULL AND l.expires_at>now() AND p.draft_revision_id=l.revision_id AND p.published_revision_id IS DISTINCT FROM l.revision_id",ws,hash).fetch_optional(db).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test]
    async fn snapshot_is_read_only_repeatable_read(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        snapshot(&mut tx).await.unwrap();
        let (level, read_only): (String, String) = sqlx::query_as(
            "SELECT current_setting('transaction_isolation'), current_setting('transaction_read_only')",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            (level.as_str(), read_only.as_str()),
            ("repeatable read", "on")
        );
    }
}
