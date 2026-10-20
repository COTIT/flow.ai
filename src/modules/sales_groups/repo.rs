//! SQL for sales_groups. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::sales_groups::dto::{NewSalesGroup, SalesGroupPatch};

/// Live sales group.
pub struct SalesGroupRow {
    pub id: Uuid,
    pub name: String,
    pub purchase_code: Option<String>,
    pub revenue_code: Option<String>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    g: &NewSalesGroup,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO sales_groups (workspace_id, name, purchase_code, revenue_code, meta)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
        workspace_id,
        g.name,
        g.purchase_code,
        g.revenue_code,
        g.meta,
    )
    .fetch_one(db)
    .await
}

/// Live group of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<SalesGroupRow>, sqlx::Error> {
    sqlx::query_as!(
        SalesGroupRow,
        "SELECT id, name, purchase_code, revenue_code, meta, created_at, updated_at
         FROM sales_groups
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// True when another live group of `workspace_id` has this name, ignoring case.
pub async fn name_taken(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    name: &str,
    except: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM sales_groups
             WHERE workspace_id = $1 AND lower(name) = lower($2) AND deleted_at IS NULL
               AND id IS DISTINCT FROM $3
           ) AS "taken!""#,
        workspace_id,
        name,
        except,
    )
    .fetch_one(db)
    .await
}

/// One page, oldest first, plus the total matching count (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SalesGroupRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, purchase_code, revenue_code, meta, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM sales_groups
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)
           ORDER BY id
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        pattern,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| SalesGroupRow {
            id: r.id,
            name: r.name,
            purchase_code: r.purchase_code,
            revenue_code: r.revenue_code,
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
    pattern: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM sales_groups
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)"#,
        workspace_id,
        pattern,
    )
    .fetch_one(db)
    .await
}

/// Name always set (Laravel update rules require it); codes and meta patch.
/// False = no such row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &SalesGroupPatch,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE sales_groups
         SET name = $3,
             purchase_code = CASE WHEN $4 THEN $5 ELSE purchase_code END,
             revenue_code = CASE WHEN $6 THEN $7 ELSE revenue_code END,
             meta = CASE WHEN $8 THEN $9 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        p.name,
        p.purchase_code.is_some(),
        p.purchase_code.as_ref().and_then(Option::as_deref),
        p.revenue_code.is_some(),
        p.revenue_code.as_ref().and_then(Option::as_deref),
        p.meta.is_some(),
        p.meta.as_ref().and_then(Option::as_ref),
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
        "UPDATE sales_groups SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
