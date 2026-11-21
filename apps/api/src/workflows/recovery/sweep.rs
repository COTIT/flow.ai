//! `wf03_expect_sweep` (contracts §11.5): every 5 minutes, one read-back per due expectation,
//! at most 10 per row. Never writes to the target and never moves an execution.

use sqlx::PgPool;

use super::{
    category::{classify, code, permanent},
    expectations::{self, Row},
};
use crate::{
    error::AppError,
    integrations::{capability::ProviderError, connections, providers::SinkClient},
    jobs::{kinds::Ctx, repo as jobs, runner::Step},
    workflows::wf01_purchase::{
        conn_ctx, reconcile,
        verify::{Readback, readback},
    },
};

/// Rows per run; the rest wait for the next run.
const BATCH: i64 = 100;

/// What one check did.
#[derive(Debug, PartialEq, Eq)]
pub enum Checked {
    Met,
    Late,
    Failed,
    /// Transient answer (e.g. 429): nothing changed, back off.
    Busy,
}

/// Job entry: sinks from each row's execution pair (decision B).
pub async fn run(ctx: &Ctx) -> Result<Step, AppError> {
    let cc = conn_ctx(ctx)?;
    for r in expectations::due(&ctx.db, BATCH).await? {
        let Some(conn) = target_of(&ctx.db, &r).await? else {
            continue;
        };
        let row = connections::load(&ctx.db, r.workspace_id, conn).await?;
        let sink = SinkClient::for_connection(&cc, &row);
        if check(&ctx.db, &r, sink.as_ref().map_err(Clone::clone), true).await? == Checked::Busy {
            return Err(AppError::Internal("wf03 sweep: target busy".into()));
        }
    }
    Ok(Step::Done)
}

/// Target connection of a WF-01 expectation: the execution's pair.
async fn target_of(db: &PgPool, r: &Row) -> Result<Option<uuid::Uuid>, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT p.target_connection_id
         FROM workflow_executions e
         JOIN approval_requests q ON q.workspace_id = e.workspace_id AND q.id = e.approval_request_id
         JOIN integration_pairs p ON p.workspace_id = q.workspace_id AND p.id = q.pair_id
         WHERE e.workspace_id = $1 AND e.id = $2",
        r.workspace_id,
        r.execution_id,
    )
    .fetch_optional(db)
    .await?)
}

/// One sweep with one sink (tests). Stops at the first `Busy`.
#[cfg(test)]
pub async fn sweep_with(db: &PgPool, sink: &SinkClient) -> Result<Vec<Checked>, AppError> {
    let mut out = vec![];
    for r in expectations::due(db, BATCH).await? {
        let c = check(db, &r, Ok(sink), true).await?;
        let busy = c == Checked::Busy;
        out.push(c);
        if busy {
            break;
        }
    }
    Ok(out)
}

/// Check one expectation now. `spend` false for a person's Verify (it skips the budget).
pub async fn check(
    db: &PgPool,
    r: &Row,
    sink: Result<&SinkClient, ProviderError>,
    spend: bool,
) -> Result<Checked, AppError> {
    let Some(exec) = r.execution_id else {
        return Ok(Checked::Busy);
    };
    // The writer already verified it: nothing to call.
    let status = sqlx::query_scalar!(
        "SELECT status FROM workflow_executions WHERE workspace_id = $1 AND id = $2",
        r.workspace_id,
        exec,
    )
    .fetch_one(db)
    .await?;
    if status == "succeeded" || status == "succeeded_with_variance" {
        let mut tx = db.begin().await?;
        expectations::met(&mut tx, r, None).await?;
        tx.commit().await?;
        return Ok(Checked::Met);
    }
    expectations::checked(db, r.workspace_id, r.id, spend).await?;
    let found = match sink {
        Ok(s) => readback(db, r.workspace_id, exec, s).await?,
        Err(e) => Readback::Failed(e),
    };
    let mut tx = db.begin().await?;
    let out = match found {
        Readback::Matched(po) => {
            expectations::met(&mut tx, r, Some(&po)).await?;
            if status == "awaiting_reconciliation" {
                // WF-01 settles its own run.
                jobs::enqueue(&mut tx, &reconcile::job(r.workspace_id, exec)).await?;
            }
            Checked::Met
        }
        Readback::Differs(incident) => {
            let c = classify(
                crate::integrations::capability::ErrorClass::Conflict,
                None,
                "",
            );
            expectations::fail(&mut tx, r, c, "conflict", Some(incident)).await?;
            Checked::Failed
        }
        Readback::Missing => {
            let latest = sqlx::query_scalar!(
                "SELECT outcome FROM workflow_execution_attempts
                 WHERE workspace_id = $1 AND execution_id = $2 ORDER BY attempt_no DESC LIMIT 1",
                r.workspace_id,
                exec,
            )
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            let category = match latest.as_deref() {
                None | Some("ambiguous") => "unknown_outcome",
                _ => "missing",
            };
            expectations::late(&mut tx, r, category).await?;
            Checked::Late
        }
        Readback::Failed(err) if permanent(err.class) => {
            let c = classify(err.class, err.http_status, &err.detail);
            if let Some(conn) = target_of(db, r).await? {
                match c.kind {
                    "auth_failure" => {
                        connections::mark(
                            &mut *tx,
                            r.workspace_id,
                            conn,
                            "unauthorized",
                            Some("auth_failed"),
                        )
                        .await?
                    }
                    "permission_denied" => {
                        connections::mark(
                            &mut *tx,
                            r.workspace_id,
                            conn,
                            "degraded",
                            Some("permission_denied"),
                        )
                        .await?
                    }
                    _ => {}
                }
            }
            expectations::fail(&mut tx, r, c, code(err.class), None).await?;
            Checked::Failed
        }
        Readback::Failed(_) => Checked::Busy,
    };
    tx.commit().await?;
    Ok(out)
}
