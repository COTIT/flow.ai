//! email_verification_tokens SQL. sha256 of the emailed token only. Rows live until expiry
//! (cleanup job removes them), so a second click or a mail scanner's prefetch still works.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

pub async fn insert(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    token_hash: &[u8; 32],
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO email_verification_tokens (user_id, token_hash, expires_at)
         VALUES ($1, $2, $3)",
        user_id,
        &token_hash[..],
        expires_at,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Owner of a live token, if that user still exists.
pub async fn find_live_user(
    db: impl PgExecutor<'_>,
    token_hash: &[u8; 32],
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT t.user_id FROM email_verification_tokens t
         JOIN users u ON u.id = t.user_id AND u.deleted_at IS NULL
         WHERE t.token_hash = $1 AND t.expires_at > now()",
        &token_hash[..],
    )
    .fetch_optional(db)
    .await
}

/// Cleanup: expired links can't verify; quota looks back only one day (= link lifetime).
pub async fn prune_expired(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM email_verification_tokens WHERE expires_at < now()")
        .execute(db)
        .await
        .map(|done| done.rows_affected())
}

/// Resend quota input for one user. Index user_created_idx.
pub struct IssueStats {
    pub last_hour: i64,
    pub last_day: i64,
    pub newest: Option<DateTime<Utc>>,
}

pub async fn issue_stats(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<IssueStats, sqlx::Error> {
    sqlx::query_as!(
        IssueStats,
        r#"SELECT count(*) FILTER (WHERE created_at > now() - interval '1 hour') AS "last_hour!",
                  count(*) AS "last_day!",
                  max(created_at) AS newest
           FROM email_verification_tokens
           WHERE user_id = $1 AND created_at > now() - interval '1 day'"#,
        user_id,
    )
    .fetch_one(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::users::repo::{self as users, NewUser};
    use chrono::Duration;
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
    async fn find_live_user_repeatable_until_expiry(pool: PgPool) {
        let user_id = user(&pool).await;
        insert(&pool, user_id, &[1; 32], Utc::now() + Duration::days(1))
            .await
            .unwrap();

        assert_eq!(
            find_live_user(&pool, &[1; 32]).await.unwrap(),
            Some(user_id)
        );
        assert_eq!(
            find_live_user(&pool, &[1; 32]).await.unwrap(),
            Some(user_id)
        );
        assert_eq!(find_live_user(&pool, &[2; 32]).await.unwrap(), None);
    }

    #[sqlx::test]
    async fn expired_token_or_deleted_user_not_found(pool: PgPool) {
        let user_id = user(&pool).await;
        insert(&pool, user_id, &[1; 32], Utc::now() - Duration::seconds(1))
            .await
            .unwrap();
        insert(&pool, user_id, &[2; 32], Utc::now() + Duration::days(1))
            .await
            .unwrap();
        sqlx::query("UPDATE users SET deleted_at = now()")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(find_live_user(&pool, &[1; 32]).await.unwrap(), None);
        assert_eq!(find_live_user(&pool, &[2; 32]).await.unwrap(), None);
    }

    #[sqlx::test]
    async fn issue_stats_windows(pool: PgPool) {
        let user_id = user(&pool).await;
        let later = Utc::now() + Duration::days(2);
        for (i, age) in ["30 minutes", "3 hours", "2 days"].iter().enumerate() {
            insert(&pool, user_id, &[i as u8; 32], later).await.unwrap();
            sqlx::query(
                "UPDATE email_verification_tokens SET created_at = now() - $2::interval
                 WHERE token_hash = $1",
            )
            .bind(vec![i as u8; 32])
            .bind(*age)
            .execute(&pool)
            .await
            .unwrap();
        }

        let stats = issue_stats(&pool, user_id).await.unwrap();

        assert_eq!((stats.last_hour, stats.last_day), (1, 2));
        assert!(stats.newest.unwrap() > Utc::now() - Duration::hours(1));
        let none = issue_stats(&pool, Uuid::nil()).await.unwrap();
        assert_eq!((none.last_hour, none.last_day, none.newest), (0, 0, None));
    }
}
