//! Human "link existing PO" (R3-§6c, C4b core of the C2 resolve seam): read the PO, verify it
//! against the approved snapshot. Match → `succeeded` (verified); mismatch →
//! `resolved_manual(linked_with_differences)`, never shown as verified. Runs in the resolve
//! transaction, under its row lock.

use std::collections::BTreeMap;

use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    execute::load,
    verify::{self, Verdict},
};
use crate::{
    AppState,
    error::AppError,
    integrations::{
        capability::{ErrorClass, PurchaseOrderSink},
        connections,
        providers::SinkClient,
    },
};

fn unreachable() -> AppError {
    AppError::Unavailable("NetSuite could not be reached. Try again later.")
}

/// C2 resolve seam: build the execution's target client, then `link`.
pub async fn link_existing(
    state: &AppState,
    tx: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    target_id: &str,
) -> Result<&'static str, AppError> {
    let e = load(&mut *tx, ws, id).await?.ok_or(AppError::NotFound)?;
    let row = connections::load(&mut *tx, ws, e.target_connection_id).await?;
    let sink = SinkClient::for_connection(&connections::service::ctx(state), &row)
        .map_err(|_| unreachable())?;
    link(tx, &sink, ws, id, target_id).await
}

/// Link `target_id` to an `awaiting_reconciliation` execution. Returns the resolution outcome.
pub async fn link(
    tx: &mut PgConnection,
    sink: &SinkClient,
    ws: Uuid,
    id: Uuid,
    target_id: &str,
) -> Result<&'static str, AppError> {
    let e = load(&mut *tx, ws, id).await?.ok_or(AppError::NotFound)?;
    let po = match sink.read(target_id).await {
        Ok(po) => po,
        Err(err) if matches!(err.class, ErrorClass::NotFound | ErrorClass::Validation) => {
            let msg = "No purchase order with this id could be read in NetSuite.".to_owned();
            return Err(AppError::Validation(BTreeMap::from([(
                "target_id".to_owned(),
                vec![msg],
            )])));
        }
        Err(_) => return Err(unreachable()),
    };
    match verify::compare(&mut *tx, ws, &e, &po).await? {
        v @ (Verdict::Match | Verdict::Variance) => {
            let variance = v == Verdict::Variance;
            if !verify::succeed(&mut *tx, ws, Uuid::nil(), &e, &po.id, variance).await? {
                return Err(AppError::Internal("execution moved during link".into()));
            }
            Ok(if variance {
                "succeeded_with_variance"
            } else {
                "succeeded"
            })
        }
        Verdict::Mismatch(fields) => {
            sqlx::query!(
                "UPDATE workflow_executions
                 SET status = 'resolved_manual', resolution = 'linked_with_differences',
                     reason = 'linked_with_differences', target_ref = $3, phase = 'done',
                     error_class = NULL, error_detail = NULL, updated_at = now()
                 WHERE workspace_id = $1 AND id = $2 AND status = 'awaiting_reconciliation'",
                ws,
                id,
                e.target_ref(&po.id),
            )
            .execute(&mut *tx)
            .await?;
            verify::flag_mismatch(&mut *tx, ws, &e, &po.id, &fields, false).await?;
            Ok("resolved_manual:linked_with_differences")
        }
    }
}
