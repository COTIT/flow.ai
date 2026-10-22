//! SQL for `email_profiles`. The sealed SMTP password is read only by `lock_smtp` (for the
//! mask check); listings carry whether one is stored.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A live profile.
pub struct ProfileRow {
    pub id: Uuid,
    pub name: String,
    pub sender_name: String,
    pub email: String,
    pub html: Option<String>,
    pub is_default: bool,
    pub smtp: Option<Value>,
    pub has_password: bool,
    pub conditions: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A profile's stored SMTP settings and sealed password.
pub struct StoredSmtp {
    pub smtp: Option<Value>,
    pub sealed: Option<Vec<u8>>,
}

/// Columns of a write; `set_*` false keeps the stored value.
pub struct Write<'a> {
    pub name: &'a str,
    pub sender_name: &'a str,
    pub email: &'a str,
    pub is_default: bool,
    pub set_html: bool,
    pub html: Option<&'a str>,
    pub set_smtp: bool,
    pub smtp: Option<&'a Value>,
    /// Replace the password columns with `sealed` / `key_id` (None clears).
    pub set_password: bool,
    pub sealed: Option<&'a [u8]>,
    pub key_id: Option<&'a str>,
    pub set_conditions: bool,
    pub conditions: Option<&'a Value>,
}

/// New profile `id`, created by `user_id`.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    user_id: Uuid,
    w: &Write<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO email_profiles (id, workspace_id, name, sender_name, email, is_default, html,
                                     smtp, smtp_password, smtp_key_id, conditions, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        id,
        workspace_id,
        w.name,
        w.sender_name,
        w.email,
        w.is_default,
        w.html,
        w.smtp,
        w.sealed,
        w.key_id,
        w.conditions,
        user_id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The stored SMTP of the live profile `id`, its row locked until the transaction ends.
pub async fn lock_smtp(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<StoredSmtp>, sqlx::Error> {
    sqlx::query_as!(
        StoredSmtp,
        r#"SELECT smtp, smtp_password AS sealed FROM email_profiles
           WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR UPDATE"#,
        workspace_id,
        id,
    )
    .fetch_optional(tx)
    .await
}

/// Patch the live profile `id`; false when there is none.
pub async fn update(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    w: &Write<'_>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE email_profiles SET
            name = $3, sender_name = $4, email = $5, is_default = $6,
            html = CASE WHEN $7 THEN $8 ELSE html END,
            smtp = CASE WHEN $9 THEN $10 ELSE smtp END,
            smtp_password = CASE WHEN $11 THEN $12 ELSE smtp_password END,
            smtp_key_id = CASE WHEN $11 THEN $13 ELSE smtp_key_id END,
            conditions = CASE WHEN $14 THEN $15 ELSE conditions END,
            updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
        w.name,
        w.sender_name,
        w.email,
        w.is_default,
        w.set_html,
        w.html,
        w.set_smtp,
        w.smtp,
        w.set_password,
        w.sealed,
        w.key_id,
        w.set_conditions,
        w.conditions,
    )
    .execute(tx)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Soft delete; false when there is no live row. The sealed password goes with it.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE email_profiles SET deleted_at = now(), smtp_password = NULL, smtp_key_id = NULL,
                                   updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The live profile `id`.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<ProfileRow>, sqlx::Error> {
    sqlx::query_as!(
        ProfileRow,
        r#"SELECT id, name, sender_name, email, html, is_default, smtp,
                  smtp_password IS NOT NULL AS "has_password!", conditions, created_at, updated_at
           FROM email_profiles WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL"#,
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// List filters (Laravel ListEmailProfile): a condition matches when equal or unset.
pub struct ListFilter<'a> {
    pub relation_type: Option<&'a str>,
    pub language: Option<&'a str>,
    pub relation_custom_type_id: Option<&'a str>,
}

/// One page of live profiles, oldest first (Laravel's list had no order), plus the total.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ProfileRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        ProfileRow,
        r#"SELECT id, name, sender_name, email, html, is_default, smtp,
                  smtp_password IS NOT NULL AS "has_password!", conditions, created_at, updated_at
           FROM email_profiles
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR conditions->>'relation_type' IS NULL
                  OR conditions->>'relation_type' = $2)
             AND ($3::text IS NULL OR conditions->>'language' IS NULL
                  OR conditions->>'language' = $3)
             AND ($4::text IS NULL OR conditions->>'relation_custom_type_id' IS NULL
                  OR conditions->>'relation_custom_type_id' = $4)
           ORDER BY created_at, id
           LIMIT $5 OFFSET $6"#,
        workspace_id,
        f.relation_type,
        f.language,
        f.relation_custom_type_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM email_profiles
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR conditions->>'relation_type' IS NULL
                  OR conditions->>'relation_type' = $2)
             AND ($3::text IS NULL OR conditions->>'language' IS NULL
                  OR conditions->>'language' = $3)
             AND ($4::text IS NULL OR conditions->>'relation_custom_type_id' IS NULL
                  OR conditions->>'relation_custom_type_id' = $4)"#,
        workspace_id,
        f.relation_type,
        f.language,
        f.relation_custom_type_id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}
