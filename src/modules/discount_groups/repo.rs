//! SQL for discount groups and their customer-relation links. Every query is scoped to one
//! workspace and live rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::discount_groups::dto::{GroupPatch, NewGroup};

/// One live discount group.
pub struct GroupRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub internal: bool,
    pub supplier_relation_id: Uuid,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Matching rows for the whole list (window count); None on single reads.
    pub total: Option<i64>,
}

/// New group; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    g: &NewGroup,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO discount_groups (workspace_id, name, active, internal, supplier_relation_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        workspace_id,
        g.name,
        g.active,
        g.internal,
        g.supplier_relation_id,
        g.meta,
    )
    .fetch_one(db)
    .await
}

/// Live group of the workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<GroupRow>, sqlx::Error> {
    sqlx::query_as!(
        GroupRow,
        r#"SELECT id, name, active, internal, supplier_relation_id, meta, created_at, updated_at,
                  NULL::bigint AS total
           FROM discount_groups WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL"#,
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Filters of the group list.
pub struct ListFilter<'a> {
    pub pattern: Option<&'a str>,
    pub active: Option<bool>,
    pub internal: Option<bool>,
}

/// One page (oldest first) with the window count.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<Vec<GroupRow>, sqlx::Error> {
    sqlx::query_as!(
        GroupRow,
        r#"SELECT id, name, active, internal, supplier_relation_id, meta, created_at, updated_at,
                  count(*) OVER () AS total
           FROM discount_groups
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)
             AND ($3::bool IS NULL OR active = $3)
             AND ($4::bool IS NULL OR internal = $4)
           ORDER BY id LIMIT $5 OFFSET $6"#,
        workspace_id,
        f.pattern,
        f.active,
        f.internal,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// Matching rows when the page came back empty.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM discount_groups
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)
             AND ($3::bool IS NULL OR active = $3)
             AND ($4::bool IS NULL OR internal = $4)"#,
        workspace_id,
        f.pattern,
        f.active,
        f.internal,
    )
    .fetch_one(db)
    .await
}

/// Patch name / active / meta (internal and supplier never change). True when a row matched.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &GroupPatch,
) -> Result<bool, sqlx::Error> {
    let (meta_sent, meta) = match &p.meta {
        Some(m) => (true, m.clone()),
        None => (false, None),
    };
    let n = sqlx::query!(
        "UPDATE discount_groups
         SET name = COALESCE($3, name), active = COALESCE($4, active),
             meta = CASE WHEN $5 THEN $6 ELSE meta END, updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        p.name,
        p.active,
        meta_sent,
        meta,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// Soft delete the group and its discounts and drop its relation links, in the caller's
/// transaction. True when the group was live.
pub async fn retire(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE discount_groups SET deleted_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(false);
    }
    sqlx::query!(
        "UPDATE discounts SET deleted_at = now()
         WHERE discount_group_id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "DELETE FROM discount_group_relation WHERE discount_group_id = $1 AND workspace_id = $2",
        id,
        workspace_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

/// How many of `ids` are live relations of the workspace.
pub async fn live_relations(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM relations
           WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL"#,
        ids,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Link relations to the group; existing links stay (Laravel syncWithoutDetaching).
pub async fn attach(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    group: Uuid,
    ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO discount_group_relation (workspace_id, discount_group_id, relation_id)
         SELECT $1, $2, unnest($3::uuid[])
         ON CONFLICT DO NOTHING",
        workspace_id,
        group,
        ids,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Unlink relations from the group.
pub async fn detach(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    group: Uuid,
    ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM discount_group_relation
         WHERE workspace_id = $1 AND discount_group_id = $2 AND relation_id = ANY($3)",
        workspace_id,
        group,
        ids,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// One page of the group's live relations (oldest first) and the total.
pub async fn relation_page(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    group: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Uuid>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT r.id, count(*) OVER () AS "total!"
           FROM discount_group_relation gr
           JOIN relations r ON r.workspace_id = gr.workspace_id AND r.id = gr.relation_id
                AND r.deleted_at IS NULL
           WHERE gr.workspace_id = $1 AND gr.discount_group_id = $2
           ORDER BY r.id LIMIT $3 OFFSET $4"#,
        workspace_id,
        group,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    Ok((rows.into_iter().map(|r| r.id).collect(), total))
}

/// Laravel DiscountGroupForRelationResource rows of `relation_id`: the groups it supplies and
/// the groups attached to it, each `{id, name, status, meta}` (`status` has no column: null).
pub async fn for_relation(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_id: Uuid,
) -> Result<(Vec<Value>, Vec<Value>), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT g.id, g.name, g.meta, g.supplier_relation_id = $2 AS "supplied!",
                  EXISTS (SELECT 1 FROM discount_group_relation gr
                          WHERE gr.discount_group_id = g.id AND gr.relation_id = $2) AS "attached!"
           FROM discount_groups g
           WHERE g.workspace_id = $1 AND g.deleted_at IS NULL
             AND (g.supplier_relation_id = $2
                  OR EXISTS (SELECT 1 FROM discount_group_relation gr
                             WHERE gr.discount_group_id = g.id AND gr.relation_id = $2))
           ORDER BY g.id"#,
        workspace_id,
        relation_id,
    )
    .fetch_all(db)
    .await?;
    let (mut supplied, mut attached) = (Vec::new(), Vec::new());
    for r in rows {
        let v = serde_json::json!({ "id": r.id, "name": r.name, "status": null, "meta": r.meta });
        if r.attached {
            attached.push(v.clone());
        }
        if r.supplied {
            supplied.push(v);
        }
    }
    Ok((supplied, attached))
}
