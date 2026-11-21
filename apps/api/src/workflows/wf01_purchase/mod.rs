//! WF-01 Odoo RFQ → Flow approval → NetSuite PO. C4a intake: tick fan-out, poll, sweep, full
//! scan, evaluate (mappings, money, requester) into approval requests, O5 control checks. C4b
//! execute, reconcile, verify, link: one POST per granted attempt, read-back verify,
//! reconciliation. Intake jobs carry `{"pair_id"}`, execution jobs `{"execution_id"}`.

pub mod activation;
pub mod evaluate;
pub mod execute;
pub mod fingerprint;
pub mod full_scan;
pub mod link;
pub mod money;
pub mod poll;
pub mod reconcile;
pub mod sweep;
pub mod tick;
pub(crate) mod verify;

#[cfg(test)]
mod exec_tests;
#[cfg(test)]
mod fake_netsuite;
#[cfg(test)]
mod fake_odoo;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wf03_tests;

use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AppError,
    integrations::{
        Runtime,
        capability::{ProviderError, SourceControls},
        connections::{self, repo::ConnectionRow},
        pairs::{repo as pairs, service as pair_service},
        providers::{ConnCtx, Mode, SourceClient},
    },
    jobs::{kinds::Ctx, repo::Claimed},
    workflows::incidents::{self, IncidentRefs},
};

use evaluate::PairCtx;

/// Statuses a connection may be used in.
fn usable(status: &str) -> bool {
    matches!(status, "active" | "degraded")
}

/// Pair id from a WF-01 job payload, with its workspace.
fn job_pair(job: &Claimed) -> Result<(Uuid, Uuid), AppError> {
    let ws = job
        .workspace_id
        .ok_or_else(|| AppError::Internal("wf01 job without workspace".into()))?;
    let pair = job
        .payload
        .get("pair_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Internal("wf01 job without pair_id".into()))?;
    Ok((ws, pair))
}

/// Connection with its expiry applied (contracts §3.6: checked on every job start).
async fn connection(db: &PgPool, ws: Uuid, id: Uuid) -> Result<ConnectionRow, AppError> {
    let mut row = connections::load(db, ws, id).await?;
    if let Some((status, reason)) = connections::service::expiry_status(&row)
        && row.status != status
        && row.status != "disconnected"
    {
        connections::mark(db, ws, id, status, Some(reason)).await?;
        row.status = status.into();
    }
    Ok(row)
}

/// A pair ready to run: active, source usable. None means "skip this run".
pub struct Ready {
    pub pair: PairCtx,
    pub source: ConnectionRow,
}

/// Load the pair for a job; None when it is not active or its source can't be used.
pub async fn ready(db: &PgPool, ws: Uuid, pair_id: Uuid) -> Result<Option<Ready>, AppError> {
    let Some(row) = pairs::find(db, ws, pair_id).await? else {
        return Ok(None);
    };
    if row.status != "active" {
        return Ok(None);
    }
    let source = connection(db, ws, row.source_connection_id).await?;
    if !usable(&source.status) {
        return Ok(None);
    }
    let target = connection(db, ws, row.target_connection_id).await?;
    Ok(Some(Ready {
        pair: PairCtx::new(&row, usable(&target.status)),
        source,
    }))
}

/// Provider context from the job context (mode lives in the app state the runner carries).
pub fn conn_ctx(ctx: &Ctx) -> Result<ConnCtx, AppError> {
    let runtime = match &ctx.invoice_state {
        Some(state) => state.integrations.clone(),
        None => Runtime::new(Mode::Live).map_err(AppError::Internal)?,
    };
    Ok(ConnCtx {
        outbound: runtime.outbound,
        credentials: ctx.credentials.clone(),
        db: ctx.db.clone(),
        mode: runtime.mode,
    })
}

/// Source client of a ready pair.
pub fn source_client(ctx: &Ctx, ready: &Ready) -> Result<SourceClient, AppError> {
    SourceClient::for_connection(&conn_ctx(ctx)?, &ready.source).map_err(provider)
}

/// A provider failure fails the job (the rail retries with backoff); detail is secret-free.
pub fn provider(e: ProviderError) -> AppError {
    AppError::Internal(format!("wf01 source call failed: {e}"))
}

/// Mapped Odoo company ids of the pair.
pub async fn companies(
    db: impl sqlx::PgExecutor<'_>,
    ws: Uuid,
    pair: Uuid,
) -> Result<Vec<String>, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT source_id FROM integration_mappings
         WHERE workspace_id = $1 AND pair_id = $2 AND entity = 'company' ORDER BY source_id",
        ws,
        pair,
    )
    .fetch_all(db)
    .await?)
}

/// Companies whose O5 controls are not `two_step` with amount 0, or that the source didn't
/// return (hidden by record rules). Activation refuses an empty list itself.
pub fn unmet(companies: &[String], controls: &[SourceControls]) -> Vec<String> {
    companies
        .iter()
        .filter(|c| {
            !controls
                .iter()
                .any(|k| &k.company_id == *c && k.two_step && k.threshold_zero)
        })
        .cloned()
        .collect()
}

/// R3-§2 drift: pause the pair and open one `odoo_control_drift` incident per company.
pub async fn pause_for_drift(db: &PgPool, pair: &PairCtx, bad: &[String]) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    pair_service::set_status(
        &mut *tx,
        pair.ws,
        pair.id,
        "paused",
        Some("odoo_control_drift"),
    )
    .await?;
    for company in bad {
        incidents::open(
            &mut tx,
            pair.ws,
            "odoo_control_drift",
            &format!("odoo:{}:res.company:{company}", pair.source),
            IncidentRefs {
                pair_id: Some(pair.id),
                connection_id: Some(pair.source),
                ..IncidentRefs::default()
            },
            json!({"company_id": company, "required": {"po_double_validation": "two_step",
                   "po_double_validation_amount": 0}}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Stamp `last_polled_at` / `last_swept_at` / `last_full_scan_at`.
pub async fn stamp(db: &PgPool, ws: Uuid, pair: Uuid, column: Stamp) -> Result<(), AppError> {
    match column {
        Stamp::Polled => {
            sqlx::query!(
                "UPDATE integration_pairs SET last_polled_at = now() WHERE workspace_id = $1 AND id = $2",
                ws,
                pair
            )
            .execute(db)
            .await?
        }
        Stamp::Swept => {
            sqlx::query!(
                "UPDATE integration_pairs SET last_swept_at = now() WHERE workspace_id = $1 AND id = $2",
                ws,
                pair
            )
            .execute(db)
            .await?
        }
        Stamp::FullScan => {
            sqlx::query!(
                "UPDATE integration_pairs SET last_full_scan_at = now() WHERE workspace_id = $1 AND id = $2",
                ws,
                pair
            )
            .execute(db)
            .await?
        }
    };
    Ok(())
}

/// Which scheduler stamp a job sets.
#[derive(Debug, Clone, Copy)]
pub enum Stamp {
    Polled,
    Swept,
    FullScan,
}
