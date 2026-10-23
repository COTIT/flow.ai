//! The job kinds this build runs, their handlers and their schedules. A new kind is a const
//! here, a match arm in `dispatch` and, when recurring, a `SCHEDULES` entry.

use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    cleanup,
    error::AppError,
    jobs::{
        repo::Claimed,
        runner::{Handler, Step},
        scheduler::{Every, Schedule},
    },
    modules::{files, messages::repo as messages},
    storage::Store,
};

/// Marks email sends a dead request left open as `unknown` (every 5 min).
pub const CLOSE_STUCK_SENDS: &str = "close_stuck_sends";
/// Deletes stale rows: tokens, invites, login quotas, old jobs and slots (hourly).
pub const PRUNE: &str = "prune";
/// Finishes file rows a crash left half written or half deleted (hourly).
pub const FILE_SWEEP: &str = "file_sweep";

/// Every kind this build can run; the claim never takes another.
pub const KINDS: &[&str] = &[CLOSE_STUCK_SENDS, PRUNE, FILE_SWEEP];

/// Recurring jobs.
pub static SCHEDULES: [Schedule; 3] = [
    Schedule {
        name: CLOSE_STUCK_SENDS,
        kind: CLOSE_STUCK_SENDS,
        every: Every::Minutes(5),
    },
    Schedule {
        name: PRUNE,
        kind: PRUNE,
        every: Every::Minutes(60),
    },
    Schedule {
        name: FILE_SWEEP,
        kind: FILE_SWEEP,
        every: Every::Minutes(60),
    },
];

/// What handlers reach: the job pool and, when configured, the file bucket.
#[derive(Clone)]
pub struct Ctx {
    pub db: PgPool,
    pub files: Option<Store>,
}

/// The runner's handler over `dispatch`.
pub fn handler(ctx: Ctx) -> Handler {
    Arc::new(move |job| {
        let ctx = ctx.clone();
        Box::pin(async move { dispatch(&ctx, job).await })
    })
}

/// Run one job (or one chunk) of a known kind.
pub async fn dispatch(ctx: &Ctx, job: Claimed) -> Result<Step, AppError> {
    match job.kind.as_str() {
        CLOSE_STUCK_SENDS => {
            messages::close_stuck(&ctx.db).await?;
        }
        PRUNE => {
            let pruned = cleanup::prune(&ctx.db).await?;
            tracing::info!(?pruned, "prune done");
        }
        FILE_SWEEP => {
            if let Some(bucket) = &ctx.files {
                let swept = files::service::sweep(&ctx.db, bucket).await?;
                tracing::info!(files = swept, "file sweep done");
            }
        }
        // The claim only takes `KINDS`, so this means a kind was listed without an arm.
        other => {
            return Err(AppError::Internal(format!(
                "no handler for job kind {other}"
            )));
        }
    }
    Ok(Step::Done)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::*;

    fn claimed(kind: &str) -> Claimed {
        Claimed {
            id: Uuid::now_v7(),
            kind: kind.into(),
            workspace_id: None,
            payload: json!({}),
            attempts: 1,
            max_attempts: 5,
            created_by: None,
            token: Uuid::now_v7(),
        }
    }

    fn ctx(db: &PgPool) -> Ctx {
        Ctx {
            db: db.clone(),
            files: None,
        }
    }

    async fn count(db: &PgPool, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(db).await.unwrap()
    }

    #[test]
    fn every_schedule_runs_a_kind_this_build_knows() {
        let names: HashSet<_> = SCHEDULES.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), SCHEDULES.len(), "schedule names are unique");
        assert!(SCHEDULES.iter().all(|s| KINDS.contains(&s.kind)));
    }

    #[sqlx::test]
    async fn prune_kind_also_prunes_jobs_and_slots(db: PgPool) {
        sqlx::query(
            "INSERT INTO jobs (kind, status, finished_at) VALUES ('x', 'succeeded', now() - interval '8 days')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO scheduled_runs (name, slot, created_at) VALUES ('x', now(), now() - interval '31 days')",
        )
        .execute(&db)
        .await
        .unwrap();

        assert_eq!(
            dispatch(&ctx(&db), claimed(PRUNE)).await.unwrap(),
            Step::Done
        );

        assert_eq!(count(&db, "SELECT count(*) FROM jobs").await, 0);
        assert_eq!(count(&db, "SELECT count(*) FROM scheduled_runs").await, 0);
    }

    #[sqlx::test]
    async fn close_stuck_sends_kind_closes_open_sends(db: PgPool) {
        sqlx::query(
            "WITH u AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ('o@x.io', 'O', 'W', 'h') RETURNING id),
                  w AS (INSERT INTO workspaces (name, created_by_user_id)
                        SELECT 'acme', id FROM u RETURNING id)
             INSERT INTO email_send_logs (id, workspace_id, provider, from_email, subject, updated_at)
             SELECT uuidv7(), id, 'smtp', 'a@b.co', 's', now() - interval '11 minutes' FROM w",
        )
        .execute(&db)
        .await
        .unwrap();

        assert_eq!(
            dispatch(&ctx(&db), claimed(CLOSE_STUCK_SENDS))
                .await
                .unwrap(),
            Step::Done
        );

        let open = count(
            &db,
            "SELECT count(*) FROM email_send_logs WHERE status IN ('pending', 'sending')",
        )
        .await;
        assert_eq!(open, 0);
    }

    #[sqlx::test]
    async fn file_sweep_without_a_bucket_does_nothing(db: PgPool) {
        assert_eq!(
            dispatch(&ctx(&db), claimed(FILE_SWEEP)).await.unwrap(),
            Step::Done
        );
    }
}
