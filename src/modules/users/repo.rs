//! users table SQL. Email match is lowercase both sides, same as users_email_unique.

use sqlx::PgExecutor;
use uuid::Uuid;

/// Insert payload. `password_hash` is argon2id output, never plaintext.
pub struct NewUser<'a> {
    pub email: &'a str,
    pub first_name: &'a str,
    pub middle_name: Option<&'a str>,
    pub last_name: &'a str,
    pub password_hash: &'a str,
}

/// Login lookup result. No Debug derive so hash can't leak into logs.
pub struct Credentials {
    pub id: Uuid,
    pub password_hash: String,
}

/// Insert user, return new id.
pub async fn insert(db: impl PgExecutor<'_>, user: &NewUser<'_>) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO users (email, first_name, middle_name, last_name, password_hash)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
        user.email,
        user.first_name,
        user.middle_name,
        user.last_name,
        user.password_hash,
    )
    .fetch_one(db)
    .await
}

/// Id + hash of live user with this email, any case.
pub async fn find_credentials_by_email(
    db: impl PgExecutor<'_>,
    email: &str,
) -> Result<Option<Credentials>, sqlx::Error> {
    sqlx::query_as!(
        Credentials,
        "SELECT id, password_hash FROM users
         WHERE lower(email) = lower($1) AND deleted_at IS NULL",
        email,
    )
    .fetch_optional(db)
    .await
}

/// True when a live user owns this email, any case.
pub async fn exists_by_email(db: impl PgExecutor<'_>, email: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM users WHERE lower(email) = lower($1) AND deleted_at IS NULL
           ) AS "exists!""#,
        email,
    )
    .fetch_one(db)
    .await
}

/// Email of live user, as stored.
pub async fn find_email(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT email FROM users WHERE id = $1 AND deleted_at IS NULL",
        user_id,
    )
    .fetch_optional(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    fn ada() -> NewUser<'static> {
        NewUser {
            email: "Ada@Example.com",
            first_name: "Ada",
            middle_name: None,
            last_name: "Lovelace",
            password_hash: "argon-hash",
        }
    }

    #[sqlx::test]
    async fn insert_then_find_credentials_ignoring_case(pool: PgPool) {
        let id = insert(&pool, &ada()).await.unwrap();

        let creds = find_credentials_by_email(&pool, "ada@EXAMPLE.com")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(creds.id, id);
        assert_eq!(creds.password_hash, "argon-hash");
    }

    #[sqlx::test]
    async fn find_credentials_unknown_email_is_none(pool: PgPool) {
        assert!(
            find_credentials_by_email(&pool, "nobody@x.io")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn soft_deleted_user_invisible(pool: PgPool) {
        let id = insert(&pool, &ada()).await.unwrap();
        sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            find_credentials_by_email(&pool, "ada@example.com")
                .await
                .unwrap()
                .is_none()
        );
        assert!(!exists_by_email(&pool, "ada@example.com").await.unwrap());
    }

    #[sqlx::test]
    async fn find_email_live_users_only(pool: PgPool) {
        let id = insert(&pool, &ada()).await.unwrap();
        assert_eq!(
            find_email(&pool, id).await.unwrap().as_deref(),
            Some("Ada@Example.com")
        );

        sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(find_email(&pool, id).await.unwrap(), None);
        assert_eq!(find_email(&pool, Uuid::nil()).await.unwrap(), None);
    }

    #[sqlx::test]
    async fn exists_by_email_ignores_case(pool: PgPool) {
        insert(&pool, &ada()).await.unwrap();

        assert!(exists_by_email(&pool, "ADA@example.com").await.unwrap());
        assert!(!exists_by_email(&pool, "other@example.com").await.unwrap());
    }

    #[sqlx::test]
    async fn insert_keeps_middle_name(pool: PgPool) {
        let id = insert(
            &pool,
            &NewUser {
                middle_name: Some("King"),
                ..ada()
            },
        )
        .await
        .unwrap();

        let middle: Option<String> =
            sqlx::query_scalar("SELECT middle_name FROM users WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(middle.as_deref(), Some("King"));
    }
}
