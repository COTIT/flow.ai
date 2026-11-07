//! Cleanup of rows nothing reads anymore (expired tokens and till sessions, old invites, quota logs, old jobs
//! and scheduler slots) and of files a crash left half written or half deleted. The job rail
//! runs it on a schedule (`jobs::kinds`); the work lives in each module. Idempotent.

use sqlx::PgPool;

use crate::{
    jobs,
    modules::{
        auth::repo as auth, email_verification::repo as email_verification, files,
        invites::repo as invites, messages::repo as messages, till::repo as till,
        webhooks::repo as webhooks,
    },
    storage::Store,
};

/// Rows removed by one `prune` run, per table.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pruned {
    pub access_tokens: u64,
    pub verification_tokens: u64,
    pub invites: u64,
    pub invite_sends: u64,
    pub login_failures: u64,
    /// Sends a dead request left open, marked `unknown` (never sent again).
    pub stuck_sends: u64,
    /// Finished jobs past their retention (`jobs::repo::prune`).
    pub jobs: u64,
    /// Scheduler slots older than 30 days.
    pub scheduled_runs: u64,
    /// Idempotency keys older than a day.
    pub idempotency_keys: u64,
    /// Webhook deliveries finished (or stuck pending) over 30 days ago.
    pub webhook_deliveries: u64,
    /// Dispatched webhook events older than 30 days with no delivery left.
    pub webhook_events: u64,
    /// Till sessions ended or expired over a day ago.
    pub till_sessions: u64,
    /// Resolved assistant actions older than 90 days; unresolved claims stay.
    pub assistant_actions: u64,
}

/// Delete stale rows via each module's repo. Windows are wider than any rule that reads them.
pub async fn prune(db: &PgPool) -> Result<Pruned, sqlx::Error> {
    Ok(Pruned {
        // First: a failure further down must not leave open sends open.
        stuck_sends: messages::close_stuck(db).await?,
        assistant_actions: crate::modules::assistant::repo::prune(db).await?,
        access_tokens: auth::prune_expired_tokens(db).await?,
        verification_tokens: email_verification::prune_expired(db).await?,
        invites: invites::prune_old(db).await?,
        invite_sends: invites::prune_old_sends(db).await?,
        login_failures: auth::prune_old_login_attempts(db).await?,
        jobs: jobs::repo::prune(db).await?,
        scheduled_runs: jobs::scheduler::prune(db).await?,
        idempotency_keys: crate::idempotency::prune(db).await?,
        // Deliveries first: an event goes once its last delivery is gone.
        webhook_deliveries: webhooks::prune_deliveries(db).await?,
        webhook_events: webhooks::prune_events(db).await?,
        till_sessions: till::prune_sessions(db).await?,
    })
}

/// Finish file rows left pending or deleting (up to 1000 a run); how many it finished.
pub async fn sweep_files(db: &PgPool, bucket: &Store) -> Result<u64, String> {
    files::service::sweep(db, bucket)
        .await
        .map_err(|e| e.to_string())
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
        run(
            &pool,
            "INSERT INTO idempotency_keys (workspace_id, key, user_id, request_hash, status,
                                           response, created_at)
             SELECT w.id, v.key, $1, sha256(v.key::bytea), 201, '', now() - v.age::interval
             FROM workspaces w, (VALUES ('old', '25 hours'), ('new', '1 hour')) AS v(key, age)",
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
                assistant_actions: 0,
                access_tokens: 1,
                verification_tokens: 1,
                invites: 2,
                invite_sends: 1,
                login_failures: 1,
                stuck_sends: 0,
                jobs: 0,
                scheduled_runs: 0,
                idempotency_keys: 1,
                webhook_deliveries: 0,
                webhook_events: 0,
                till_sessions: 0,
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

    #[sqlx::test]
    async fn prune_drops_finished_webhook_rows_after_30_days_only(pool: PgPool) {
        let user = seed_user(&pool).await;
        // Endpoint + 4 events: old failed, old stuck pending, old undispatched, fresh delivered.
        run(
            &pool,
            "WITH w AS (SELECT id FROM workspaces WHERE created_by_user_id = $1),
                  ep AS (INSERT INTO webhook_endpoints (workspace_id, url, events, secret_sealed,
                                                        secret_key_id)
                         SELECT id, 'https://a.io/h', '{stock.changed}', '\\x00', 'k' FROM w
                         RETURNING id, workspace_id),
                  ev AS (INSERT INTO webhook_events (workspace_id, type, payload, dispatched_at)
                         SELECT w.id, 'stock.changed', jsonb_build_object('n', n),
                                CASE WHEN n = 3 THEN NULL
                                     WHEN n = 4 THEN now() - interval '1 day'
                                     ELSE now() - interval '31 days' END
                         FROM w, generate_series(1, 4) n
                         RETURNING id, workspace_id, (payload->>'n')::int AS n)
             INSERT INTO webhook_deliveries (workspace_id, event_id, endpoint_id, status, updated_at)
             SELECT ev.workspace_id, ev.id, ep.id,
                    CASE ev.n WHEN 2 THEN 'pending' WHEN 1 THEN 'failed' ELSE 'delivered' END,
                    CASE WHEN ev.n = 4 THEN now() - interval '1 day'
                         ELSE now() - interval '31 days' END
             FROM ev, ep WHERE ev.n <> 3",
            user,
        )
        .await;

        let pruned = prune(&pool).await.unwrap();

        assert_eq!((pruned.webhook_deliveries, pruned.webhook_events), (2, 2));
        let left: Vec<i32> =
            sqlx::query_scalar("SELECT (payload->>'n')::int FROM webhook_events ORDER BY 1")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(left, [3, 4], "undispatched and fresh rows stay");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM webhook_deliveries").await,
            1
        );
    }

    #[sqlx::test]
    async fn prune_drops_till_sessions_a_day_after_they_stop(pool: PgPool) {
        let user = seed_user(&pool).await;
        run(
            &pool,
            "WITH w AS (SELECT id FROM workspaces WHERE created_by_user_id = $1),
                  c AS (INSERT INTO currencies (workspace_id, name, asset_type)
                        SELECT id, 'Euro', 'EUR' FROM w RETURNING id, workspace_id),
                  h AS (INSERT INTO warehouses (workspace_id, name, slug, is_store)
                        SELECT id, 'Shop', 'shop', true FROM w RETURNING id),
                  k AS (INSERT INTO app_keys (workspace_id, name, kind, scopes, key_hash, prefix)
                        SELECT id, 'Till', 'till', '{till:login}', decode(repeat('ab', 32), 'hex'),
                               'gsk_abcd' FROM w RETURNING id),
                  r AS (INSERT INTO till_registers (workspace_id, name, warehouse_id, currency_id,
                                                    app_key_id)
                        SELECT c.workspace_id, 'Till 1', h.id, c.id, k.id FROM c, h, k
                        RETURNING id, workspace_id, app_key_id),
                  u AS (INSERT INTO till_users (workspace_id, name, pin_hash)
                        SELECT id, 'Mary', 'x' FROM w RETURNING id)
             INSERT INTO till_sessions (workspace_id, register_id, till_user_id, app_key_id,
                                        token_hash, created_at, expires_at, ended_at)
             SELECT r.workspace_id, r.id, u.id, r.app_key_id, sha256(n::text::bytea),
                    now() - interval '3 days',
                    CASE n WHEN 1 THEN now() - interval '2 days'
                           WHEN 4 THEN now() - interval '1 hour'
                           ELSE now() + interval '1 hour' END,
                    CASE n WHEN 2 THEN now() - interval '2 days'
                           WHEN 3 THEN now() - interval '1 hour' END
             FROM r, u, generate_series(1, 5) n",
            user,
        )
        .await;

        let pruned = prune(&pool).await.unwrap();

        assert_eq!(pruned.till_sessions, 2, "expired and ended over a day ago");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM till_sessions").await,
            3,
            "recently ended, recently expired and live stay"
        );
    }
}
