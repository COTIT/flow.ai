//! `wf01_tick` (contracts §6, O-9): a system job every minute that enqueues the due per-pair
//! workspace jobs. Same fan-out as `webhook_sweep` (`modules/webhooks/deliver.rs:204-217`):
//! `NewJob { workspace_id: Some(ws), dedupe_key, concurrency_key }`, so a slot that is still
//! queued or running is not stacked. Poll, sweep and full scan of one pair share the
//! concurrency key `wf01:<pair>` and never overlap. C4b adds the queued-execution backstop as
//! `tick::enqueue_queued_executions`.

use serde_json::json;
use sqlx::PgPool;

use crate::{
    error::AppError,
    jobs::{
        kinds::{WF01_FULL_SCAN, WF01_POLL, WF01_SWEEP},
        repo::{self as jobs, NewJob},
        runner::Step,
    },
};

/// Job entry.
pub async fn run(db: &PgPool) -> Result<Step, AppError> {
    fan_out(db).await?;
    Ok(Step::Done)
}

/// Enqueue poll/sweep (every `poll_minutes`) and full scan (every `full_scan_minutes`) for
/// active WF-01 pairs whose last run is that old. Returns the jobs enqueued.
pub async fn fan_out(db: &PgPool) -> Result<usize, AppError> {
    let mut tx = db.begin().await?;
    let due = sqlx::query!(
        r#"SELECT id, workspace_id,
             (last_polled_at IS NULL OR last_polled_at + make_interval(
                 mins => coalesce((config->>'poll_minutes')::int, 5)) <= now()) AS "poll!",
             (last_swept_at IS NULL OR last_swept_at + make_interval(
                 mins => coalesce((config->>'poll_minutes')::int, 5)) <= now()) AS "sweep!",
             (last_full_scan_at IS NULL OR last_full_scan_at + make_interval(
                 mins => coalesce((config->>'full_scan_minutes')::int, 60)) <= now()) AS "scan!"
           FROM integration_pairs
           WHERE status = 'active' AND workflow_key = 'wf01_purchase'
           ORDER BY id"#
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut n = 0;
    for p in due {
        for (wanted, kind, name) in [
            (p.poll, WF01_POLL, "wf01_poll"),
            (p.sweep, WF01_SWEEP, "wf01_sweep"),
            (p.scan, WF01_FULL_SCAN, "wf01_full_scan"),
        ] {
            if !wanted {
                continue;
            }
            let job = NewJob {
                workspace_id: Some(p.workspace_id),
                payload: json!({"pair_id": p.id}),
                dedupe_key: Some(format!("{name}:{}", p.id)),
                concurrency_key: Some(format!("wf01:{}", p.id)),
                ..NewJob::new(kind)
            };
            jobs::enqueue(&mut tx, &job).await?;
            n += 1;
        }
    }
    tx.commit().await?;
    Ok(n)
}
