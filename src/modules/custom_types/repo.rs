//! SQL for custom_types. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Live custom type.
pub struct CustomTypeRow {
    pub id: Uuid,
    pub name: String,
    pub domain: String,
    pub properties: Option<Value>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List filter. Flags compare `properties->>key` with "true"/"false"; None = any.
pub struct ListFilter<'a> {
    pub domain: Option<&'a str>,
    pub pattern: Option<&'a str>,
    pub default_for_work_order: Option<&'a str>,
    pub is_leave_hours_type: Option<&'a str>,
    pub is_supplier: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

/// New rows of one workspace in one statement (uuidv7 ids follow input order); returns the ids.
pub async fn insert_many(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    names: &[&str],
    domains: &[&str],
    properties: &[Option<Value>],
    meta: Option<&Value>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO custom_types (workspace_id, name, domain, properties, meta)
         SELECT $1, t.name, t.domain, t.properties, $5
         FROM unnest($2::text[], $3::text[], $4::jsonb[]) WITH ORDINALITY
              AS t(name, domain, properties, n)
         ORDER BY t.n
         RETURNING id",
        workspace_id,
        names as &[&str],
        domains as &[&str],
        properties as &[Option<Value>],
        meta,
    )
    .fetch_all(db)
    .await
}

/// Live type of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<CustomTypeRow>, sqlx::Error> {
    sqlx::query_as!(
        CustomTypeRow,
        "SELECT id, name, domain, properties, meta, created_at, updated_at
         FROM custom_types
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// True when another live type of this domain has the name, ignoring case.
pub async fn name_taken(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    domain: &str,
    name: &str,
    except: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM custom_types
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

/// One page, oldest first, plus the total matching count (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<CustomTypeRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, domain, properties, meta, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM custom_types
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR domain = $2)
             AND ($3::text IS NULL OR name ILIKE $3)
             AND ($4::text IS NULL OR properties->>'default_for_work_order' = $4)
             AND ($5::text IS NULL OR properties->>'is_leave_hours_type' = $5)
             AND ($6::text IS NULL OR properties->>'is_supplier' = $6)
           ORDER BY id
           LIMIT $7 OFFSET $8"#,
        workspace_id,
        f.domain,
        f.pattern,
        f.default_for_work_order,
        f.is_leave_hours_type,
        f.is_supplier,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| CustomTypeRow {
            id: r.id,
            name: r.name,
            domain: r.domain,
            properties: r.properties,
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
        r#"SELECT count(*) AS "n!" FROM custom_types
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR domain = $2)
             AND ($3::text IS NULL OR name ILIKE $3)
             AND ($4::text IS NULL OR properties->>'default_for_work_order' = $4)
             AND ($5::text IS NULL OR properties->>'is_leave_hours_type' = $5)
             AND ($6::text IS NULL OR properties->>'is_supplier' = $6)"#,
        workspace_id,
        f.domain,
        f.pattern,
        f.default_for_work_order,
        f.is_leave_hours_type,
        f.is_supplier,
    )
    .fetch_one(db)
    .await
}

/// Patch: None = keep, `Some(None)` clears properties/meta. False = no such row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    name: Option<&str>,
    properties: Option<Option<&Value>>,
    meta: Option<Option<&Value>>,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE custom_types
         SET name = COALESCE($3, name),
             properties = CASE WHEN $4 THEN $5 ELSE properties END,
             meta = CASE WHEN $6 THEN $7 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        name,
        properties.is_some(),
        properties.flatten(),
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
        "UPDATE custom_types SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Laravel WorkspaceSetup custom types for a new workspace, three bugs fixed: hour types use
/// `hour_registration_type` (Laravel wrote `hour_registration`, which vimdesk-web never reads),
/// Supplier carries `is_supplier: true` (Laravel dropped it), and Sick leave carries
/// `is_sick_hours_type` (LeaveRequest's sick check reads it).
pub async fn seed_defaults(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<(), sqlx::Error> {
    let leave = Some(serde_json::json!({ "is_leave_hours_type": true }));
    let sick = Some(serde_json::json!({ "is_leave_hours_type": true, "is_sick_hours_type": true }));
    let rows: [(&str, &str, Option<Value>); 9] = [
        (
            "Supplier",
            "relation",
            Some(serde_json::json!({ "is_supplier": true })),
        ),
        (
            "Client",
            "relation",
            Some(serde_json::json!({ "is_supplier": false })),
        ),
        ("Note", "activity", None),
        (
            "Ticket",
            "activity",
            Some(serde_json::json!({ "is_ticket_type": true })),
        ),
        ("Website", "lead_source", None),
        ("Social media", "lead_source", None),
        ("Work", "hour_registration_type", None),
        ("Leave", "hour_registration_type", leave),
        ("Sick leave", "hour_registration_type", sick),
    ];
    let names: Vec<&str> = rows.iter().map(|r| r.0).collect();
    let domains: Vec<&str> = rows.iter().map(|r| r.1).collect();
    let properties: Vec<Option<Value>> = rows.into_iter().map(|r| r.2).collect();
    insert_many(db, workspace_id, &names, &domains, &properties, None)
        .await
        .map(drop)
}
