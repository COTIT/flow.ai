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

/// Email + verification state of a live user.
pub struct Contact {
    pub email: Option<String>,
    pub email_verified: bool,
}

/// Contact of live user, email as stored.
pub async fn find_contact(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<Option<Contact>, sqlx::Error> {
    sqlx::query_as!(
        Contact,
        r#"SELECT email, email_verified_at IS NOT NULL AS "email_verified!"
           FROM users WHERE id = $1 AND deleted_at IS NULL"#,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Set email_verified_at once; later calls keep the first timestamp.
pub async fn mark_email_verified(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET email_verified_at = now(), updated_at = now()
         WHERE id = $1 AND email_verified_at IS NULL",
        user_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// `find_contact` + row lock until the transaction ends (serializes per-user quota checks).
pub async fn lock_contact(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<Option<Contact>, sqlx::Error> {
    sqlx::query_as!(
        Contact,
        r#"SELECT email, email_verified_at IS NOT NULL AS "email_verified!"
           FROM users WHERE id = $1 AND deleted_at IS NULL
           FOR UPDATE"#,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Fields /v1/me shows about a live user.
pub struct Profile {
    pub id: Uuid,
    pub email: Option<String>,
    pub first_name: String,
    pub middle_name: Option<String>,
    pub last_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub email_verified: bool,
}

pub async fn find_profile(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<Option<Profile>, sqlx::Error> {
    sqlx::query_as!(
        Profile,
        r#"SELECT id, email, first_name, middle_name, last_name, created_at, updated_at,
                  email_verified_at IS NOT NULL AS "email_verified!"
           FROM users WHERE id = $1 AND deleted_at IS NULL"#,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Live user with their membership in one workspace.
pub struct Member {
    pub id: Uuid,
    pub email: Option<String>,
    pub first_name: String,
    pub middle_name: Option<String>,
    pub last_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Membership flag, not the account's.
    pub active: bool,
    /// Created the workspace.
    pub owner: bool,
}

/// Users-tab filter. `pattern` = escaped ILIKE pattern over names and email.
pub struct MemberFilter<'a> {
    pub pattern: Option<&'a str>,
    pub team_id: Option<Uuid>,
    /// PIN login not ported: nobody has one, so `true` matches no one.
    pub has_pin_code: bool,
    pub limit: i64,
    pub offset: i64,
}

/// One page of `workspace_id`'s members (active or not), oldest membership first, + total.
pub async fn workspace_members(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &MemberFilter<'_>,
) -> Result<(Vec<Member>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT u.id, u.email, u.first_name, u.middle_name, u.last_name, u.created_at,
                  u.updated_at, m.active, w.created_by_user_id = u.id AS "owner!",
                  count(*) OVER () AS "total!"
           FROM workspace_members m
           JOIN users u ON u.id = m.user_id AND u.deleted_at IS NULL
           JOIN workspaces w ON w.id = m.workspace_id
           WHERE m.workspace_id = $1 AND NOT $4
             AND ($2::text IS NULL OR u.email ILIKE $2
                  OR concat_ws(' ', u.first_name, u.middle_name, u.last_name) ILIKE $2
                  OR concat_ws(' ', u.first_name, u.last_name) ILIKE $2)
             AND ($3::uuid IS NULL OR EXISTS (
                   SELECT 1 FROM team_members tm
                   JOIN teams t ON t.id = tm.team_id AND t.deleted_at IS NULL
                   WHERE tm.user_id = u.id AND t.id = $3 AND t.workspace_id = $1))
           ORDER BY m.created_at, u.id
           LIMIT $5 OFFSET $6"#,
        workspace_id,
        f.pattern,
        f.team_id,
        f.has_pin_code,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    // Empty page past the end has no row to carry the count: caller asks `count_members`.
    let total = rows.first().map_or(0, |r| r.total);
    let members = rows
        .into_iter()
        .map(|r| Member {
            id: r.id,
            email: r.email,
            first_name: r.first_name,
            middle_name: r.middle_name,
            last_name: r.last_name,
            created_at: r.created_at,
            updated_at: r.updated_at,
            active: r.active,
            owner: r.owner,
        })
        .collect();
    Ok((members, total))
}

/// Members matching `f`, ignoring paging. Same WHERE as `workspace_members`.
pub async fn count_members(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &MemberFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!"
           FROM workspace_members m
           JOIN users u ON u.id = m.user_id AND u.deleted_at IS NULL
           WHERE m.workspace_id = $1 AND NOT $4
             AND ($2::text IS NULL OR u.email ILIKE $2
                  OR concat_ws(' ', u.first_name, u.middle_name, u.last_name) ILIKE $2
                  OR concat_ws(' ', u.first_name, u.last_name) ILIKE $2)
             AND ($3::uuid IS NULL OR EXISTS (
                   SELECT 1 FROM team_members tm
                   JOIN teams t ON t.id = tm.team_id AND t.deleted_at IS NULL
                   WHERE tm.user_id = u.id AND t.id = $3 AND t.workspace_id = $1))"#,
        workspace_id,
        f.pattern,
        f.team_id,
        f.has_pin_code,
    )
    .fetch_one(db)
    .await
}

/// One member of `workspace_id`. None = not a member, unknown or deleted user.
pub async fn find_member(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Member>, sqlx::Error> {
    sqlx::query_as!(
        Member,
        r#"SELECT u.id, u.email, u.first_name, u.middle_name, u.last_name, u.created_at,
                  u.updated_at, m.active, w.created_by_user_id = u.id AS "owner!"
           FROM workspace_members m
           JOIN users u ON u.id = m.user_id AND u.deleted_at IS NULL
           JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
           WHERE m.workspace_id = $1 AND m.user_id = $2"#,
        workspace_id,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Live user as other resources embed them (source of Laravel UserEmailResource).
pub struct UserRef {
    pub id: Uuid,
    pub first_name: String,
    pub middle_name: Option<String>,
    pub last_name: String,
    pub email: Option<String>,
}

/// Live users among `ids` with a membership (active or not) in `workspace_id`. One query
/// serves both the "is this id a member" check and a page's embedded users.
pub async fn members_among(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<UserRef>, sqlx::Error> {
    sqlx::query_as!(
        UserRef,
        "SELECT u.id, u.first_name, u.middle_name, u.last_name, u.email
         FROM workspace_members m
         JOIN users u ON u.id = m.user_id AND u.deleted_at IS NULL
         WHERE m.workspace_id = $1 AND m.user_id = ANY($2)",
        workspace_id,
        ids,
    )
    .fetch_all(db)
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
    async fn find_contact_live_users_only(pool: PgPool) {
        let id = insert(&pool, &ada()).await.unwrap();
        let contact = find_contact(&pool, id).await.unwrap().unwrap();
        assert_eq!(contact.email.as_deref(), Some("Ada@Example.com"));
        assert!(!contact.email_verified);

        sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(find_contact(&pool, id).await.unwrap().is_none());
        assert!(find_contact(&pool, Uuid::nil()).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn mark_email_verified_keeps_first_timestamp(pool: PgPool) {
        let id = insert(&pool, &ada()).await.unwrap();
        assert!(
            mark_email_verified(&pool, id).await.unwrap(),
            "first time reports true"
        );
        let first: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT email_verified_at FROM users WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert!(
            !mark_email_verified(&pool, id).await.unwrap(),
            "repeat reports false"
        );

        let again: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT email_verified_at FROM users WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(first, again);
        assert!(
            find_contact(&pool, id)
                .await
                .unwrap()
                .unwrap()
                .email_verified
        );
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
