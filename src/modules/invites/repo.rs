//! invites table SQL. Rows found by sha256(code) only; plaintext code never reaches DB.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::teams::repo::TeamRow;

/// Insert/rotate payload. `token_hash` = sha256 of emailed code.
pub struct NewInvite<'a> {
    pub workspace_id: Uuid,
    pub invited_by_user_id: Uuid,
    pub email: &'a str,
    pub token_hash: &'a [u8; 32],
    pub expires_at: DateTime<Utc>,
    pub team_id: Option<Uuid>,
}

/// Invite plus workspace name, enough for status page and accept checks.
#[derive(Debug)]
pub struct InviteRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub workspace_name: String,
    pub email: String,
    /// `pending` | `accepted` | `denied`. Expiry is read from `expires_at`.
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    /// Current code hash; answering re-checks it so a code rotated meanwhile can't be used.
    pub token_hash: Vec<u8>,
    /// Team joined on accept, if still live then.
    pub team_id: Option<Uuid>,
}

/// Create pending invite, or rotate code + expiry + team on existing pending one for same
/// email. Rotation kills old link, so only newest email works.
pub async fn upsert_pending(
    db: impl PgExecutor<'_>,
    invite: &NewInvite<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO invites (workspace_id, invited_by_user_id, email, token_hash, expires_at, team_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (workspace_id, lower(email)) WHERE status = 'pending'
         DO UPDATE SET token_hash = EXCLUDED.token_hash,
                       expires_at = EXCLUDED.expires_at,
                       invited_by_user_id = EXCLUDED.invited_by_user_id,
                       email = EXCLUDED.email,
                       team_id = EXCLUDED.team_id,
                       updated_at = now()
         RETURNING id",
        invite.workspace_id,
        invite.invited_by_user_id,
        invite.email,
        &invite.token_hash[..],
        invite.expires_at,
        invite.team_id,
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
                i.expires_at, i.created_at, i.token_hash, i.team_id
         FROM invites i
         JOIN workspaces w ON w.id = i.workspace_id AND w.deleted_at IS NULL
         WHERE i.token_hash = $1",
        &token_hash[..],
    )
    .fetch_optional(db)
    .await
}

/// Invite by id, any status (for the /v1/me accept path). Caller checks email + verification.
pub async fn find_by_id(
    db: impl PgExecutor<'_>,
    invite_id: Uuid,
) -> Result<Option<InviteRow>, sqlx::Error> {
    sqlx::query_as!(
        InviteRow,
        "SELECT i.id, i.workspace_id, w.name AS workspace_name, i.email, i.status,
                i.expires_at, i.created_at, i.token_hash, i.team_id
         FROM invites i
         JOIN workspaces w ON w.id = i.workspace_id AND w.deleted_at IS NULL
         WHERE i.id = $1",
        invite_id,
    )
    .fetch_optional(db)
    .await
}

/// Set accepted/denied. Only flips a pending row still holding this code hash:
/// false = already answered, or code rotated by a re-invite since it was looked up.
pub async fn mark_answered(
    db: impl PgExecutor<'_>,
    invite_id: Uuid,
    token_hash: &[u8],
    accepted: bool,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE invites
         SET status = CASE WHEN $3 THEN 'accepted' ELSE 'denied' END,
             responded_at = now(),
             updated_at = now()
         WHERE id = $1 AND token_hash = $2 AND status = 'pending'",
        invite_id,
        token_hash,
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

/// Pending invite as /v1/me lists it.
pub struct PendingInvite {
    pub id: Uuid,
    pub workspace_name: String,
    pub created_at: DateTime<Utc>,
    pub inviter_email: String,
    pub inviter_first_name: String,
    pub inviter_middle_name: Option<String>,
    pub inviter_last_name: String,
}

/// Pending, unexpired invites to this email (any case) in live workspaces, newest first.
pub async fn pending_for_email(
    db: impl PgExecutor<'_>,
    email: &str,
) -> Result<Vec<PendingInvite>, sqlx::Error> {
    sqlx::query_as!(
        PendingInvite,
        "SELECT i.id, w.name AS workspace_name, i.created_at,
                u.email AS inviter_email, u.first_name AS inviter_first_name,
                u.middle_name AS inviter_middle_name, u.last_name AS inviter_last_name
         FROM invites i
         JOIN workspaces w ON w.id = i.workspace_id AND w.deleted_at IS NULL
         JOIN users u ON u.id = i.invited_by_user_id
         WHERE lower(i.email) = lower($1) AND i.status = 'pending' AND i.expires_at > now()
         ORDER BY i.created_at DESC",
        email,
    )
    .fetch_all(db)
    .await
}

/// Cleanup: answered or long-expired invites, kept 30 days for "who invited whom" questions.
pub async fn prune_old(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    sqlx::query!(
        "DELETE FROM invites
         WHERE (status <> 'pending' AND updated_at < now() - interval '30 days')
            OR (status = 'pending' AND expires_at < now() - interval '30 days')"
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected())
}

/// Cleanup: the send quota reads one hour back.
pub async fn prune_old_sends(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM invite_sends WHERE sent_at < now() - interval '1 day'")
        .execute(db)
        .await
        .map(|done| done.rows_affected())
}

/// Invite as the settings pending-invites modal lists it, with its live team if any.
pub struct WorkspaceInvite {
    pub id: Uuid,
    pub email: String,
    pub status: String,
    pub inviter_email: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub team: Option<TeamRow>,
}

/// One page of `workspace_id`'s invites, optionally one status, oldest first, + total.
pub async fn list_in_workspace(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<WorkspaceInvite>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT i.id, i.email, i.status, i.expires_at, i.created_at, i.updated_at,
                  u.email AS inviter_email,
                  t.id AS "team_id?", t.workspace_id AS "team_workspace_id?", t.name AS "team_name?",
                  t.description AS "team_description?", t.meta AS "team_meta?",
                  t.created_at AS "team_created_at?", t.updated_at AS "team_updated_at?",
                  ARRAY(SELECT p.permission FROM team_permissions p
                        WHERE p.team_id = t.id ORDER BY p.permission) AS "team_permissions!",
                  count(*) OVER () AS "total!"
           FROM invites i
           JOIN users u ON u.id = i.invited_by_user_id
           LEFT JOIN teams t ON t.id = i.team_id AND t.workspace_id = i.workspace_id
                             AND t.deleted_at IS NULL
           WHERE i.workspace_id = $1 AND ($2::text IS NULL OR i.status = $2)
           ORDER BY i.created_at, i.id
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        status,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    // Empty page past the end has no row to carry the count: caller asks `count_in_workspace`.
    let total = rows.first().map_or(0, |r| r.total);
    let invites = rows
        .into_iter()
        .map(|r| {
            // LEFT JOIN: all team columns are set together or not at all.
            let team = match (
                r.team_id,
                r.team_workspace_id,
                r.team_name,
                r.team_description,
                r.team_created_at,
                r.team_updated_at,
            ) {
                (Some(id), Some(workspace_id), Some(name), Some(description), Some(c), Some(u)) => {
                    Some(TeamRow {
                        id,
                        workspace_id,
                        name,
                        description,
                        meta: r.team_meta,
                        created_at: c,
                        updated_at: u,
                        permissions: r.team_permissions,
                    })
                }
                _ => None,
            };
            WorkspaceInvite {
                id: r.id,
                email: r.email,
                status: r.status,
                inviter_email: r.inviter_email,
                expires_at: r.expires_at,
                created_at: r.created_at,
                updated_at: r.updated_at,
                team,
            }
        })
        .collect();
    Ok((invites, total))
}

/// Invites of `workspace_id` with `status` (all when None), ignoring paging.
pub async fn count_in_workspace(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    status: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM invites
           WHERE workspace_id = $1 AND ($2::text IS NULL OR status = $2)"#,
        workspace_id,
        status,
    )
    .fetch_one(db)
    .await
}

/// Delete an invite of `workspace_id` (any status); its emailed link stops working.
/// False = no such invite here.
pub async fn delete_in_workspace(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    invite_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "DELETE FROM invites WHERE id = $1 AND workspace_id = $2",
        invite_id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
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
            team_id: None,
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
    async fn decline_sets_denied(pool: PgPool) {
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
            "denied"
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
