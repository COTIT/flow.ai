//! WF-01 pair activation check (R3-§2, contracts §5.2): every mapped Odoo company must have
//! `po_double_validation = two_step` and amount 0, else 422 `source_controls_unmet`. A pair
//! with no mapped company is refused too: nothing would be governed.

use axum::http::StatusCode;
use sqlx::PgConnection;

use super::unmet;
use crate::{
    error::AppError,
    integrations::{
        capability::PurchaseRequestSource,
        connections,
        pairs::repo::PairRow,
        providers::{ConnCtx, SourceClient},
    },
};

fn refuse(message: String) -> AppError {
    AppError::Reason {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "source_controls_unmet",
        message,
    }
}

/// Called from `pairs::service::activate` inside its transaction.
pub async fn check(conn: &ConnCtx, tx: &mut PgConnection, pair: &PairRow) -> Result<(), AppError> {
    if pair.workflow_key != super::evaluate::WORKFLOW_KEY {
        return Ok(());
    }
    let companies = sqlx::query!(
        "SELECT source_id, source_label FROM integration_mappings
         WHERE workspace_id = $1 AND pair_id = $2 AND entity = 'company' ORDER BY source_id",
        pair.workspace_id,
        pair.id,
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| (r.source_id, r.source_label))
    .collect::<Vec<_>>();
    let source = connections::load(&mut *tx, pair.workspace_id, pair.source_connection_id).await?;
    let client = SourceClient::for_connection(conn, &source)
        .map_err(|_| refuse("Odoo purchase approval settings could not be read.".into()))?;
    verify(&client, &companies).await
}

/// The rule on `(company id, label)` pairs.
pub async fn verify(
    client: &SourceClient,
    companies: &[(String, Option<String>)],
) -> Result<(), AppError> {
    if companies.is_empty() {
        return Err(refuse(
            "Map at least one Odoo company before activating.".into(),
        ));
    }
    let ids: Vec<String> = companies.iter().map(|(id, _)| id.clone()).collect();
    let controls = client
        .read_controls(&ids)
        .await
        .map_err(|_| refuse("Odoo purchase approval settings could not be read.".into()))?;
    let bad = unmet(&ids, &controls);
    if bad.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = companies
        .iter()
        .filter(|(id, _)| bad.contains(id))
        .map(|(id, label)| {
            label
                .clone()
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| id.clone())
        })
        .collect();
    Err(refuse(format!(
        "Odoo purchase approval settings do not meet the requirement for: {}.",
        names.join(", ")
    )))
}
