//! `wf03_health` (contracts §11.5): every 15 minutes probe each non-disconnected connection;
//! a failed sign-in or permission opens one incident and changes the connection status.

use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;

use super::category::{classify, code, sentence};
use crate::{
    error::AppError,
    integrations::{
        capability::{CapabilityCheck, ErrorClass},
        connections::repo::{self, ConnectionRow},
        probe,
    },
    jobs::{kinds::Ctx, runner::Step},
    workflows::{
        incidents::{self, IncidentRefs},
        wf01_purchase::conn_ctx,
    },
};

/// Job entry (system job, all workspaces; every write carries the row's workspace).
pub async fn run(ctx: &Ctx) -> Result<Step, AppError> {
    let cc = conn_ctx(ctx)?;
    let rows = sqlx::query_as!(
        ConnectionRow,
        "SELECT id, workspace_id, provider, label, config, status, status_reason,
                credential_expires_at, last_probe, last_probe_at, created_at, updated_at
         FROM integration_connections WHERE status <> 'disconnected' ORDER BY id"
    )
    .fetch_all(&ctx.db)
    .await?;
    for row in rows {
        let checks = probe::run(&cc, &row).await;
        record(&ctx.db, &row, &checks).await?;
    }
    Ok(Step::Done)
}

/// Store the probe like `connections::test`, then open `auth_failure` / `permission_denied`.
pub async fn record(
    db: &PgPool,
    row: &ConnectionRow,
    checks: &[CapabilityCheck],
) -> Result<(), AppError> {
    let (status, reason) = probe::status(checks, row.credential_expires_at, Utc::now());
    let stored = serde_json::to_value(checks).map_err(|e| AppError::Internal(e.to_string()))?;
    let mut tx = db.begin().await?;
    repo::set_probe(
        &mut *tx,
        row.workspace_id,
        row.id,
        status,
        reason.as_deref(),
        &stored,
    )
    .await?;
    let failed = [ErrorClass::Auth, ErrorClass::Permission]
        .into_iter()
        .find_map(|class| {
            checks
                .iter()
                .find(|c| !c.ok && c.error_class == Some(class))
        });
    if let Some(c) = failed {
        let class = c.error_class.unwrap_or(ErrorClass::Unknown);
        let k = classify(class, None, "");
        let subject = format!("connection:{}", row.id);
        let d = json!({
            "code": code(class),
            "sentence": sentence(k.kind, k.category),
            "records": [{"subject_ref": subject, "target_id": null}],
            "last_attempt": null,
        });
        let refs = IncidentRefs {
            connection_id: Some(row.id),
            ..IncidentRefs::default()
        };
        incidents::open_classified(
            &mut tx,
            row.workspace_id,
            k.kind,
            &subject,
            refs,
            json!({"key": c.key}),
            Some((k.category, d)),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
