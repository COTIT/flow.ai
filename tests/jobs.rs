//! Job rail: claim, lease and fencing, retries, caps, dedupe, pruning (`jobs::repo`).

use std::{collections::HashSet, time::Duration};

use genesy_360_api::jobs::repo::{self, NewJob};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const KIND: &str = "test.noop";
const LEASE: Duration = Duration::from_secs(60);

async fn workspace(pool: &PgPool, name: &str) -> Uuid {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, first_name, last_name, password_hash)
         VALUES ($1, 'O', 'W', 'h') RETURNING id",
    )
    .bind(format!("{name}@x.io"))
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO workspaces (name, created_by_user_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(name)
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn enqueue(pool: &PgPool, job: NewJob) -> Uuid {
    let mut conn = pool.acquire().await.unwrap();
    repo::enqueue(&mut conn, &job).await.unwrap()
}

async fn claim(pool: &PgPool, cap: i64) -> Option<repo::Claimed> {
    repo::claim(pool, &[KIND], cap, LEASE).await.unwrap()
}

async fn status(pool: &PgPool, id: Uuid) -> (String, i32) {
    sqlx::query_as("SELECT status, attempts FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Lease ran out (worker stalled): what the reaper sees.
async fn expire(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE jobs SET locked_until = now() - interval '1 second' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn due_now(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE jobs SET run_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn claim_never_hands_one_job_to_two_workers(pool: PgPool) {
    for _ in 0..200 {
        enqueue(&pool, NewJob::new(KIND)).await;
    }
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let pool = pool.clone();
        tasks.push(tokio::spawn(async move {
            let mut got = Vec::new();
            while let Some(job) = repo::claim(&pool, &[KIND], 1_000, LEASE).await.unwrap() {
                got.push(job.id);
            }
            got
        }));
    }
    let mut all = Vec::new();
    for t in tasks {
        all.extend(t.await.unwrap());
    }
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!((all.len(), unique.len()), (200, 200));
}

#[sqlx::test]
async fn unknown_kind_is_never_claimed(pool: PgPool) {
    enqueue(&pool, NewJob::new("test.newer_color")).await;
    assert!(claim(&pool, 10).await.is_none());
}

#[sqlx::test]
async fn claim_takes_due_jobs_by_priority_then_age(pool: PgPool) {
    let old = enqueue(&pool, NewJob::new(KIND)).await;
    let urgent = enqueue(
        &pool,
        NewJob {
            priority: 5,
            ..NewJob::new(KIND)
        },
    )
    .await;
    let later = enqueue(&pool, NewJob::new(KIND)).await;
    sqlx::query("UPDATE jobs SET run_at = now() + interval '1 hour' WHERE id = $1")
        .bind(later)
        .execute(&pool)
        .await
        .unwrap();

    let first = claim(&pool, 10).await.unwrap();
    let second = claim(&pool, 10).await.unwrap();

    assert_eq!((first.id, second.id), (urgent, old));
    assert!(claim(&pool, 10).await.is_none(), "not due yet");
    assert_eq!(first.attempts, 1);
    assert_eq!(first.kind, KIND);
}

#[sqlx::test]
async fn concurrency_key_runs_one_at_a_time(pool: PgPool) {
    let key = || Some("import:ws".to_owned());
    let a = enqueue(
        &pool,
        NewJob {
            concurrency_key: key(),
            ..NewJob::new(KIND)
        },
    )
    .await;
    let b = enqueue(
        &pool,
        NewJob {
            concurrency_key: key(),
            ..NewJob::new(KIND)
        },
    )
    .await;

    let first = claim(&pool, 10).await.unwrap();
    assert_eq!(first.id, a);
    assert!(claim(&pool, 10).await.is_none(), "b waits while a runs");

    assert!(repo::finish(&pool, a, first.token).await.unwrap());
    assert_eq!(claim(&pool, 10).await.unwrap().id, b);
}

#[sqlx::test]
async fn dedupe_key_returns_the_live_job(pool: PgPool) {
    let key = || Some("stock_seed:ws".to_owned());
    let a = enqueue(
        &pool,
        NewJob {
            dedupe_key: key(),
            ..NewJob::new(KIND)
        },
    )
    .await;
    let again = enqueue(
        &pool,
        NewJob {
            dedupe_key: key(),
            ..NewJob::new(KIND)
        },
    )
    .await;
    assert_eq!(a, again);

    let job = claim(&pool, 10).await.unwrap();
    let while_running = enqueue(
        &pool,
        NewJob {
            dedupe_key: key(),
            ..NewJob::new(KIND)
        },
    )
    .await;
    assert_eq!(while_running, a);

    repo::finish(&pool, a, job.token).await.unwrap();
    let after = enqueue(
        &pool,
        NewJob {
            dedupe_key: key(),
            ..NewJob::new(KIND)
        },
    )
    .await;
    assert_ne!(after, a, "a finished job no longer dedupes");
}

#[sqlx::test]
async fn checkpoint_refuses_a_worker_that_lost_its_lease(pool: PgPool) {
    let id = enqueue(&pool, NewJob::new(KIND)).await;
    let zombie = claim(&pool, 10).await.unwrap();
    expire(&pool, id).await;
    assert_eq!(repo::reap(&pool).await.unwrap(), 1);
    due_now(&pool, id).await;
    let fresh = claim(&pool, 10).await.unwrap();
    assert_ne!(zombie.token, fresh.token);

    let mut tx = pool.begin().await.unwrap();
    let stale = zombie
        .checkpoint(&mut *tx, &json!({"after": 1}), 1, None)
        .await
        .unwrap();
    assert!(!stale, "the zombie's chunk must roll back");
    tx.rollback().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    assert!(
        fresh
            .checkpoint(&mut *tx, &json!({"after": 2}), 2, Some(10))
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    let (payload, done, total): (serde_json::Value, i64, Option<i64>) =
        sqlx::query_as("SELECT payload, progress_done, progress_total FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((payload, done, total), (json!({"after": 2}), 2, Some(10)));
    assert!(!repo::finish(&pool, id, zombie.token).await.unwrap());
}

#[sqlx::test]
async fn reap_backs_off_then_fails_after_max_attempts(pool: PgPool) {
    let id = enqueue(
        &pool,
        NewJob {
            max_attempts: 2,
            ..NewJob::new(KIND)
        },
    )
    .await;
    claim(&pool, 10).await.unwrap();
    expire(&pool, id).await;
    repo::reap(&pool).await.unwrap();
    assert_eq!(status(&pool, id).await, ("queued".into(), 1));
    assert!(claim(&pool, 10).await.is_none(), "backoff: not due yet");

    due_now(&pool, id).await;
    claim(&pool, 10).await.unwrap();
    expire(&pool, id).await;
    repo::reap(&pool).await.unwrap();
    let (state, last_error): (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "failed");
    assert!(last_error.unwrap().contains("lease"));
}

#[sqlx::test]
async fn retry_backs_off_and_fail_is_final(pool: PgPool) {
    let id = enqueue(
        &pool,
        NewJob {
            max_attempts: 2,
            ..NewJob::new(KIND)
        },
    )
    .await;
    let job = claim(&pool, 10).await.unwrap();
    assert!(
        repo::retry(&pool, id, job.token, "smtp down")
            .await
            .unwrap()
    );
    assert_eq!(status(&pool, id).await, ("queued".into(), 1));
    assert!(claim(&pool, 10).await.is_none(), "backoff");

    due_now(&pool, id).await;
    let job = claim(&pool, 10).await.unwrap();
    assert!(
        repo::retry(&pool, id, job.token, "smtp down")
            .await
            .unwrap()
    );
    assert_eq!(status(&pool, id).await.0, "failed", "out of attempts");

    let other = enqueue(&pool, NewJob::new(KIND)).await;
    let job = claim(&pool, 10).await.unwrap();
    assert!(
        repo::fail(&pool, other, job.token, "bad payload")
            .await
            .unwrap()
    );
    assert_eq!(status(&pool, other).await.0, "failed");
}

#[sqlx::test]
async fn requeue_goes_to_the_back_and_resets_attempts(pool: PgPool) {
    let chunked = enqueue(&pool, NewJob::new(KIND)).await;
    let job = claim(&pool, 10).await.unwrap();
    let waiting = enqueue(&pool, NewJob::new(KIND)).await;

    assert!(repo::requeue(&pool, chunked, job.token).await.unwrap());
    assert_eq!(status(&pool, chunked).await, ("queued".into(), 0));
    assert_eq!(claim(&pool, 10).await.unwrap().id, waiting);
    assert_eq!(claim(&pool, 10).await.unwrap().id, chunked);
}

#[sqlx::test]
async fn running_cap_per_workspace_lets_others_through(pool: PgPool) {
    let (a, b) = (workspace(&pool, "a").await, workspace(&pool, "b").await);
    for _ in 0..3 {
        enqueue(
            &pool,
            NewJob {
                workspace_id: Some(a),
                ..NewJob::new(KIND)
            },
        )
        .await;
    }
    enqueue(
        &pool,
        NewJob {
            workspace_id: Some(b),
            ..NewJob::new(KIND)
        },
    )
    .await;
    enqueue(&pool, NewJob::new(KIND)).await;

    let mut got = Vec::new();
    while let Some(job) = claim(&pool, 2).await {
        got.push(job.workspace_id);
    }

    assert_eq!(
        got.iter().filter(|w| **w == Some(a)).count(),
        2,
        "cap 2 for a"
    );
    assert!(got.contains(&Some(b)));
    assert!(got.contains(&None), "system jobs have no cap");
}

#[sqlx::test]
async fn renew_extends_only_a_held_lease(pool: PgPool) {
    let id = enqueue(&pool, NewJob::new(KIND)).await;
    let job = claim(&pool, 10).await.unwrap();
    assert!(repo::renew(&pool, id, job.token, LEASE).await.unwrap());
    assert!(!repo::renew(&pool, id, Uuid::now_v7(), LEASE).await.unwrap());
    repo::finish(&pool, id, job.token).await.unwrap();
    assert!(!repo::renew(&pool, id, job.token, LEASE).await.unwrap());
}

#[sqlx::test]
async fn prune_keeps_live_and_recent_jobs(pool: PgPool) {
    let old_done = enqueue(&pool, NewJob::new(KIND)).await;
    let old_failed = enqueue(&pool, NewJob::new(KIND)).await;
    let recent_failed = enqueue(&pool, NewJob::new(KIND)).await;
    let queued = enqueue(&pool, NewJob::new(KIND)).await;
    sqlx::query(
        "UPDATE jobs SET status = 'succeeded', finished_at = now() - interval '8 days' WHERE id = $1",
    )
    .bind(old_done)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE jobs SET status = 'failed', finished_at = now() - $2::interval WHERE id = $1",
    )
    .bind(old_failed)
    .bind("31 days")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE jobs SET status = 'failed', finished_at = now() - $2::interval WHERE id = $1",
    )
    .bind(recent_failed)
    .bind("8 days")
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(repo::prune(&pool).await.unwrap(), 2);

    let left: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM jobs ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, vec![recent_failed, queued]);
}

#[sqlx::test]
async fn deleting_a_workspace_drops_its_jobs(pool: PgPool) {
    let ws = workspace(&pool, "gone").await;
    enqueue(
        &pool,
        NewJob {
            workspace_id: Some(ws),
            ..NewJob::new(KIND)
        },
    )
    .await;
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(ws)
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test]
async fn error_text_is_capped(pool: PgPool) {
    let id = enqueue(&pool, NewJob::new(KIND)).await;
    let job = claim(&pool, 10).await.unwrap();
    repo::fail(&pool, id, job.token, &"é".repeat(5_000))
        .await
        .unwrap();
    let len: i32 = sqlx::query_scalar("SELECT char_length(last_error) FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(len <= 1_000, "{len}");
}
