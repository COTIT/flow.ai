//! Pair rules: a source (Odoo) and target (NetSuite) connection of the workspace, config with
//! defaults, activation only with both connections active, pause.

use axum::http::StatusCode;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::Rules,
    integrations::{
        connections::{self, service::parse},
        pairs::{
            dto::{Input, check_config, default_config},
            repo::{self, PairRow},
        },
    },
};

/// Every active pair, all workspaces.
pub async fn load_active(db: impl sqlx::PgExecutor<'_>) -> Result<Vec<PairRow>, AppError> {
    Ok(repo::active(db).await?)
}

/// Set status + paused reason (§3.7). 404 when the pair isn't the workspace's.
pub async fn set_status(
    db: impl sqlx::PgExecutor<'_>,
    ws: Uuid,
    pair: Uuid,
    status: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    if repo::set_status(db, ws, pair, status, reason).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// One page of pairs.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<PairRow>, i64), AppError> {
    Ok(repo::list(&state.db, ws, limit, offset).await?)
}

/// The pair, or 404.
pub async fn get(state: &AppState, ws: Uuid, id: &str) -> Result<PairRow, AppError> {
    repo::find(&state.db, ws, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// The connection `id` of the workspace when it has `provider`.
async fn side(
    state: &AppState,
    ws: Uuid,
    id: Option<&str>,
    provider: &str,
    field: &str,
    r: &mut Rules,
) -> Option<Uuid> {
    let Some(id) = id else {
        r.missing(field);
        return None;
    };
    let found = match Uuid::parse_str(id.trim()) {
        Ok(uuid) => connections::repo::find(&state.db, ws, uuid, false)
            .await
            .ok()
            .flatten(),
        Err(_) => None,
    };
    let ok = found.filter(|c| c.provider == provider).map(|c| c.id);
    if ok.is_none() {
        r.add(
            field,
            format!("The selected {} is invalid.", field.replace('_', " ")),
        );
    }
    ok
}

/// New draft pair.
pub async fn create(
    state: &AppState,
    ws: Uuid,
    user: Uuid,
    input: Input,
) -> Result<Uuid, AppError> {
    let mut r = Rules::default();
    let source = side(
        state,
        ws,
        input.source.as_deref(),
        "odoo",
        "source_connection_id",
        &mut r,
    )
    .await;
    let target = side(
        state,
        ws,
        input.target.as_deref(),
        "netsuite",
        "target_connection_id",
        &mut r,
    )
    .await;
    let mut config = default_config();
    config.extend(input.config.unwrap_or_default());
    let config = check_config(&config, &mut r);
    r.into_result()?;
    let (Some(source), Some(target)) = (source, target) else {
        return Err(AppError::Internal("validated input incomplete".into()));
    };
    repo::insert(&state.db, ws, source, target, &config, user)
        .await
        .map_err(|e| {
            AppError::unique(
                e,
                "integration_pairs_key",
                "source_connection_id",
                "A pair for these connections already exists.",
            )
        })
}

/// Patch the config (merged into the stored one).
pub async fn update(state: &AppState, ws: Uuid, id: &str, input: Input) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let pair = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut r = Rules::default();
    if input.source.is_some() {
        r.add(
            "source_connection_id",
            "The connections of a pair can't be changed.",
        );
    }
    if input.target.is_some() {
        r.add(
            "target_connection_id",
            "The connections of a pair can't be changed.",
        );
    }
    let mut config = default_config();
    config.extend(pair.config.as_object().cloned().unwrap_or_default());
    config.extend(input.config.unwrap_or_default());
    let config = check_config(&config, &mut r);
    r.into_result()?;
    repo::set_config(&mut *tx, ws, id, &config).await?;
    tx.commit().await?;
    Ok(())
}

/// Activate: both connections must be `active`.
pub async fn activate(state: &AppState, ws: Uuid, id: &str) -> Result<PairRow, AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let pair = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    for conn in [pair.source_connection_id, pair.target_connection_id] {
        let c = connections::load(&mut *tx, ws, conn).await?;
        if c.status != "active" {
            return Err(AppError::Reason {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "connection_not_active",
                message: "Both connections must be active first.".into(),
            });
        }
    }
    // C4a: WF-01 activation check (`source_controls_unmet`) is called here.
    repo::set_status(&mut *tx, ws, id, "active", None).await?;
    let pair = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(pair)
}

/// Pause by hand (`paused_reason = manual`).
pub async fn pause(state: &AppState, ws: Uuid, id: &str) -> Result<PairRow, AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    repo::set_status(&mut *tx, ws, id, "paused", Some("manual")).await?;
    let pair = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(pair)
}
