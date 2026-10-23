//! Workers: claim a due job, run its handler while a side task keeps the lease alive, record
//! the outcome. A lost lease aborts the handler; a stop signal lets the running job finish
//! and claims nothing more.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use sqlx::PgPool;
use tokio::{sync::watch, task::JoinHandle};
use uuid::Uuid;

use crate::{
    error::AppError,
    jobs::repo::{self, Claimed},
};

/// What a handler did with its job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing left: the job succeeded.
    Done,
    /// A chunk is committed (cursor saved with `Claimed::checkpoint`); queue it again.
    More,
}

/// A handler's future.
pub type HandlerFuture = Pin<Box<dyn Future<Output = Result<Step, AppError>> + Send>>;

/// Runs one claimed job (or one chunk of it). Holds at most one connection at a time.
pub type Handler = Arc<dyn Fn(Claimed) -> HandlerFuture + Send + Sync>;

/// A pool of workers over one handler.
pub struct Runner {
    /// Claims and outcomes.
    pub db: PgPool,
    /// Lease renewals, apart from `db` so a pool full of chunks can't starve them.
    pub control: PgPool,
    /// Kinds this build can run; anything else stays queued for the color that knows it.
    pub kinds: Vec<&'static str>,
    pub workers: usize,
    /// Running jobs per workspace (soft cap).
    pub cap: i64,
    pub lease: Duration,
    /// Wait before polling again when nothing is due.
    pub idle: Duration,
    pub handler: Handler,
}

impl Runner {
    /// Start the workers. The handle resolves once `stop` turns true and every worker has
    /// finished its current job.
    pub fn spawn(self, stop: watch::Receiver<bool>) -> JoinHandle<()> {
        let workers = self.workers.max(1);
        let runner = Arc::new(self);
        let handles: Vec<_> = (0..workers)
            .map(|_| tokio::spawn(work(runner.clone(), stop.clone())))
            .collect();
        tokio::spawn(async move {
            for h in handles {
                if let Err(err) = h.await {
                    tracing::error!(error = %err, "job worker ended abnormally");
                }
            }
        })
    }
}

async fn work(r: Arc<Runner>, mut stop: watch::Receiver<bool>) {
    while !*stop.borrow() {
        match repo::claim(&r.db, &r.kinds, r.cap, r.lease).await {
            Ok(Some(job)) => run(&r, job).await,
            Ok(None) => idle(&r, &mut stop).await,
            Err(err) => {
                tracing::warn!(error = %err, "job claim failed");
                idle(&r, &mut stop).await;
            }
        }
    }
}

async fn idle(r: &Runner, stop: &mut watch::Receiver<bool>) {
    tokio::select! {
        _ = tokio::time::sleep(r.idle) => {}
        _ = stop.changed() => {}
    }
}

/// Run one job under its lease, then record what happened.
async fn run(r: &Runner, job: Claimed) {
    let (id, token, kind) = (job.id, job.token, job.kind.clone());
    let mut task = tokio::spawn((r.handler)(job));
    let outcome = tokio::select! {
        res = &mut task => res,
        () = hold_lease(r, id, token) => {
            task.abort();
            tracing::warn!(%id, %kind, "job lease lost; handler stopped, its chunk rolls back");
            return;
        }
    };
    let recorded = match outcome {
        Ok(Ok(Step::Done)) => repo::finish(&r.db, id, token).await,
        Ok(Ok(Step::More)) => repo::requeue(&r.db, id, token).await,
        Ok(Err(err)) if err.is_permanent() => {
            tracing::warn!(%id, %kind, error = %err, "job failed");
            repo::fail(&r.db, id, token, &err.to_string()).await
        }
        Ok(Err(err)) => {
            tracing::warn!(%id, %kind, error = %err, "job will retry");
            repo::retry(&r.db, id, token, &err.to_string()).await
        }
        Err(join) => {
            tracing::error!(%id, %kind, error = %join, "job handler panicked");
            repo::retry(&r.db, id, token, "handler panicked").await
        }
    };
    match recorded {
        Ok(true) => {}
        // Lease ran out while the outcome was on its way; the reaper or a new claim owns it.
        Ok(false) => tracing::warn!(%id, %kind, "job outcome dropped: lease already gone"),
        Err(err) => tracing::warn!(%id, %kind, error = %err, "job outcome not saved"),
    }
}

/// Renew the lease every third of its length. Returns only when the lease is gone.
async fn hold_lease(r: &Runner, id: Uuid, token: Uuid) {
    let every = r.lease / 3;
    loop {
        tokio::time::sleep(every).await;
        match repo::renew(&r.control, id, token, r.lease).await {
            Ok(true) => {}
            Ok(false) => return,
            // DB unreachable: keep trying; fencing stops this worker if the lease runs out.
            Err(err) => tracing::warn!(%id, error = %err, "lease renewal failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use serde_json::json;
    use sqlx::PgPool;
    use tokio::sync::watch;
    use uuid::Uuid;

    use super::*;
    use crate::jobs::repo::{self, NewJob};

    const KIND: &str = "test.work";

    fn runner(db: &PgPool, lease: Duration, handler: Handler) -> Runner {
        Runner {
            db: db.clone(),
            control: db.clone(),
            kinds: vec![KIND],
            workers: 4,
            cap: 1_000,
            lease,
            idle: Duration::from_millis(20),
            handler,
        }
    }

    async fn enqueue(db: &PgPool, payload: serde_json::Value) -> Uuid {
        let mut conn = db.acquire().await.unwrap();
        repo::enqueue(
            &mut conn,
            &NewJob {
                payload,
                ..NewJob::new(KIND)
            },
        )
        .await
        .unwrap()
    }

    async fn status(db: &PgPool, id: Uuid) -> (String, Option<String>) {
        sqlx::query_as("SELECT status, last_error FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    /// Poll until no job of the test kind is queued or running, or panic after 30 s.
    async fn settle(db: &PgPool) {
        for _ in 0..1_500 {
            let open: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM jobs WHERE status IN ('queued', 'running') AND run_at <= now()",
            )
            .fetch_one(db)
            .await
            .unwrap();
            if open == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("jobs never settled");
    }

    #[sqlx::test]
    async fn two_runners_run_every_job_exactly_once(db: PgPool) {
        let mut ids = Vec::new();
        for _ in 0..500 {
            ids.push(enqueue(&db, json!({})).await);
        }
        let seen: Arc<Mutex<HashMap<Uuid, usize>>> = Arc::default();
        let handler: Handler = {
            let seen = seen.clone();
            Arc::new(move |job| {
                let seen = seen.clone();
                Box::pin(async move {
                    *seen.lock().unwrap().entry(job.id).or_default() += 1;
                    Ok(Step::Done)
                })
            })
        };
        let (stop, rx) = watch::channel(false);
        let a = runner(&db, Duration::from_secs(60), handler.clone()).spawn(rx.clone());
        let b = runner(&db, Duration::from_secs(60), handler).spawn(rx);

        settle(&db).await;
        stop.send(true).unwrap();
        a.await.unwrap();
        b.await.unwrap();

        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen.len(), 500);
            assert!(seen.values().all(|n| *n == 1), "a job ran twice");
        }
        let done: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status = 'succeeded'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(done, 500);
    }

    #[sqlx::test]
    async fn more_requeues_until_the_cursor_is_done(db: PgPool) {
        let id = enqueue(&db, json!({"n": 0})).await;
        let pool = db.clone();
        let handler: Handler = Arc::new(move |job| {
            let pool = pool.clone();
            Box::pin(async move {
                let n = job.payload["n"].as_i64().unwrap_or(0) + 1;
                let mut tx = pool.begin().await?;
                if !job
                    .checkpoint(&mut *tx, &json!({"n": n}), n, Some(3))
                    .await?
                {
                    return Err(AppError::Internal("lease lost".into()));
                }
                tx.commit().await?;
                Ok(if n < 3 { Step::More } else { Step::Done })
            })
        });
        let (stop, rx) = watch::channel(false);
        let r = runner(&db, Duration::from_secs(60), handler).spawn(rx);

        settle(&db).await;
        stop.send(true).unwrap();
        r.await.unwrap();

        let (state, payload, done): (String, serde_json::Value, i64) =
            sqlx::query_as("SELECT status, payload, progress_done FROM jobs WHERE id = $1")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(
            (state.as_str(), payload, done),
            ("succeeded", json!({"n": 3}), 3)
        );
    }

    #[sqlx::test]
    async fn errors_retry_unless_permanent_and_panics_retry(db: PgPool) {
        let busy = enqueue(&db, json!({"fail": "busy"})).await;
        let gone = enqueue(&db, json!({"fail": "gone"})).await;
        let boom = enqueue(&db, json!({"fail": "panic"})).await;
        let handler: Handler = Arc::new(|job| {
            Box::pin(async move {
                match job.payload["fail"].as_str() {
                    Some("busy") => Err(AppError::Overloaded),
                    Some("gone") => Err(AppError::NotFound),
                    _ => panic!("handler bug"),
                }
            })
        });
        let (stop, rx) = watch::channel(false);
        let r = runner(&db, Duration::from_secs(60), handler).spawn(rx);

        settle(&db).await;
        stop.send(true).unwrap();
        r.await.unwrap();

        assert_eq!(
            status(&db, busy).await,
            ("queued".into(), Some("server busy".into()))
        );
        assert_eq!(
            status(&db, gone).await,
            ("failed".into(), Some("not found".into()))
        );
        let (state, error) = status(&db, boom).await;
        assert_eq!(state, "queued");
        assert!(error.unwrap().contains("panicked"));
    }

    #[sqlx::test]
    async fn a_lost_lease_stops_the_handler(db: PgPool) {
        let id = enqueue(&db, json!({})).await;
        let finished = Arc::new(AtomicUsize::new(0));
        let handler: Handler = {
            let finished = finished.clone();
            Arc::new(move |_| {
                let finished = finished.clone();
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    finished.fetch_add(1, Ordering::SeqCst);
                    Ok(Step::Done)
                })
            })
        };
        let (stop, rx) = watch::channel(false);
        let r = runner(&db, Duration::from_millis(300), handler).spawn(rx);
        while status(&db, id).await.0 != "running" {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Another color took the job over.
        let thief: Uuid = sqlx::query_scalar(
            "UPDATE jobs SET locked_by = uuidv7() WHERE id = $1 RETURNING locked_by",
        )
        .bind(id)
        .fetch_one(&db)
        .await
        .unwrap();

        tokio::time::sleep(Duration::from_millis(600)).await;
        stop.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), r)
            .await
            .expect("aborted handler must not hold the worker")
            .unwrap();

        assert_eq!(
            finished.load(Ordering::SeqCst),
            0,
            "handler ran on after losing the lease"
        );
        let owner: Uuid = sqlx::query_scalar("SELECT locked_by FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(owner, thief, "the old worker changed nothing");
    }

    #[sqlx::test]
    async fn stop_lets_the_running_job_finish_and_claims_no_more(db: PgPool) {
        let first = enqueue(&db, json!({})).await;
        let handler: Handler = Arc::new(|_| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                Ok(Step::Done)
            })
        });
        let (stop, rx) = watch::channel(false);
        let r = Runner {
            workers: 1,
            ..runner(&db, Duration::from_secs(60), handler)
        }
        .spawn(rx);
        while status(&db, first).await.0 != "running" {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let second = enqueue(&db, json!({})).await;
        stop.send(true).unwrap();
        r.await.unwrap();

        assert_eq!(status(&db, first).await.0, "succeeded");
        assert_eq!(status(&db, second).await.0, "queued");
    }
}
