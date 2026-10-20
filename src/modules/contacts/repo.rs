//! SQL for contacts. Every statement takes `workspace_id` and skips soft-deleted rows; a
//! contact is only reached through its relation.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::contacts::dto::{ContactFields, ContactPatch};

/// Live contact.
pub struct ContactRow {
    pub id: Uuid,
    pub relation_id: Uuid,
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phones: Value,
    pub job_title: Option<String>,
    pub initials: Option<String>,
    pub prefix: Option<String>,
    pub attn: Option<String>,
    pub salutation: Option<String>,
    pub default_contact: bool,
    pub default_for: Option<String>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New contact of `relation_id`; returns its id. Phones must already be normalized.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_id: Uuid,
    c: &ContactFields,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO contacts (workspace_id, relation_id, first_name, middle_name, last_name,
                               prefix, initials, job_title, salutation, email, phone_number,
                               phones, default_contact, default_for, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         RETURNING id",
        workspace_id,
        relation_id,
        c.first_name,
        c.middle_name,
        c.last_name,
        c.prefix,
        c.initials,
        c.job_title,
        c.salutation,
        c.email,
        c.phone_number,
        Value::Array(c.phones.clone()),
        c.default_contact,
        c.default_for,
        c.meta,
    )
    .fetch_one(db)
    .await
}

/// Live contacts of these relations, oldest first (Laravel hasMany order). One query per page.
pub async fn for_relations(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_ids: &[Uuid],
) -> Result<Vec<ContactRow>, sqlx::Error> {
    sqlx::query_as!(
        ContactRow,
        "SELECT id, relation_id, first_name, middle_name, last_name, email, phones, job_title,
                initials, prefix, attn, salutation, default_contact, default_for, meta,
                created_at, updated_at
         FROM contacts
         WHERE workspace_id = $1 AND relation_id = ANY($2) AND deleted_at IS NULL
         ORDER BY id",
        workspace_id,
        relation_ids,
    )
    .fetch_all(db)
    .await
}

/// Clear the default flag on the relation's other live contacts (one default per relation).
pub async fn demote_others(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_id: Uuid,
    keep: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE contacts SET default_contact = false, updated_at = now()
         WHERE workspace_id = $1 AND relation_id = $2 AND default_contact
           AND deleted_at IS NULL AND id IS DISTINCT FROM $3",
        workspace_id,
        relation_id,
        keep,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Patch a live contact of `relation_id`; absent fields keep their value. False = no such row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_id: Uuid,
    id: Uuid,
    p: &ContactPatch,
) -> Result<bool, sqlx::Error> {
    let text = |v: &Option<Option<String>>| (v.is_some(), v.clone().flatten());
    let (s_first, first) = text(&p.first_name);
    let (s_middle, middle) = text(&p.middle_name);
    let (s_last, last) = text(&p.last_name);
    let (s_prefix, prefix) = text(&p.prefix);
    let (s_initials, initials) = text(&p.initials);
    let (s_job, job) = text(&p.job_title);
    let (s_salutation, salutation) = text(&p.salutation);
    let (s_email, email) = text(&p.email);
    let (s_phone, phone) = text(&p.phone_number);
    let (s_for, default_for) = text(&p.default_for);
    sqlx::query!(
        "UPDATE contacts
         SET first_name = CASE WHEN $4 THEN $5 ELSE first_name END,
             middle_name = CASE WHEN $6 THEN $7 ELSE middle_name END,
             last_name = CASE WHEN $8 THEN $9 ELSE last_name END,
             prefix = CASE WHEN $10 THEN $11 ELSE prefix END,
             initials = CASE WHEN $12 THEN $13 ELSE initials END,
             job_title = CASE WHEN $14 THEN $15 ELSE job_title END,
             salutation = CASE WHEN $16 THEN $17 ELSE salutation END,
             email = CASE WHEN $18 THEN $19 ELSE email END,
             phone_number = CASE WHEN $20 THEN $21 ELSE phone_number END,
             phones = COALESCE($22, phones),
             default_contact = COALESCE($23, default_contact),
             default_for = CASE WHEN $24 THEN $25 ELSE default_for END,
             updated_at = now()
         WHERE id = $3 AND workspace_id = $1 AND relation_id = $2 AND deleted_at IS NULL",
        workspace_id,
        relation_id,
        id,
        s_first,
        first,
        s_middle,
        middle,
        s_last,
        last,
        s_prefix,
        prefix,
        s_initials,
        initials,
        s_job,
        job,
        s_salutation,
        salutation,
        s_email,
        email,
        s_phone,
        phone,
        p.phones.clone().map(Value::Array),
        p.default_contact,
        s_for,
        default_for,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete a live contact of `relation_id`. False = no such row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE contacts SET deleted_at = now(), updated_at = now()
         WHERE id = $3 AND workspace_id = $1 AND relation_id = $2 AND deleted_at IS NULL",
        workspace_id,
        relation_id,
        id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
