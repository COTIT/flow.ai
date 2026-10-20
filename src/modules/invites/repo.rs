//! invites table SQL. Rows found by sha256(code) only; plaintext code never reaches DB.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// Insert/rotate payload. `token_hash` = sha256 of emailed code.
pub struct NewInvite<'a> {
    pub workspace_id: Uuid,
    pub invited_by_user_id: Uuid,
    pub email: &'a str,
    pub token_hash: &'a [u8; 32],
    pub expires_at: DateTime<Utc>,
}

/// Invite plus workspace name, enough for status page and accept checks.
#[derive(Debug)]
pub struct InviteRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub workspace_name: String,
    pub email: String,
    /// `pending` | `accepted` | `declined`. Expiry is read from `expires_at`.
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Create pending invite, or rotate code + expiry on existing pending one for same email.
/// Rotation kills old link, so only newest email works.
pub async fn upsert_pending(
    db: impl PgExecutor<'_>,
    invite: &NewInvite<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO invites (workspace_id, invited_by_user_id, email, token_hash, expires_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (workspace_id, lower(email)) WHERE status = 'pending'
         DO UPDATE SET token_hash = EXCLUDED.token_hash,
                       expires_at = EXCLUDED.expires_at,
                       invited_by_user_id = EXCLUDED.invited_by_user_id,
                       email = EXCLUDED.email,
                       updated_at = now()
         RETURNING id",
        invite.workspace_id,
        invite.invited_by_user_id,
        invite.email,
        &invite.token_hash[..],
        invite.expires_at,
    )
    .fetch_one(db)
    .await
}

/// Invite for this code hash, any status. Caller decides on expired/answered.
pub async fn find_by_token_hash(
    db: impl PgExecutor<'_>,
    token_hash: &[u8; 32],
) -> Result<Option<InviteRow>, sqlx::Error> {
    sqlx::query_as!(
        InviteRow,
        "SELECT i.id, i.workspace_id, w.name AS workspace_name, i.email, i.status,
                i.expires_at, i.created_at
         FROM invites i
         JOIN workspaces w ON w.id = i.workspace_id AND w.deleted_at IS NULL
         WHERE i.token_hash = $1",
        &token_hash[..],
    )
    .fetch_optional(db)
    .await
}

/// Set accepted/declined. Only flips a pending row still holding this code hash:
/// false = already answered, or code rotated by a re-invite since it was looked up.
pub async fn mark_answered(
    db: impl PgExecutor<'_>,
    invite_id: Uuid,
    token_hash: &[u8; 32],
    accepted: bool,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE invites
         SET status = CASE WHEN $3 THEN 'accepted' ELSE 'declined' END,
             responded_at = now(),
             updated_at = now()
         WHERE id = $1 AND token_hash = $2 AND status = 'pending'",
        invite_id,
        &token_hash[..],
        accepted,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Invite emails this user sent in the last hour, re-sends included (quota input).
/// Uses index invite_sends_inviter_sent_idx.
pub async fn sends_in_last_hour(
    db: impl PgExecutor<'_>,
    inviter: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM invite_sends
           WHERE invited_by_user_id = $1 AND sent_at > now() - interval '1 hour'"#,
        inviter,
    )
    .fetch_one(db)
    .await
}

/// Log one send for the quota.
pub async fn record_send(db: impl PgExecutor<'_>, inviter: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO invite_sends (invited_by_user_id) VALUES ($1)",
        inviter
    )
    .execute(db)
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{
        users::repo::{self as users, NewUser},
        workspaces::repo as workspaces,
    };
    use chrono::{Duration, Utc};
    use sqlx::PgPool;

    struct Fixture {
        ws: Uuid,
        owner: Uuid,
    }

    async fn fixture(pool: &PgPool) -> Fixture {
        let owner = users::insert(
            pool,
            &NewUser {
                email: "owner@x.io",
                first_name: "O",
                middle_name: None,
                last_name: "W",
                password_hash: "h",
            },
        )
        .await
        .unwrap();
        let ws = workspaces::insert(pool, "acme", "retail", owner)
            .await
            .unwrap();
        Fixture { ws, owner }
    }

    fn invite<'a>(f: &Fixture, email: &'a str, hash: &'a [u8; 32]) -> NewInvite<'a> {
        NewInvite {
            workspace_id: f.ws,
            invited_by_user_id: f.owner,
            email,
            token_hash: hash,
            expires_at: Utc::now() + Duration::days(7),
        }
    }

    #[sqlx::test]
    async fn upsert_then_find_by_hash_with_workspace_name(pool: PgPool) {
        let f = fixture(&pool).await;

        let id = upsert_pending(&pool, &invite(&f, "new@x.io", &[1; 32]))
            .await
            .unwrap();
        let row = find_by_token_hash(&pool, &[1; 32]).await.unwrap().unwrap();

        assert_eq!(row.id, id);
        assert_eq!(row.workspace_id, f.ws);
        assert_eq!(row.workspace_name, "acme");
        assert_eq!(row.email, "new@x.io");
        assert_eq!(row.status, "pending");
    }

    #[sqlx::test]
    async fn find_by_unknown_hash_is_none(pool: PgPool) {
        assert!(find_by_token_hash(&pool, &[9; 32]).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn reinvite_rotates_code_on_same_row(pool: PgPool) {
        let f = fixture(&pool).await;
        let first = upsert_pending(&pool, &invite(&f, "new@x.io", &[1; 32]))
            .await
            .unwrap();

        let second = upsert_pending(&pool, &invite(&f, "NEW@x.io", &[2; 32]))
            .await
            .unwrap();

        assert_eq!(first, second);
        assert!(
            find_by_token_hash(&pool, &[1; 32]).await.unwrap().is_none(),
            "old code must die"
        );
        assert!(find_by_token_hash(&pool, &[2; 32]).await.unwrap().is_some());
    }

    #[sqlx::test]
    async fn mark_answered_only_once(pool: PgPool) {
        let f = fixture(&pool).await;
        let id = upsert_pending(&pool, &invite(&f, "new@x.io", &[1; 32]))
            .await
            .unwrap();

        assert!(mark_answered(&pool, id, &[1; 32], true).await.unwrap());
        assert!(
            !mark_answered(&pool, id, &[1; 32], false).await.unwrap(),
            "second answer must not flip status"
        );

        let row = find_by_token_hash(&pool, &[1; 32]).await.unwrap().unwrap();
        assert_eq!(row.status, "accepted");
    }

    #[sqlx::test]
    async fn rotated_code_cannot_answer(pool: PgPool) {
        let f = fixture(&pool).await;
        let id = upsert_pending(&pool, &invite(&f, "new@x.io", &[1; 32]))
            .await
            .unwrap();
        upsert_pending(&pool, &invite(&f, "new@x.io", &[2; 32]))
            .await
            .unwrap(); // re-invite

        assert!(
            !mark_answered(&pool, id, &[1; 32], true).await.unwrap(),
            "old code is dead"
        );
        assert!(mark_answered(&pool, id, &[2; 32], true).await.unwrap());
    }

    #[sqlx::test]
    async fn sends_in_last_hour_counts_recent_sends_by_inviter(pool: PgPool) {
        let f = fixture(&pool).await;
        for _ in 0..3 {
            record_send(&pool, f.owner).await.unwrap();
        }
        sqlx::query(
            "UPDATE invite_sends SET sent_at = now() - interval '2 hours'
             WHERE id = (SELECT id FROM invite_sends ORDER BY id LIMIT 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(sends_in_last_hour(&pool, f.owner).await.unwrap(), 2);
        assert_eq!(sends_in_last_hour(&pool, Uuid::nil()).await.unwrap(), 0);
    }

    #[sqlx::test]
    async fn decline_sets_declined(pool: PgPool) {
        let f = fixture(&pool).await;
        let id = upsert_pending(&pool, &invite(&f, "new@x.io", &[1; 32]))
            .await
            .unwrap();

        mark_answered(&pool, id, &[1; 32], false).await.unwrap();

        assert_eq!(
            find_by_token_hash(&pool, &[1; 32])
                .await
                .unwrap()
                .unwrap()
                .status,
            "declined"
        );
    }

    #[sqlx::test]
    async fn answered_invite_allows_fresh_pending_row(pool: PgPool) {
        let f = fixture(&pool).await;
        let old = upsert_pending(&pool, &invite(&f, "new@x.io", &[1; 32]))
            .await
            .unwrap();
        mark_answered(&pool, old, &[1; 32], false).await.unwrap();

        let fresh = upsert_pending(&pool, &invite(&f, "new@x.io", &[2; 32]))
            .await
            .unwrap();

        assert_ne!(old, fresh);
    }
}
