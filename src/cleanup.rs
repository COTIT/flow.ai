//! Hourly cleanup of rows nothing reads anymore (expired tokens, old invites, quota logs) and
//! of files a crash left half written or half deleted. Scheduler only: the work lives in each
//! module. Idempotent, so several replicas running it at once is harmless.

use std::time::Duration;

use sqlx::PgPool;

use crate::{
    modules::{
        auth::repo as auth, email_verification::repo as email_verification, files,
        invites::repo as invites,
    },
    storage::Store,
};

const EVERY: Duration = Duration::from_secs(60 * 60);

/// Rows removed by one `prune` run, per table.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pruned {
    pub access_tokens: u64,
    pub verification_tokens: u64,
    pub invites: u64,
    pub invite_sends: u64,
    pub login_failures: u64,
}

/// Delete stale rows via each module's repo. Windows are wider than any rule that reads them.
pub async fn prune(db: &PgPool) -> Result<Pruned, sqlx::Error> {
    Ok(Pruned {
        access_tokens: auth::prune_expired_tokens(db).await?,
        verification_tokens: email_verification::prune_expired(db).await?,
        invites: invites::prune_old(db).await?,
        invite_sends: invites::prune_old_sends(db).await?,
        login_failures: auth::prune_old_login_attempts(db).await?,
    })
}

/// Finish file rows left pending or deleting (up to 100 a run); how many it finished.
pub async fn sweep_files(db: &PgPool, bucket: &Store) -> Result<u64, String> {
    files::service::sweep(db, bucket)
        .await
        .map_err(|e| e.to_string())
}

/// Run `prune` (and the file sweep, with a bucket) every hour in the background. Failures are
/// logged and retried next tick.
pub fn spawn(db: PgPool, bucket: Option<Store>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tick.tick().await;
            match prune(&db).await {
                Ok(pruned) => tracing::info!(?pruned, "cleanup done"),
                Err(err) => tracing::warn!(error = %err, "cleanup failed, retrying next hour"),
            }
            if let Some(bucket) = &bucket {
                match sweep_files(&db, bucket).await {
                    Ok(n) => tracing::info!(files = n, "file sweep done"),
                    Err(err) => {
                        tracing::warn!(error = %err, "file sweep failed, retrying next hour")
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    async fn seed_user(pool: &PgPool) -> Uuid {
        let user: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, first_name, last_name, password_hash)
             VALUES ('o@x.io', 'O', 'W', 'h') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO workspaces (name, created_by_user_id) VALUES ('acme', $1)")
            .bind(user)
            .execute(pool)
            .await
            .unwrap();
        user
    }

    /// Run seed SQL that references the user as $1.
    async fn run(pool: &PgPool, sql: &'static str, user: Uuid) {
        sqlx::query(sql).bind(user).execute(pool).await.unwrap();
    }

    async fn count(pool: &PgPool, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
    }

    #[sqlx::test]
    async fn prune_removes_only_stale_rows(pool: PgPool) {
        let user = seed_user(&pool).await;
        // One stale + one fresh row per table.
        run(
            &pool,
            "INSERT INTO access_tokens (user_id, name, token_hash, expires_at) VALUES
            ($1, 't', sha256('a'::bytea), now() - interval '1 second'),
            ($1, 't', sha256('b'::bytea), now() + interval '1 day')",
            user,
        )
        .await;
        run(
            &pool,
            "INSERT INTO email_verification_tokens (user_id, token_hash, expires_at) VALUES
            ($1, sha256('c'::bytea), now() - interval '1 second'),
            ($1, sha256('d'::bytea), now() + interval '1 day')",
            user,
        )
        .await;
        run(&pool, "INSERT INTO invites
                (workspace_id, invited_by_user_id, email, token_hash, expires_at, status, updated_at)
            SELECT w.id, $1, v.email, sha256(v.email::bytea), now() - v.expired::interval, v.status,
                   now() - v.touched::interval
            FROM workspaces w, (VALUES
                ('a@x.io', '40 days', 'denied',   '31 days'),
                ('b@x.io', '31 days', 'pending',  '38 days'),
                ('c@x.io', '2 days',  'accepted', '2 days'),
                ('d@x.io', '1 day',   'pending',  '8 days')
            ) AS v(email, expired, status, touched)", user).await;
        run(
            &pool,
            "INSERT INTO invite_sends (invited_by_user_id, sent_at) VALUES
            ($1, now() - interval '25 hours'), ($1, now() - interval '30 minutes')",
            user,
        )
        .await;
        sqlx::query(
            "INSERT INTO login_failures (email_norm, failed_at) VALUES
            ('x', now() - interval '25 hours'), ('x', now() - interval '5 minutes')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let pruned = prune(&pool).await.unwrap();

        assert_eq!(
            pruned,
            Pruned {
                access_tokens: 1,
                verification_tokens: 1,
                invites: 2,
                invite_sends: 1,
                login_failures: 1,
            }
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM invites").await,
            2,
            "recent answered + recently expired stay"
        );
        assert_eq!(
            prune(&pool).await.unwrap(),
            Pruned::default(),
            "second run is a no-op"
        );
    }
}
