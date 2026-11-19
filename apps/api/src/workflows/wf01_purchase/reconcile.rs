//! `wf01_reconcile` (C4b, R3-§6c): only presence is evidence. Ids from created attempts and
//! stale-worker events are read back first, then the externalId lookup. A miss is never proof of
//! absence (facts N2 l.202-206): re-check with backoff, then leave it to a human. Never POSTs.

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    conn_ctx,
    execute::{Job, class_str, load},
    verify,
};
use crate::{
    error::AppError,
    integrations::{
        capability::{ErrorClass, ProviderError, PurchaseOrderSink},
        connections,
        providers::SinkClient,
    },
    jobs::{
        kinds::{Ctx, WF01_RECONCILE},
        repo::{Claimed, NewJob},
        runner::Step,
    },
    workflows::{
        events::{self, Actor},
        incidents,
    },
};

/// `wf01_reconcile` job (contracts §6).
pub fn job(ws: Uuid, execution_id: Uuid) -> NewJob {
    NewJob {
        workspace_id: Some(ws),
        payload: json!({"execution_id": execution_id}),
        dedupe_key: Some(format!("wf01_reconcile:{execution_id}")),
        max_attempts: 10,
        ..NewJob::new(WF01_RECONCILE)
    }
}

/// Job entry: build the target client, then reconcile.
pub async fn run(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let j = Job::of(job)?;
    let Some(e) = load(&ctx.db, j.ws, j.execution_id).await? else {
        return Ok(Step::Done);
    };
    let tgt = connections::load(&ctx.db, j.ws, e.target_connection_id).await?;
    let sink = SinkClient::for_connection(&conn_ctx(ctx)?, &tgt);
    reconcile(&ctx.db, &j, sink.as_ref().map_err(Clone::clone)).await
}

/// One reconcile check. `Err` (retryable) asks the rail for another check with backoff.
pub async fn reconcile(
    db: &PgPool,
    j: &Job,
    sink: Result<&SinkClient, ProviderError>,
) -> Result<Step, AppError> {
    let Some(e) = load(db, j.ws, j.execution_id).await? else {
        return Ok(Step::Done);
    };
    if e.status != "awaiting_reconciliation" {
        return Ok(Step::Done);
    }
    let sink = match sink {
        Ok(s) => s,
        Err(err) => return again(db, j, &e, err).await,
    };
    let ids = sqlx::query_scalar!(
        r#"SELECT evidence->>'target_id' AS "id!" FROM workflow_execution_attempts
           WHERE workspace_id = $1 AND execution_id = $2 AND evidence ? 'target_id'
           UNION
           SELECT payload->>'target_id' FROM workflow_events
           WHERE workspace_id = $1 AND execution_id = $2 AND kind = 'stale_worker_result'
             AND payload ? 'target_id'"#,
        j.ws,
        e.id,
    )
    .fetch_all(db)
    .await?;
    for id in ids {
        match sink.read(&id).await {
            Ok(po) => return found(db, j, &e, po).await,
            Err(err) if err.class == ErrorClass::NotFound => {}
            Err(err) => return again(db, j, &e, err).await,
        }
    }
    match sink.find_by_external_id(&e.correlation_id).await {
        Ok(Some(po)) => found(db, j, &e, po).await,
        Ok(None) => {
            let p = json!({"found": false, "check_no": j.attempt});
            events::record(db, j.ws, e.at(), "reconcile_check", p, Actor::System).await?;
            next(j, "not found yet")
        }
        Err(err) if err.class == ErrorClass::Conflict => {
            let mut tx = db.begin().await?;
            let d = json!({"detail": err.detail});
            incidents::open(
                &mut tx,
                j.ws,
                "target_conflict",
                &e.subject_ref,
                e.refs(),
                d,
            )
            .await?;
            let p = json!({"kind": "target_conflict"});
            events::record(&mut *tx, j.ws, e.at(), "incident_opened", p, Actor::System).await?;
            tx.commit().await?;
            Ok(Step::Done)
        }
        Err(err) => again(db, j, &e, err).await,
    }
}

async fn found(
    db: &PgPool,
    j: &Job,
    e: &super::execute::Loaded,
    po: crate::integrations::capability::TargetPo,
) -> Result<Step, AppError> {
    let p = json!({"found": true, "target_id": po.id, "check_no": j.attempt});
    events::record(db, j.ws, e.at(), "reconcile_check", p, Actor::System).await?;
    verify::settle(db, j, e, po).await
}

/// Lookup failed: record it, mark the connection on Auth/Permission, check again later.
async fn again(
    db: &PgPool,
    j: &Job,
    e: &super::execute::Loaded,
    err: ProviderError,
) -> Result<Step, AppError> {
    match err.class {
        ErrorClass::Auth => {
            connections::mark(
                db,
                j.ws,
                e.target_connection_id,
                "unauthorized",
                Some("auth_failed"),
            )
            .await?
        }
        ErrorClass::Permission => {
            connections::mark(
                db,
                j.ws,
                e.target_connection_id,
                "degraded",
                Some("permission_denied"),
            )
            .await?
        }
        _ => {}
    }
    let p = json!({"found": null, "error_class": class_str(err.class), "check_no": j.attempt});
    events::record(db, j.ws, e.at(), "reconcile_check", p, Actor::System).await?;
    next(j, "lookup failed")
}

/// Another check while the job has attempts left; after the last one a human decides.
fn next(j: &Job, why: &str) -> Result<Step, AppError> {
    if j.attempt >= j.max_attempts {
        return Ok(Step::Done);
    }
    Err(AppError::Internal(format!("wf01 reconcile: {why}")))
}
