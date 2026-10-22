//! SQL for `pdf_templates`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// A live template.
pub struct TemplateRow {
    pub id: Uuid,
    pub name: String,
    pub domain: String,
    pub domain_action: Option<String>,
    pub columns: Value,
    pub header_left: Option<String>,
    pub header_right: Option<String>,
    pub header_top: Option<String>,
    pub footer: Option<String>,
    pub padding_top: i32,
    pub padding_bottom: i32,
    pub padding_x: i32,
    pub enable_summary: bool,
    pub is_default: bool,
    pub conditions: Option<Value>,
    pub extra_fields: Option<Value>,
    pub meta: Option<Value>,
    pub background_file_id: Option<Uuid>,
    pub logo_file_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Every column of a write; on update `set_*` false keeps the stored value.
pub struct Write<'a> {
    pub name: Option<&'a str>,
    /// Create only: the domain's action can't change (Laravel).
    pub domain_action: Option<&'a str>,
    pub columns: Option<&'a Value>,
    pub set_header_left: bool,
    pub header_left: Option<&'a str>,
    pub set_header_right: bool,
    pub header_right: Option<&'a str>,
    pub set_header_top: bool,
    pub header_top: Option<&'a str>,
    pub set_footer: bool,
    pub footer: Option<&'a str>,
    pub padding_top: Option<i32>,
    pub padding_bottom: Option<i32>,
    pub padding_x: Option<i32>,
    pub enable_summary: Option<bool>,
    pub is_default: Option<bool>,
    pub set_conditions: bool,
    pub conditions: Option<&'a Value>,
    pub set_extra_fields: bool,
    pub extra_fields: Option<&'a Value>,
    pub set_meta: bool,
    pub meta: Option<&'a Value>,
    pub set_background: bool,
    pub background_file_id: Option<Uuid>,
    pub set_logo: bool,
    pub logo_file_id: Option<Uuid>,
}

/// New template of `domain`; its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    domain: &str,
    w: &Write<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO pdf_templates (workspace_id, domain, name, domain_action, columns,
                                    header_left, header_right, header_top, footer,
                                    padding_top, padding_bottom, padding_x, enable_summary,
                                    is_default, conditions, extra_fields, meta,
                                    background_file_id, logo_file_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, COALESCE($13, true),
                 COALESCE($14, false), $15, $16, $17, $18, $19)
         RETURNING id",
        workspace_id,
        domain,
        w.name,
        w.domain_action,
        w.columns,
        w.header_left,
        w.header_right,
        w.header_top,
        w.footer,
        w.padding_top,
        w.padding_bottom,
        w.padding_x,
        w.enable_summary,
        w.is_default,
        w.conditions,
        w.extra_fields,
        w.meta,
        w.background_file_id,
        w.logo_file_id,
    )
    .fetch_one(db)
    .await
}

/// Patch the live template `id`; false when there is none.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    w: &Write<'_>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE pdf_templates SET
            name = COALESCE($3, name),
            columns = COALESCE($4, columns),
            header_left = CASE WHEN $5 THEN $6 ELSE header_left END,
            header_right = CASE WHEN $7 THEN $8 ELSE header_right END,
            header_top = CASE WHEN $9 THEN $10 ELSE header_top END,
            footer = CASE WHEN $11 THEN $12 ELSE footer END,
            padding_top = COALESCE($13, padding_top),
            padding_bottom = COALESCE($14, padding_bottom),
            padding_x = COALESCE($15, padding_x),
            enable_summary = COALESCE($16, enable_summary),
            is_default = COALESCE($17, is_default),
            conditions = CASE WHEN $18 THEN $19 ELSE conditions END,
            extra_fields = CASE WHEN $20 THEN $21 ELSE extra_fields END,
            meta = CASE WHEN $22 THEN $23 ELSE meta END,
            background_file_id = CASE WHEN $24 THEN $25 ELSE background_file_id END,
            logo_file_id = CASE WHEN $26 THEN $27 ELSE logo_file_id END,
            updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
        w.name,
        w.columns,
        w.set_header_left,
        w.header_left,
        w.set_header_right,
        w.header_right,
        w.set_header_top,
        w.header_top,
        w.set_footer,
        w.footer,
        w.padding_top,
        w.padding_bottom,
        w.padding_x,
        w.enable_summary,
        w.is_default,
        w.set_conditions,
        w.conditions,
        w.set_extra_fields,
        w.extra_fields,
        w.set_meta,
        w.meta,
        w.set_background,
        w.background_file_id,
        w.set_logo,
        w.logo_file_id,
    )
    .execute(db)
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
        "UPDATE pdf_templates SET deleted_at = now(), updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The live template `id`; with `domain`, only one of that domain.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    domain: Option<&str>,
) -> Result<Option<TemplateRow>, sqlx::Error> {
    sqlx::query_as!(
        TemplateRow,
        "SELECT id, name, domain, domain_action, columns, header_left, header_right, header_top,
                footer, padding_top, padding_bottom, padding_x, enable_summary, is_default,
                conditions, extra_fields, meta, background_file_id, logo_file_id, created_at,
                updated_at
         FROM pdf_templates
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL
           AND ($3::text IS NULL OR domain = $3)",
        workspace_id,
        id,
        domain,
    )
    .fetch_optional(db)
    .await
}

/// List filters (Laravel ListPdfTemplate): a condition matches when equal or unset.
pub struct ListFilter<'a> {
    pub domain: Option<&'a str>,
    /// Escaped ILIKE pattern on the name.
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
        r#"SELECT id, name, domain, domain_action, columns, header_left, header_right,
                  header_top, footer, padding_top, padding_bottom, padding_x, enable_summary,
                  is_default, conditions, extra_fields, meta, background_file_id, logo_file_id,
                  created_at, updated_at
           FROM pdf_templates
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR domain = $2)
             AND ($3::text IS NULL OR name ILIKE $3)
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
        r#"SELECT count(*) AS "n!" FROM pdf_templates
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR domain = $2)
             AND ($3::text IS NULL OR name ILIKE $3)
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

/// True when `id` is a live custom type of the relation domain in this workspace.
pub async fn relation_type_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM custom_types
                          WHERE workspace_id = $1 AND id = $2 AND domain = 'relation'
                            AND deleted_at IS NULL) AS "e!""#,
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}
