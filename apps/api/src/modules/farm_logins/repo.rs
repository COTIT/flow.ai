//! Farmer login SQL: the person a login is issued for, phone accounts, the issue history.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// A live person in the caller's staff scope, as a login needs them.
pub struct LoginPerson {
    pub id: Uuid,
    pub village_id: Uuid,
    pub given_name: String,
    pub family_name: String,
    pub phone: Option<String>,
    pub user_id: Option<Uuid>,
}

/// Lock the person (one issue at a time per person) if live and in the staff scope.
pub async fn lock_person(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    all: bool,
    villages: &[Uuid],
    id: Uuid,
) -> Result<Option<LoginPerson>, sqlx::Error> {
    sqlx::query_as!(
        LoginPerson,
        "SELECT id, village_id, given_name, family_name, phone, user_id FROM farm_persons
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL AND ($3 OR village_id = ANY($4))
         FOR UPDATE",
        ws,
        id,
        all,
        villages,
    )
    .fetch_optional(db)
    .await
}

/// True when the account signs in by email or holds a live staff role anywhere: its password
/// is not an officer's to reset.
pub async fn is_staff_account(db: impl PgExecutor<'_>, user_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT (u.email IS NOT NULL OR EXISTS (
                    SELECT 1 FROM farm_memberships m
                    WHERE m.user_id = u.id AND m.revoked_at IS NULL AND m.role <> 'farmer')) AS "staff!"
           FROM users u WHERE u.id = $1"#,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// The live account signing in with this phone, if any.
pub async fn user_by_phone(
    db: impl PgExecutor<'_>,
    phone: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM users WHERE phone = $1 AND deleted_at IS NULL",
        phone
    )
    .fetch_optional(db)
    .await
}

/// New phone account that must change its password at first sign-in.
pub async fn insert_user(
    db: impl PgExecutor<'_>,
    phone: &str,
    first_name: &str,
    last_name: &str,
    password_hash: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO users (phone, first_name, last_name, password_hash, must_change_password)
         VALUES ($1, $2, $3, $4, true) RETURNING id",
        phone,
        first_name,
        last_name,
        password_hash,
    )
    .fetch_one(db)
    .await
}

/// New temporary password on an existing phone account; the phone follows the person record.
pub async fn reset_user(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    phone: &str,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET phone = $2, password_hash = $3, must_change_password = true, updated_at = now()
         WHERE id = $1",
        user_id,
        phone,
        password_hash,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Link the person to its account.
pub async fn link_person(
    db: impl PgExecutor<'_>,
    person: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_persons SET user_id = $2, updated_at = now() WHERE id = $1",
        person,
        user_id
    )
    .execute(db)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_issue(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    user_id: Uuid,
    person: Uuid,
    issued_by: Uuid,
    kind: &str,
    temp_password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_login_issues (workspace_id, user_id, person_id, issued_by, kind, temp_password_hash)
         VALUES ($1, $2, $3, $4, $5, $6)",
        ws,
        user_id,
        person,
        issued_by,
        kind,
        temp_password_hash,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// One issue in a person's history.
pub struct IssueRow {
    pub issued_at: DateTime<Utc>,
    pub kind: String,
    pub issued_by: Uuid,
    pub issued_by_name: String,
    pub must_change_password: bool,
}

/// A person's issues, newest first, if the person is live and in the staff scope.
pub async fn history(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    all: bool,
    villages: &[Uuid],
    person: Uuid,
) -> Result<Vec<IssueRow>, sqlx::Error> {
    sqlx::query_as!(
        IssueRow,
        r#"SELECT li.issued_at, li.kind, li.issued_by,
                  concat_ws(' ', iu.first_name, iu.last_name) AS "issued_by_name!",
                  u.must_change_password
           FROM farm_persons p
           JOIN farm_login_issues li ON li.person_id = p.id AND li.workspace_id = p.workspace_id
           JOIN users u ON u.id = li.user_id
           JOIN users iu ON iu.id = li.issued_by
           WHERE p.workspace_id = $1 AND p.id = $2 AND p.deleted_at IS NULL
             AND ($3 OR p.village_id = ANY($4))
           ORDER BY li.issued_at DESC, li.id DESC
           LIMIT 100"#,
        ws,
        person,
        all,
        villages,
    )
    .fetch_all(db)
    .await
}

/// What sign-in and password change need about an account.
pub struct Credentials {
    pub id: Uuid,
    pub password_hash: String,
    pub phone: Option<String>,
    pub has_email: bool,
    pub must_change_password: bool,
}

/// The live phone account for this phone.
pub async fn credentials_by_phone(
    db: impl PgExecutor<'_>,
    phone: &str,
) -> Result<Option<Credentials>, sqlx::Error> {
    sqlx::query_as!(
        Credentials,
        r#"SELECT id, password_hash, phone, email IS NOT NULL AS "has_email!", must_change_password
           FROM users WHERE phone = $1 AND deleted_at IS NULL"#,
        phone,
    )
    .fetch_optional(db)
    .await
}

pub async fn credentials_by_id(
    db: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Option<Credentials>, sqlx::Error> {
    sqlx::query_as!(
        Credentials,
        r#"SELECT id, password_hash, phone, email IS NOT NULL AS "has_email!", must_change_password
           FROM users WHERE id = $1 AND deleted_at IS NULL"#,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Hash of the latest temporary password issued to this account.
pub async fn latest_temp_hash(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT temp_password_hash FROM farm_login_issues WHERE user_id = $1
         ORDER BY issued_at DESC, id DESC LIMIT 1",
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// The farmer's own password: flag cleared.
pub async fn set_password(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET password_hash = $2, must_change_password = false, updated_at = now() WHERE id = $1",
        user_id,
        password_hash,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The account's live farm roles in live workspaces, as `[{workspace_id, role, village_id}]`.
pub async fn memberships(db: impl PgExecutor<'_>, user_id: Uuid) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT coalesce(jsonb_agg(jsonb_build_object('workspace_id', m.workspace_id, 'role', m.role,
                                                        'village_id', m.village_id)
                                     ORDER BY m.created_at), '[]') AS "m!"
           FROM farm_memberships m
           JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
           WHERE m.user_id = $1 AND m.revoked_at IS NULL"#,
        user_id,
    )
    .fetch_one(db)
    .await
}
