//! Till SQL: registers, till users, PIN failure counters and sessions. Every query filters
//! `workspace_id`; sessions are found by token sha256 through the unique index.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// A register as listed; `key_prefix` of its live device key.
pub struct RegisterRow {
    pub id: Uuid,
    pub name: String,
    pub warehouse_id: Uuid,
    pub currency_id: Uuid,
    pub key_prefix: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A till user as listed (never the PIN hash).
pub struct UserRow {
    pub id: Uuid,
    pub name: String,
    pub user_id: Option<Uuid>,
    pub active: bool,
    pub locked: bool,
    pub created_at: DateTime<Utc>,
}

/// The register a device key logs in on.
pub struct LoginRegister {
    pub id: Uuid,
    pub locked: bool,
}

/// A till user as login sees it.
pub struct LoginUser {
    pub id: Uuid,
    pub pin_hash: String,
    pub active: bool,
    pub locked: bool,
}

/// A live session with its register and user.
pub struct SessionRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub register_id: Uuid,
    pub register_name: String,
    pub warehouse_id: Uuid,
    pub currency_id: Uuid,
    pub till_user_id: Uuid,
    pub user_name: String,
    pub expires_at: DateTime<Utc>,
}

/// A live store warehouse of the workspace exists.
pub async fn store_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM warehouses WHERE workspace_id = $1 AND id = $2
                          AND deleted_at IS NULL AND is_store IS TRUE) AS "e!""#,
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}

/// Store a register bound to `app_key_id`; its row.
pub async fn insert_register(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    name: &str,
    warehouse_id: Uuid,
    currency_id: Uuid,
    app_key_id: Uuid,
    created_by: Uuid,
) -> Result<(Uuid, DateTime<Utc>), sqlx::Error> {
    let row = sqlx::query!(
        "INSERT INTO till_registers (workspace_id, name, warehouse_id, currency_id, app_key_id,
                                     created_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id, created_at",
        workspace_id,
        name,
        warehouse_id,
        currency_id,
        app_key_id,
        created_by,
    )
    .fetch_one(db)
    .await?;
    Ok((row.id, row.created_at))
}

/// One page of registers, newest first, and the total.
pub async fn list_registers(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<RegisterRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT r.id, r.name, r.warehouse_id, r.currency_id, r.created_at,
                  a.prefix AS "key_prefix?", count(*) OVER () AS "total!"
           FROM till_registers r
           LEFT JOIN app_keys a ON a.id = r.app_key_id AND a.revoked_at IS NULL
           WHERE r.workspace_id = $1
           ORDER BY r.created_at DESC, r.id DESC
           LIMIT $2 OFFSET $3"#,
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| RegisterRow {
            id: r.id,
            name: r.name,
            warehouse_id: r.warehouse_id,
            currency_id: r.currency_id,
            key_prefix: r.key_prefix,
            created_at: r.created_at,
        })
        .collect();
    Ok((rows, total))
}

/// The register's name and device key id under a row lock; None = no such register.
pub async fn lock_register_key(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<(String, Option<Uuid>)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT name, app_key_id FROM till_registers WHERE workspace_id = $1 AND id = $2
         FOR UPDATE",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.name, r.app_key_id)))
}

/// Point the register at a new device key.
pub async fn set_register_key(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    app_key_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE till_registers SET app_key_id = $3, failed_pins = 0, failed_since = NULL,
                                   locked_until = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        app_key_id,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Store a till user; its row.
pub async fn insert_user(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    name: &str,
    user_id: Option<Uuid>,
    pin_hash: &str,
    created_by: Uuid,
) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        r#"INSERT INTO till_users (workspace_id, name, user_id, pin_hash, created_by)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, name, user_id, active, false AS "locked!", created_at"#,
        workspace_id,
        name,
        user_id,
        pin_hash,
        created_by,
    )
    .fetch_one(db)
    .await
}

/// Every till user of the workspace by name (a shop has a handful).
pub async fn list_users(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<UserRow>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        r#"SELECT id, name, user_id, active, coalesce(locked_until > now(), false) AS "locked!",
                  created_at
           FROM till_users WHERE workspace_id = $1 ORDER BY name, id
           LIMIT 1000"#,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// Apply a patch (None keeps a field); the row, None when not in the workspace. A new PIN or
/// `active = false` ends the user's live sessions.
pub async fn update_user(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    name: Option<&str>,
    pin_hash: Option<&str>,
    active: Option<bool>,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        r#"WITH u AS (
               UPDATE till_users SET name = coalesce($3, name), pin_hash = coalesce($4, pin_hash),
                      active = coalesce($5, active),
                      failed_pins = CASE WHEN $4::text IS NULL THEN failed_pins ELSE 0 END,
                      locked_until = CASE WHEN $4::text IS NULL THEN locked_until END,
                      updated_at = now()
               WHERE workspace_id = $1 AND id = $2
               RETURNING id, name, user_id, active, locked_until, created_at
           ), ended AS (
               UPDATE till_sessions SET ended_at = now()
               WHERE workspace_id = $1 AND till_user_id = (SELECT id FROM u) AND ended_at IS NULL
                 AND ($4::text IS NOT NULL OR $5::bool IS FALSE)
           )
           SELECT id AS "id!", name AS "name!", user_id, active AS "active!",
                  coalesce(locked_until > now(), false) AS "locked!", created_at AS "created_at!"
           FROM u"#,
        workspace_id,
        id,
        name,
        pin_hash,
        active,
    )
    .fetch_optional(db)
    .await
}

/// The register a live device key of the workspace is bound to.
pub async fn register_by_key(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    app_key_id: Uuid,
) -> Result<Option<LoginRegister>, sqlx::Error> {
    sqlx::query_as!(
        LoginRegister,
        r#"SELECT id, coalesce(locked_until > now(), false) AS "locked!"
           FROM till_registers WHERE workspace_id = $1 AND app_key_id = $2"#,
        workspace_id,
        app_key_id,
    )
    .fetch_optional(db)
    .await
}

/// Active till users for the login picker.
pub async fn login_users(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, name FROM till_users WHERE workspace_id = $1 AND active
         ORDER BY name, id LIMIT 1000",
        workspace_id,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.name)).collect())
}

/// A till user for a PIN check.
pub async fn login_user(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<LoginUser>, sqlx::Error> {
    sqlx::query_as!(
        LoginUser,
        r#"SELECT id, pin_hash, active, coalesce(locked_until > now(), false) AS "locked!"
           FROM till_users WHERE workspace_id = $1 AND id = $2"#,
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Count a wrong PIN for the user: every 5th since the last right one locks them, 15 minutes
/// the first time, twice as long each time after, 24 hours at most.
pub async fn user_failed(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE till_users
         SET failed_pins = least(failed_pins + 1, 10000),
             locked_until = CASE WHEN (failed_pins + 1) % 5 = 0
                 THEN now() + least(interval '15 minutes'
                                    * power(2, least((failed_pins + 1) / 5 - 1, 7)),
                                    interval '24 hours')
                 ELSE locked_until END
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Count a wrong PIN on the register: the 20th within an hour locks its login 15 minutes.
/// One statement on the row, so parallel failures count exactly.
pub async fn register_failed(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE till_registers
         SET failed_pins = CASE
                 WHEN failed_since IS NULL OR failed_since < now() - interval '1 hour' THEN 1
                 WHEN failed_pins >= 19 THEN 0
                 ELSE failed_pins + 1 END,
             failed_since = CASE
                 WHEN failed_since IS NULL OR failed_since < now() - interval '1 hour' THEN now()
                 WHEN failed_pins >= 19 THEN NULL
                 ELSE failed_since END,
             locked_until = CASE
                 WHEN failed_since >= now() - interval '1 hour' AND failed_pins >= 19
                 THEN now() + interval '15 minutes'
                 ELSE locked_until END
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// A right PIN: the user's count back to 0, unless the user or the register got locked
/// meanwhile (a guess in flight when the lock landed); false then.
pub async fn login_ok(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    register_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let ok = sqlx::query_scalar!(
        r#"UPDATE till_users SET failed_pins = 0
           WHERE workspace_id = $1 AND id = $3
             AND (locked_until IS NULL OR locked_until <= now())
             AND EXISTS (SELECT 1 FROM till_registers
                         WHERE workspace_id = $1 AND id = $2
                           AND (locked_until IS NULL OR locked_until <= now()))
           RETURNING true AS "ok!""#,
        workspace_id,
        register_id,
        user_id,
    )
    .fetch_optional(db)
    .await?;
    Ok(ok.is_some())
}

/// Open a session for 8 hours, ending the user's older ones on this register; its expiry.
pub async fn insert_session(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    register_id: Uuid,
    till_user_id: Uuid,
    app_key_id: Uuid,
    token_hash: &[u8; 32],
) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar!(
        "WITH old AS (
             UPDATE till_sessions SET ended_at = now()
             WHERE workspace_id = $1 AND till_user_id = $3 AND register_id = $2
               AND ended_at IS NULL
         )
         INSERT INTO till_sessions (workspace_id, register_id, till_user_id, app_key_id,
                                    token_hash, created_at, expires_at)
         VALUES ($1, $2, $3, $4, $5, now(), now() + interval '8 hours')
         RETURNING expires_at",
        workspace_id,
        register_id,
        till_user_id,
        app_key_id,
        &token_hash[..],
    )
    .fetch_one(db)
    .await
}

/// The live session for this token hash: not ended or expired, its device key still the
/// register's and not revoked, its user active, its workspace not deleted.
pub async fn live_session(
    db: impl PgExecutor<'_>,
    token_hash: &[u8; 32],
) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as!(
        SessionRow,
        "SELECT s.id, s.workspace_id, s.register_id, r.name AS register_name, r.warehouse_id,
                r.currency_id, s.till_user_id, u.name AS user_name, s.expires_at
         FROM till_sessions s
         JOIN till_registers r ON r.workspace_id = s.workspace_id AND r.id = s.register_id
                              AND r.app_key_id = s.app_key_id
         JOIN app_keys a ON a.id = s.app_key_id AND a.revoked_at IS NULL
         JOIN till_users u ON u.workspace_id = s.workspace_id AND u.id = s.till_user_id
                          AND u.active
         JOIN workspaces w ON w.id = s.workspace_id AND w.deleted_at IS NULL
         WHERE s.token_hash = $1 AND s.ended_at IS NULL AND s.expires_at > now()",
        &token_hash[..],
    )
    .fetch_optional(db)
    .await
}

/// End a session (logout).
pub async fn end_session(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE till_sessions SET ended_at = now()
         WHERE workspace_id = $1 AND id = $2 AND ended_at IS NULL",
        workspace_id,
        id,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Drop sessions ended or expired over a day ago; how many.
pub async fn prune_sessions(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "DELETE FROM till_sessions
         WHERE expires_at < now() - interval '1 day' OR ended_at < now() - interval '1 day'"
    )
    .execute(db)
    .await?
    .rows_affected())
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    /// A workspace with a register and a till user: `(workspace, register, user)`.
    async fn fixture(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
        sqlx::query_as(
            "WITH o AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ('ada@example.com', 'Ada', 'L', 'x') RETURNING id),
                  w AS (INSERT INTO workspaces (name, created_by_user_id)
                        SELECT 'acme', id FROM o RETURNING id),
                  c AS (INSERT INTO currencies (workspace_id, name, asset_type)
                        SELECT id, 'Euro', 'EUR' FROM w RETURNING id, workspace_id),
                  h AS (INSERT INTO warehouses (workspace_id, name, slug, is_store)
                        SELECT id, 'Shop', 'shop', true FROM w RETURNING id),
                  r AS (INSERT INTO till_registers (workspace_id, name, warehouse_id, currency_id)
                        SELECT c.workspace_id, 'Till 1', h.id, c.id FROM c, h
                        RETURNING id, workspace_id),
                  u AS (INSERT INTO till_users (workspace_id, name, pin_hash)
                        SELECT id, 'Mary', 'x' FROM w RETURNING id)
             SELECT r.workspace_id, r.id, u.id FROM r, u",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// A right PIN verified while a lock landed (user or register) must not log in.
    #[sqlx::test]
    async fn login_ok_refuses_when_a_lock_landed_meanwhile(pool: PgPool) {
        let (ws, reg, user) = fixture(&pool).await;
        assert!(login_ok(&pool, ws, reg, user).await.unwrap());
        for (lock, unlock) in [
            (
                "UPDATE till_users SET locked_until = now() + interval '1 minute'",
                "UPDATE till_users SET locked_until = NULL",
            ),
            (
                "UPDATE till_registers SET locked_until = now() + interval '1 minute'",
                "UPDATE till_registers SET locked_until = NULL",
            ),
        ] {
            sqlx::query(lock).execute(&pool).await.unwrap();
            assert!(!login_ok(&pool, ws, reg, user).await.unwrap(), "{lock}");
            sqlx::query(unlock).execute(&pool).await.unwrap();
        }
        assert!(login_ok(&pool, ws, reg, user).await.unwrap());
    }
}
