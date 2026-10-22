//! SQL for `email_templates`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A live template.
pub struct TemplateRow {
    pub id: Uuid,
    pub template_name: String,
    pub template_domain: String,
    pub default_template: bool,
    pub subject: String,
    pub body: String,
    pub conditions: Option<Value>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Columns of a write; on update None keeps the stored value, `set_*` false keeps a jsonb.
pub struct Write<'a> {
    pub template_name: Option<&'a str>,
    pub default_template: Option<bool>,
    pub subject: Option<&'a str>,
    pub body: &'a str,
    pub set_conditions: bool,
    pub conditions: Option<&'a Value>,
    pub set_meta: bool,
    pub meta: Option<&'a Value>,
}

/// Serialise default changes of one workspace and domain until the transaction ends.
pub async fn lock_domain(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    domain: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended('email_templates.default|'
                                                       || ($1::uuid)::text || '|' || $2, 0))",
        workspace_id,
        domain,
    )
    .execute(tx)
    .await?;
    Ok(())
}

/// Clear the default flag of the domain's other live templates.
pub async fn clear_defaults(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    domain: &str,
    except: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE email_templates SET default_template = false, updated_at = now()
         WHERE workspace_id = $1 AND template_domain = $2 AND default_template
           AND deleted_at IS NULL AND id <> $3",
        workspace_id,
        domain,
        except,
    )
    .execute(tx)
    .await?;
    Ok(())
}

/// New template `id` of `domain`.
pub async fn insert(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    domain: &str,
    w: &Write<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO email_templates (id, workspace_id, template_domain, template_name,
                                      default_template, subject, body, conditions, meta)
         VALUES ($1, $2, $3, $4, COALESCE($5, false), $6, $7, $8, $9)",
        id,
        workspace_id,
        domain,
        w.template_name,
        w.default_template,
        w.subject,
        w.body,
        w.conditions,
        w.meta,
    )
    .execute(tx)
    .await?;
    Ok(())
}

/// Patch the live template `id`; false when there is none.
pub async fn update(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    w: &Write<'_>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE email_templates SET
            template_name = COALESCE($3, template_name),
            default_template = COALESCE($4, default_template),
            subject = COALESCE($5, subject),
            body = $6,
            conditions = CASE WHEN $7 THEN $8 ELSE conditions END,
            meta = CASE WHEN $9 THEN $10 ELSE meta END,
            updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
        w.template_name,
        w.default_template,
        w.subject,
        w.body,
        w.set_conditions,
        w.conditions,
        w.set_meta,
        w.meta,
    )
    .execute(tx)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Soft delete; false when there is no live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE email_templates SET deleted_at = now(), default_template = false,
                                    updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The live template `id`.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<TemplateRow>, sqlx::Error> {
    sqlx::query_as!(
        TemplateRow,
        "SELECT id, template_name, template_domain, default_template, subject, body, conditions,
                meta, created_at, updated_at
         FROM email_templates WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// List filters (Laravel ListEmailTemplate): a condition matches when equal or unset.
pub struct ListFilter<'a> {
    pub domain: Option<&'a str>,
    /// Escaped ILIKE pattern on the name and subject.
    pub search: Option<&'a str>,
    pub relation_type: Option<&'a str>,
    pub language: Option<&'a str>,
    pub relation_custom_type_id: Option<&'a str>,
}

/// One page of live templates, newest first, plus the total.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<TemplateRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        TemplateRow,
        r#"SELECT id, template_name, template_domain, default_template, subject, body,
                  conditions, meta, created_at, updated_at
           FROM email_templates
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR template_domain = $2)
             AND ($3::text IS NULL OR template_name ILIKE $3 OR subject ILIKE $3)
             AND ($4::text IS NULL OR conditions->>'relation_type' IS NULL
                  OR conditions->>'relation_type' = $4)
             AND ($5::text IS NULL OR conditions->>'language' IS NULL
                  OR conditions->>'language' = $5)
             AND ($6::text IS NULL OR conditions->>'relation_custom_type_id' IS NULL
                  OR conditions->>'relation_custom_type_id' = $6)
           ORDER BY created_at DESC, id DESC
           LIMIT $7 OFFSET $8"#,
        workspace_id,
        f.domain,
        f.search,
        f.relation_type,
        f.language,
        f.relation_custom_type_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM email_templates
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR template_domain = $2)
             AND ($3::text IS NULL OR template_name ILIKE $3 OR subject ILIKE $3)
             AND ($4::text IS NULL OR conditions->>'relation_type' IS NULL
                  OR conditions->>'relation_type' = $4)
             AND ($5::text IS NULL OR conditions->>'language' IS NULL
                  OR conditions->>'language' = $5)
             AND ($6::text IS NULL OR conditions->>'relation_custom_type_id' IS NULL
                  OR conditions->>'relation_custom_type_id' = $6)"#,
        workspace_id,
        f.domain,
        f.search,
        f.relation_type,
        f.language,
        f.relation_custom_type_id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}
