//! Scheduler: every instance ticks every 30 s. The one holding the advisory lock enqueues
//! each schedule's latest due slot once (`scheduled_runs` primary key) and reaps expired
//! leases. A slot missed while every instance was down runs once on the next tick; a job
//! still live from the last slot isn't stacked (dedupe key per schedule).

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::{sync::watch, task::JoinHandle};

use crate::jobs::repo::{self, NewJob};

/// Session-independent key of the scheduler's transaction-scoped advisory lock.
const LOCK_KEY: i64 = 0x6765_6e65_7379_0007;
const TICK: Duration = Duration::from_secs(30);

/// How often a schedule is due. Times are UTC (no DST shift handling).
#[derive(Debug, Clone, Copy)]
pub enum Every {
    /// On every multiple of this many minutes since the epoch.
    Minutes(u32),
    /// Once a day at this UTC time.
    DailyAt {
        /// UTC hour, 0..24.
        hour: u32,
        /// Minute of the hour, 0..60.
        minute: u32,
    },
}

impl Every {
    /// The latest due time at or before `now`.
    pub fn slot(self, now: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::Minutes(n) => {
                let step = i64::from(n.max(1)) * 60;
                let secs = now.timestamp();
                DateTime::from_timestamp(secs - secs.rem_euclid(step), 0).unwrap_or(now)
            }
            Self::DailyAt { hour, minute } => {
                let today = now
                    .date_naive()
                    .and_hms_opt(hour, minute, 0)
                    .map_or(now, |t| t.and_utc());
                if now >= today {
                    today
                } else {
                    today - chrono::Duration::days(1)
                }
            }
        }
    }
}

/// A recurring job: `kind` is enqueued under `name` whenever `every` passes a slot.
#[derive(Debug, Clone, Copy)]
pub struct Schedule {
    /// Slot name in `scheduled_runs`, also the dedupe key `schedule:<name>`.
    pub name: &'static str,
    /// Job kind to enqueue.
    pub kind: &'static str,
    /// When it is due.
    pub every: Every,
}

/// One tick. Returns the schedules it enqueued (empty when another instance holds the lock).
pub async fn tick(db: &PgPool, schedules: &[Schedule]) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut tx = db.begin().await?;
    let locked = sqlx::query_scalar!(
        r#"SELECT pg_try_advisory_xact_lock($1) AS "locked!""#,
        LOCK_KEY
    )
    .fetch_one(&mut *tx)
    .await?;
    if !locked {
        return Ok(Vec::new());
    }
    let now = sqlx::query_scalar!(r#"SELECT now() AS "now!""#)
        .fetch_one(&mut *tx)
        .await?;
    let names: Vec<&str> = schedules.iter().map(|s| s.name).collect();
    let slots: Vec<DateTime<Utc>> = schedules.iter().map(|s| s.every.slot(now)).collect();
    let fresh = sqlx::query_scalar!(
        "INSERT INTO scheduled_runs (name, slot)
         SELECT * FROM UNNEST($1::text[], $2::timestamptz[])
         ON CONFLICT DO NOTHING
         RETURNING name",
        &names as &[&str],
        &slots,
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut enqueued = Vec::new();
    for s in schedules
        .iter()
        .filter(|s| fresh.iter().any(|n| n == s.name))
    {
        let job = NewJob {
            dedupe_key: Some(format!("schedule:{}", s.name)),
            ..NewJob::new(s.kind)
        };
        repo::enqueue(&mut tx, &job).await?;
        enqueued.push(s.name);
    }
    repo::reap(&mut *tx).await?;
    tx.commit().await?;
    Ok(enqueued)
}

/// Drop slot rows older than 30 days; the newest slot of each schedule is far younger.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query!("DELETE FROM scheduled_runs WHERE created_at < now() - interval '30 days'")
            .execute(db)
            .await?
            .rows_affected(),
    )
}

/// Tick every 30 s until `stop` turns true. Failures are logged and retried next tick.
pub fn spawn(
    db: PgPool,
    schedules: &'static [Schedule],
    mut stop: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while !*stop.borrow() {
            match tick(&db, schedules).await {
                Ok(names) if !names.is_empty() => tracing::info!(?names, "scheduled jobs enqueued"),
                Ok(_) => {}
                Err(err) => tracing::warn!(error = %err, "scheduler tick failed"),
            }
            tokio::select! {
                _ = tokio::time::sleep(TICK) => {}
                _ = stop.changed() => {}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use sqlx::PgPool;

    use super::*;

    const SCHEDULES: [Schedule; 2] = [
        Schedule {
            name: "test.often",
            kind: "test.often",
            every: Every::Minutes(5),
        },
        Schedule {
            name: "test.nightly",
            kind: "test.nightly",
            every: Every::DailyAt {
                hour: 2,
                minute: 30,
            },
        },
    ];

    fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 23, h, m, s).unwrap()
    }

    async fn live_jobs(db: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status IN ('queued', 'running')")
            .fetch_one(db)
            .await
            .unwrap()
    }

    #[test]
    fn slots_round_down_to_the_last_due_time() {
        assert_eq!(Every::Minutes(5).slot(at(10, 7, 30)), at(10, 5, 0));
        assert_eq!(Every::Minutes(60).slot(at(10, 59, 59)), at(10, 0, 0));
        assert_eq!(Every::Minutes(60).slot(at(11, 0, 0)), at(11, 0, 0));
        let nightly = Every::DailyAt {
            hour: 2,
            minute: 30,
        };
        assert_eq!(nightly.slot(at(2, 30, 0)), at(2, 30, 0));
        assert_eq!(nightly.slot(at(23, 0, 0)), at(2, 30, 0));
        assert_eq!(
            nightly.slot(at(2, 29, 59)),
            Utc.with_ymd_and_hms(2026, 10, 22, 2, 30, 0).unwrap(),
            "before today's time: yesterday's slot"
        );
    }

    #[sqlx::test]
    async fn each_slot_is_enqueued_once(db: PgPool) {
        let first = tick(&db, &SCHEDULES).await.unwrap();
        let again = tick(&db, &SCHEDULES).await.unwrap();

        assert_eq!(first.len(), 2);
        assert!(again.is_empty(), "same slot, second tick");
        assert_eq!(live_jobs(&db).await, 2);
    }

    #[sqlx::test]
    async fn two_instances_ticking_together_enqueue_once(db: PgPool) {
        let ticks: Vec<_> = (0..4)
            .map(|_| {
                let db = db.clone();
                tokio::spawn(async move { tick(&db, &SCHEDULES).await.unwrap().len() })
            })
            .collect();
        let mut total = 0;
        for t in ticks {
            total += t.await.unwrap();
        }
        assert_eq!(total, 2);
        assert_eq!(live_jobs(&db).await, 2);
    }

    #[sqlx::test]
    async fn a_slow_job_is_not_stacked_by_the_next_slot(db: PgPool) {
        tick(&db, &SCHEDULES).await.unwrap();
        // Next slot arrives while the jobs are still queued.
        sqlx::query("DELETE FROM scheduled_runs")
            .execute(&db)
            .await
            .unwrap();

        assert_eq!(tick(&db, &SCHEDULES).await.unwrap().len(), 2);
        assert_eq!(
            live_jobs(&db).await,
            2,
            "dedupe keeps one live job per schedule"
        );
    }

    #[sqlx::test]
    async fn tick_reaps_expired_leases(db: PgPool) {
        let mut conn = db.acquire().await.unwrap();
        let id = repo::enqueue(&mut conn, &NewJob::new("test.work"))
            .await
            .unwrap();
        sqlx::query(
            "UPDATE jobs SET status = 'running', attempts = 1, locked_by = uuidv7(),
                 locked_until = now() - interval '1 second' WHERE id = $1",
        )
        .bind(id)
        .execute(&db)
        .await
        .unwrap();

        tick(&db, &[]).await.unwrap();

        let state: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(state, "queued");
    }

    #[sqlx::test]
    async fn prune_drops_slots_older_than_30_days(db: PgPool) {
        sqlx::query(
            "INSERT INTO scheduled_runs (name, slot, created_at) VALUES
                 ('old', now(), now() - interval '31 days'), ('new', now(), now())",
        )
        .execute(&db)
        .await
        .unwrap();
        assert_eq!(prune(&db).await.unwrap(), 1);
    }
}
