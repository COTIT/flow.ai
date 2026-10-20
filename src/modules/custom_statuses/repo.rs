//! SQL for custom_statuses. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Live custom status (archived ones included).
pub struct CustomStatusRow {
    pub id: Uuid,
    pub name: String,
    pub domain: String,
    pub archived: bool,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List filter. Archived rows never list (Laravel ListCustomStatus).
pub struct ListFilter<'a> {
    pub domain: Option<&'a str>,
    pub pattern: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

/// New rows of one workspace and domain in one statement (uuidv7 ids follow `names` order).
pub async fn insert_many(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    domain: &str,
    names: &[&str],
    meta: Option<&Value>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO custom_statuses (workspace_id, name, domain, meta)
         SELECT $1, t.name, $2, $4
         FROM unnest($3::text[]) WITH ORDINALITY AS t(name, n)
         ORDER BY t.n
         RETURNING id",
        workspace_id,
        domain,
        names as &[&str],
        meta,
    )
    .fetch_all(db)
    .await
}

/// Live status of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<CustomStatusRow>, sqlx::Error> {
    sqlx::query_as!(
        CustomStatusRow,
        "SELECT id, name, domain, archived, meta, created_at, updated_at
         FROM custom_statuses
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// True when another live status of this domain (archived or not) has the name, ignoring case.
pub async fn name_taken(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    domain: &str,
    name: &str,
    except: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM custom_statuses
             WHERE workspace_id = $1 AND domain = $2 AND lower(name) = lower($3)
               AND deleted_at IS NULL AND id IS DISTINCT FROM $4
           ) AS "taken!""#,
        workspace_id,
        domain,
        name,
        except,
    )
    .fetch_one(db)
    .await
}

/// One page of non-archived statuses, oldest first, plus the total matching count.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<CustomStatusRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, domain, archived, meta, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM custom_statuses
           WHERE workspace_id = $1 AND deleted_at IS NULL AND NOT archived
             AND ($2::text IS NULL OR domain = $2)
             AND ($3::text IS NULL OR name ILIKE $3)
           ORDER BY id
           LIMIT $4 OFFSET $5"#,
        workspace_id,
        f.domain,
        f.pattern,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| CustomStatusRow {
            id: r.id,
            name: r.name,
            domain: r.domain,
            archived: r.archived,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Matching count, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM custom_statuses
           WHERE workspace_id = $1 AND deleted_at IS NULL AND NOT archived
             AND ($2::text IS NULL OR domain = $2)
             AND ($3::text IS NULL OR name ILIKE $3)"#,
        workspace_id,
        f.domain,
        f.pattern,
    )
    .fetch_one(db)
    .await
}

/// Patch: None = keep, `Some(None)` clears meta. False = no such row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    name: Option<&str>,
    meta: Option<Option<&Value>>,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE custom_statuses
         SET name = COALESCE($3, name),
             meta = CASE WHEN $4 THEN $5 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        name,
        meta.is_some(),
        meta.flatten(),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete; frees the name. False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE custom_statuses SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Laravel WorkspaceSetup deal pipeline statuses for a new workspace.
pub async fn seed_defaults(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<(), sqlx::Error> {
    let names = ["Won", "Lost", "In Progress", "To Do", "Completed"];
    insert_many(db, workspace_id, "deal_pipeline", &names, None)
        .await
        .map(drop)
}
