//! access_tokens SQL. Lookup by sha256 hash; expired tokens and deleted users never match.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// Token row matched by bearer hash.
pub struct ActiveToken {
    pub id: Uuid,
    pub user_id: Uuid,
    /// Account signs in by phone and has no email (a farmer).
    pub phone_only: bool,
}

/// Store token hash, return token id. Name fixed to Laravel's `auth_token`.
pub async fn insert(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    token_hash: &[u8; 32],
    expires_at: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO access_tokens (user_id, name, token_hash, expires_at)
         VALUES ($1, 'auth_token', $2, $3)
         RETURNING id",
        user_id,
        &token_hash[..],
        expires_at,
    )
    .fetch_one(db)
    .await
}

/// Live token for this hash: not expired, owner not soft-deleted. One unique-index lookup.
// shortcut: last_used_at never written (saves a write per request, Laravel skipped it too), add if audits need it.
pub async fn find_active_by_hash(
    db: impl PgExecutor<'_>,
    token_hash: &[u8; 32],
) -> Result<Option<ActiveToken>, sqlx::Error> {
    sqlx::query_as!(
        ActiveToken,
        r#"SELECT t.id, t.user_id, u.email IS NULL AS "phone_only!" FROM access_tokens t
           JOIN users u ON u.id = t.user_id AND u.deleted_at IS NULL
           WHERE t.token_hash = $1 AND t.expires_at > now()"#,
        &token_hash[..],
    )
    .fetch_optional(db)
    .await
}

/// Revoke one token (logout). Other sessions of same user stay live.
pub async fn delete(db: impl PgExecutor<'_>, token_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM access_tokens WHERE id = $1", token_id)
        .execute(db)
        .await
        .map(|_| ())
}

/// Login attempts for this email (any case) in the last 15 minutes, the current one included.
/// Rows of successful logins are cleared, so this counts failures plus in-flight attempts.
/// Index login_failures_email_failed_idx.
pub async fn recent_login_attempts(
    db: impl PgExecutor<'_>,
    email: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM login_failures
           WHERE email_norm = lower($1) AND failed_at > now() - interval '15 minutes'"#,
        email,
    )
    .fetch_one(db)
    .await
}

pub async fn record_login_attempt(db: impl PgExecutor<'_>, email: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO login_failures (email_norm) VALUES (lower($1))",
        email
    )
    .execute(db)
    .await
    .map(|_| ())
}

pub async fn clear_login_attempts(db: impl PgExecutor<'_>, email: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM login_failures WHERE email_norm = lower($1)",
        email
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Cleanup: expired tokens never authenticate again.
pub async fn prune_expired_tokens(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM access_tokens WHERE expires_at < now()")
        .execute(db)
        .await
        .map(|done| done.rows_affected())
}

/// Cleanup: the login lock reads 15 minutes back; keep a day for incident review.
pub async fn prune_old_login_attempts(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM login_failures WHERE failed_at < now() - interval '1 day'")
        .execute(db)
        .await
        .map(|done| done.rows_affected())
}

/// Revoke every session of a user (first email verification, pre-hijack mitigation).
pub async fn delete_all_for_user(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM access_tokens WHERE user_id = $1", user_id)
        .execute(db)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::users::repo::{self as users, NewUser};
    use chrono::{Duration, Utc};
    use sqlx::PgPool;

    async fn user(pool: &PgPool) -> Uuid {
        users::insert(
            pool,
            &NewUser {
                email: "u@x.io",
                first_name: "U",
                middle_name: None,
                last_name: "S",
                password_hash: "h",
            },
        )
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn insert_then_find_active(pool: PgPool) {
        let user_id = user(&pool).await;

        let id = insert(&pool, user_id, &[1; 32], Utc::now() + Duration::days(1))
            .await
            .unwrap();
        let found = find_active_by_hash(&pool, &[1; 32]).await.unwrap().unwrap();

        assert_eq!(found.id, id);
        assert_eq!(found.user_id, user_id);
    }

    #[sqlx::test]
    async fn expired_token_not_found(pool: PgPool) {
        let user_id = user(&pool).await;
        insert(&pool, user_id, &[1; 32], Utc::now() - Duration::seconds(1))
            .await
            .unwrap();

        assert!(
            find_active_by_hash(&pool, &[1; 32])
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn deleted_user_token_not_found(pool: PgPool) {
        let user_id = user(&pool).await;
        insert(&pool, user_id, &[1; 32], Utc::now() + Duration::days(1))
            .await
            .unwrap();
        sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            find_active_by_hash(&pool, &[1; 32])
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn delete_revokes_only_that_token(pool: PgPool) {
        let user_id = user(&pool).await;
        let later = Utc::now() + Duration::days(1);
        let first = insert(&pool, user_id, &[1; 32], later).await.unwrap();
        insert(&pool, user_id, &[2; 32], later).await.unwrap();

        delete(&pool, first).await.unwrap();

        assert!(
            find_active_by_hash(&pool, &[1; 32])
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            find_active_by_hash(&pool, &[2; 32])
                .await
                .unwrap()
                .is_some()
        );
    }
}
