//! SQL for product_categories. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::product_categories::dto::CategoryInput;

/// Live category.
pub struct CategoryRow {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub meta: Option<Value>,
    /// Live products of any status in this category (count subquery in each SELECT).
    pub products_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List filter: `parent_id` wins over `root_only` (Laravel order).
pub struct ListFilter<'a> {
    pub pattern: Option<&'a str>,
    pub parent_id: Option<Uuid>,
    pub root_only: bool,
    pub limit: i64,
    pub offset: i64,
}

/// New row; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    c: &CategoryInput,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO product_categories (workspace_id, parent_id, name, description, meta)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
        workspace_id,
        c.parent_id.flatten(),
        c.name.as_deref().unwrap_or_default(),
        c.description.clone().flatten(),
        c.meta.clone().flatten(),
    )
    .fetch_one(db)
    .await
}

/// Live category of `workspace_id`. None = 404.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<CategoryRow>, sqlx::Error> {
    sqlx::query_as!(
        CategoryRow,
        "SELECT id, parent_id, name, description, meta, created_at, updated_at"
            + r#", (SELECT count(*) FROM products p
              WHERE p.workspace_id = c.workspace_id AND p.category_id = c.id
                AND p.deleted_at IS NULL) AS "products_count!""#
            + " FROM product_categories c
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Live children of `id`, oldest first.
pub async fn children(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Vec<CategoryRow>, sqlx::Error> {
    sqlx::query_as!(
        CategoryRow,
        "SELECT id, parent_id, name, description, meta, created_at, updated_at"
            + r#", (SELECT count(*) FROM products p
              WHERE p.workspace_id = c.workspace_id AND p.category_id = c.id
                AND p.deleted_at IS NULL) AS "products_count!""#
            + " FROM product_categories c
         WHERE workspace_id = $1 AND parent_id = $2 AND deleted_at IS NULL
         ORDER BY id",
        workspace_id,
        id,
    )
    .fetch_all(db)
    .await
}

/// True when `parent` is a live category of the workspace and not `id` or one of its
/// Serialise re-parenting per workspace; the caller then checks for cycles and writes.
pub async fn lock_tree(conn: &mut PgConnection, workspace_id: Uuid) -> Result<(), sqlx::Error> {
    // A waiter holds a pooled connection; give up instead of starving the pool.
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext('product_categories'), hashtext($1::text))",
        workspace_id.to_string(),
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// descendants (that would make a cycle). `id` None = a new category.
pub async fn parent_ok(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    parent: Uuid,
    id: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"WITH RECURSIVE up AS (
               SELECT id, parent_id, 1 AS depth FROM product_categories
               WHERE id = $2 AND workspace_id = $1 AND deleted_at IS NULL
               UNION ALL
               SELECT c.id, c.parent_id, up.depth + 1 FROM product_categories c
               JOIN up ON c.id = up.parent_id AND c.workspace_id = $1
               WHERE up.depth < 1000
           )
           SELECT EXISTS (SELECT 1 FROM up WHERE id = $2)
              AND NOT EXISTS (SELECT 1 FROM up WHERE id = $3) AS "ok!""#,
        workspace_id,
        parent,
        id,
    )
    .fetch_one(db)
    .await
}

/// One page, oldest first, plus the total (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<CategoryRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, parent_id, name, description, meta, created_at, updated_at,
                  count(*) OVER () AS "total!""#
            + r#", (SELECT count(*) FROM products p
              WHERE p.workspace_id = c.workspace_id AND p.category_id = c.id
                AND p.deleted_at IS NULL) AS "products_count!""#
            + r#" FROM product_categories c
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)
             AND ($3::uuid IS NULL OR parent_id = $3)
             AND ($3::uuid IS NOT NULL OR NOT $4 OR parent_id IS NULL)
           ORDER BY id LIMIT $5 OFFSET $6"#,
        workspace_id,
        f.pattern,
        f.parent_id,
        f.root_only,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| CategoryRow {
            id: r.id,
            parent_id: r.parent_id,
            name: r.name,
            description: r.description,
            meta: r.meta,
            products_count: r.products_count,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// True when another live category of the workspace (not `except`) has `name`, compared
/// trimmed and case-insensitive.
pub async fn name_taken(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    name: &str,
    except: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM product_categories
               WHERE workspace_id = $1 AND deleted_at IS NULL
                 AND lower(btrim(name)) = lower(btrim($2))
                 AND ($3::uuid IS NULL OR id <> $3)
           ) AS "taken!""#,
        workspace_id,
        name,
        except,
    )
    .fetch_one(db)
    .await
}

/// Matching count, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM product_categories
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)
             AND ($3::uuid IS NULL OR parent_id = $3)
             AND ($3::uuid IS NOT NULL OR NOT $4 OR parent_id IS NULL)"#,
        workspace_id,
        f.pattern,
        f.parent_id,
        f.root_only,
    )
    .fetch_one(db)
    .await
}

/// Patch in one statement. False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    c: &CategoryInput,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE product_categories SET
             name = COALESCE($3, name),
             parent_id = CASE WHEN $4 THEN $5 ELSE parent_id END,
             description = CASE WHEN $6 THEN $7 ELSE description END,
             meta = CASE WHEN $8 THEN $9 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        c.name.as_deref(),
        c.parent_id.is_some(),
        c.parent_id.flatten(),
        c.description.is_some(),
        c.description.clone().flatten(),
        c.meta.is_some(),
        c.meta.clone().flatten(),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete; children keep pointing at it (Laravel parity). False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    // One statement: products lose the link in the same snapshot the category goes.
    sqlx::query!(
        "WITH gone AS (
             UPDATE product_categories SET deleted_at = now(), updated_at = now()
             WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
             RETURNING id
         ), cleared AS (
             UPDATE products SET category_id = NULL, updated_at = now()
             WHERE workspace_id = $2 AND category_id IN (SELECT id FROM gone)
         )
         SELECT id FROM gone",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
    .map(|row| row.is_some())
}
