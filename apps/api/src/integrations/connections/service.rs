//! Connection rules: create, patch with write-only secrets (mask keeps, a moved host needs the
//! secret again), probe, disconnect, and the status writer `mark` used by other modules.

use chrono::Utc;

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::Rules,
    integrations::{
        connections::{
            dto::{Input, SecretChange, bound_key, check_config, secret_name},
            repo::{self, ConnectionRow},
            secrets,
        },
        probe,
        providers::ConnCtx,
    },
    modules::settings::credentials::{NO_KEY, RE_ENTER},
};

const LABEL_TAKEN: &str = "The label has already been taken.";

/// Path id → uuid; malformed is 404.
pub fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Provider context for jobs and probes.
pub fn ctx(state: &AppState) -> ConnCtx {
    ConnCtx {
        outbound: state.integrations.outbound.clone(),
        credentials: state.credentials.clone(),
        db: state.db.clone(),
        mode: state.integrations.mode.clone(),
    }
}

/// The connection, or 404.
pub async fn load(
    db: impl sqlx::PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<ConnectionRow, AppError> {
    repo::find(db, ws, id, false)
        .await?
        .ok_or(AppError::NotFound)
}

/// Set status + reason (§3.6). The only status writer outside the C1 routes.
pub async fn mark(
    db: impl sqlx::PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    status: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    if repo::mark(db, ws, id, status, reason).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// One page of connections with whether each has its secret stored.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<(ConnectionRow, bool)>, i64), AppError> {
    let (rows, total) = repo::list(&state.db, ws, limit, offset).await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let stored = secrets::names_for(&state.db, ws, &ids).await?;
    let rows = rows
        .into_iter()
        .map(|r| {
            let has = stored
                .iter()
                .any(|(id, name)| *id == r.id && name == secret_name(&r.provider));
            (r, has)
        })
        .collect();
    Ok((rows, total))
}

/// The connection and whether its secret is stored.
pub async fn get(state: &AppState, ws: Uuid, id: &str) -> Result<(ConnectionRow, bool), AppError> {
    let row = load(&state.db, ws, parse(id)?).await?;
    let stored = secrets::names(&state.db, ws, row.id).await?;
    let has = stored.iter().any(|n| n == secret_name(&row.provider));
    Ok((row, has))
}

/// Secrets to seal; refuses names the provider doesn't use, the mask where nothing may be
/// kept, and any secret when no server key is configured.
fn secret_rules(
    state: &AppState,
    provider: &str,
    changes: &[(String, SecretChange)],
    keepable: bool,
    r: &mut Rules,
) {
    for (name, change) in changes {
        let field = format!("secrets.{name}");
        if name != secret_name(provider) {
            r.add(&field, "The selected secret is invalid.");
            continue;
        }
        match change {
            SecretChange::Keep if !keepable => r.add(&field, RE_ENTER),
            SecretChange::Set(_) if state.credentials.is_none() => r.add(&field, NO_KEY),
            _ => {}
        }
    }
}

/// New draft connection.
pub async fn create(
    state: &AppState,
    ws: Uuid,
    user: Uuid,
    input: Input,
) -> Result<Uuid, AppError> {
    let mut r = Rules::default();
    let provider = r.required("provider", input.provider);
    let label = r.required("label", input.label);
    let config = match (&provider, &input.config) {
        (Some(p), Some(c)) => Some(check_config(p, c, &mut r)),
        (_, None) => {
            r.missing("config");
            None
        }
        _ => None,
    };
    if let Some(p) = &provider {
        secret_rules(state, p, &input.secrets, false, &mut r);
    }
    r.into_result()?;
    let (Some(provider), Some(label), Some(config)) = (provider, label, config) else {
        return Err(AppError::Internal("validated input incomplete".into()));
    };
    let expires = input.expires_at.flatten();
    let mut tx = state.db.begin().await?;
    let id = repo::insert(&mut *tx, ws, &provider, &label, &config, expires, user)
        .await
        .map_err(|e| {
            AppError::unique(e, "integration_connections_label_key", "label", LABEL_TAKEN)
        })?;
    seal(state, &mut tx, ws, id, user, &input.secrets).await?;
    tx.commit().await?;
    Ok(id)
}

async fn seal(
    state: &AppState,
    tx: &mut sqlx::PgConnection,
    ws: Uuid,
    id: Uuid,
    user: Uuid,
    changes: &[(String, SecretChange)],
) -> Result<(), AppError> {
    for (name, change) in changes {
        match change {
            SecretChange::Set(plain) => {
                let key = state
                    .credentials
                    .as_deref()
                    .ok_or_else(|| AppError::field(&format!("secrets.{name}"), NO_KEY))?;
                secrets::store(tx, key, ws, id, name, plain, user).await?;
            }
            SecretChange::Clear => secrets::clear(&mut *tx, ws, id).await?,
            SecretChange::Keep => {}
        }
    }
    Ok(())
}

/// Patch a connection. A new secret or config sends it back to `draft` (probe again).
pub async fn update(
    state: &AppState,
    ws: Uuid,
    user: Uuid,
    id: &str,
    input: Input,
) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let row = repo::find(&mut *tx, ws, id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut r = Rules::default();
    if input.provider.as_ref().is_some_and(|p| *p != row.provider) {
        r.add("provider", "The provider can't be changed.");
    }
    let config = match &input.config {
        Some(patch) => {
            let mut merged = row.config.as_object().cloned().unwrap_or_default();
            merged.extend(patch.clone());
            check_config(&row.provider, &merged, &mut r)
        }
        None => row.config.clone(),
    };
    let stored = secrets::names(&mut *tx, ws, id).await?;
    let name = secret_name(&row.provider);
    let has = stored.iter().any(|n| n == name);
    let bound = bound_key(&row.provider);
    let moved = config.get(bound) != row.config.get(bound);
    let sent = input
        .secrets
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, c)| c);
    // A kept secret may only go to the host it was entered for.
    if moved && has && sent.is_none() {
        r.add(&format!("secrets.{name}"), RE_ENTER);
    }
    secret_rules(state, &row.provider, &input.secrets, has && !moved, &mut r);
    r.into_result()?;

    let changed = config != row.config
        || input
            .secrets
            .iter()
            .any(|(_, c)| !matches!(c, SecretChange::Keep));
    let (status, reason) = if changed {
        ("draft", None)
    } else {
        (row.status.as_str(), row.status_reason.as_deref())
    };
    let label = input.label.unwrap_or(row.label.clone());
    let expires = input.expires_at.unwrap_or(row.credential_expires_at);
    repo::update(&mut *tx, ws, id, &label, &config, expires, status, reason)
        .await
        .map_err(|e| {
            AppError::unique(e, "integration_connections_label_key", "label", LABEL_TAKEN)
        })?;
    seal(state, &mut tx, ws, id, user, &input.secrets).await?;
    tx.commit().await?;
    Ok(())
}

/// Probe the connection, store the checks and the status they lead to. A disconnected
/// connection keeps its status.
pub async fn test(state: &AppState, ws: Uuid, id: &str) -> Result<(ConnectionRow, bool), AppError> {
    let row = load(&state.db, ws, parse(id)?).await?;
    let checks = probe::run(&ctx(state), &row).await;
    let (status, reason) = if row.status == "disconnected" {
        ("disconnected", row.status_reason.clone())
    } else {
        let (s, r) = probe::status(&checks, row.credential_expires_at, Utc::now());
        (s, r)
    };
    let checks = serde_json::to_value(&checks).map_err(|e| AppError::Internal(e.to_string()))?;
    repo::set_probe(&state.db, ws, row.id, status, reason.as_deref(), &checks).await?;
    get(state, ws, &row.id.to_string()).await
}

/// Disconnect: status `disconnected`, stored secrets dropped.
pub async fn disconnect(state: &AppState, ws: Uuid, id: &str) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    repo::find(&mut *tx, ws, id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    secrets::clear(&mut *tx, ws, id).await?;
    repo::mark(&mut *tx, ws, id, "disconnected", None).await?;
    tx.commit().await?;
    Ok(())
}

/// Status and reason a connection should have now from its expiry alone (job start, §3.6).
pub fn expiry_status(row: &ConnectionRow) -> Option<(&'static str, &'static str)> {
    probe::expiry(row.credential_expires_at, Utc::now())
}
