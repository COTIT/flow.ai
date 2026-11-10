//! Workspace account selection and locked credential/account identity changes.
use crate::{
    AppError, AppState,
    modules::{
        auth::token,
        settings::{self, dto::Change},
    },
    payments::globadyme::Globadyme,
};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Selected immutable account identity and revision-scoped client.
pub struct Account {
    pub environment: String,
    pub merchant_id: String,
    pub revision: i64,
    pub client: Globadyme,
}

/// Fixed secret path; no tenant-supplied host or alternate credential namespace.
pub fn path(environment: &str, field: &str) -> String {
    format!("integrations.payment.payment_providers.globadyme.{environment}.{field}")
}

/// Read sealed account credential without exposing it in errors/logs.
pub async fn secret(
    state: &AppState,
    ws: Uuid,
    environment: &str,
    field: &str,
) -> Result<Option<token::PlainSecret>, AppError> {
    let path = path(environment, field);
    let Some(row) = settings::repo::credential(&state.db, ws, &path).await? else {
        return Ok(None);
    };
    let key = state
        .credentials
        .as_ref()
        .ok_or(AppError::Unavailable("Credential storage is unavailable."))?;
    if key.id() != row.0 {
        return Err(AppError::Unavailable(
            "Payment credentials require their original storage key.",
        ));
    }
    Ok(Some(key.open(ws, &path, &row.1)?))
}

/// Workspace payment selection; absent selection falls back to legacy platform choice.
pub async fn configuration(state: &AppState, ws: Uuid) -> Result<Value, AppError> {
    Ok(settings::repo::section(&state.db, ws, "integrations")
        .await?
        .unwrap_or(Value::Null)["payment"]
        .clone())
}

/// Selected Globadyme account for new collections. Incomplete selection fails closed.
pub async fn select(state: &AppState, ws: Uuid) -> Result<Option<Account>, AppError> {
    let config = configuration(state, ws).await?;
    if config["active_provider"].as_str() != Some("globadyme") {
        return Ok(None);
    }
    if config["enabled"].as_bool() != Some(true) {
        return Err(AppError::PaymentSetupRequired);
    }
    let env = config["environment"].as_str().unwrap_or("sandbox");
    let merchant = config["payment_providers"]["globadyme"][env]["merchant_id"]
        .as_str()
        .filter(|id| !id.trim().is_empty())
        .ok_or(AppError::PaymentSetupRequired)?;
    bound(state, ws, env, Some(merchant))
        .await
        .map(Some)
        .map_err(setup_error)
}

/// Bound-account checks perform no provider I/O; database failures remain operational errors.
pub(crate) fn setup_error(error: AppError) -> AppError {
    match error {
        AppError::Unavailable(_)
        | AppError::NotFound
        | AppError::Conflict(_)
        | AppError::Internal(_) => AppError::PaymentSetupRequired,
        error => error,
    }
}

/// Historical account resolution bypasses collection disable/selection switches.
pub async fn bound(
    state: &AppState,
    ws: Uuid,
    environment: &str,
    expected: Option<&str>,
) -> Result<Account, AppError> {
    if !matches!(environment, "sandbox" | "production") {
        return Err(AppError::NotFound);
    }
    let mut tx = state.db.begin().await?;
    let row: Option<(String,i64)> = sqlx::query_as("SELECT merchant_id,credential_revision FROM payment_accounts WHERE workspace_id=$1 AND environment=$2 FOR SHARE")
        .bind(ws).bind(environment).fetch_optional(&mut *tx).await?;
    let (merchant_id, revision) =
        row.ok_or(AppError::Unavailable("Globadyme account is incomplete."))?;
    if expected.is_some_and(|id| id != merchant_id) {
        return Err(AppError::Conflict("Payment account binding changed."));
    }
    let key = state
        .credentials
        .as_ref()
        .ok_or(AppError::Unavailable("Credential storage is unavailable."))?;
    let mut credentials = Vec::new();
    for field in ["username", "password", "callback_secret"] {
        let path = path(environment, field);
        let row = settings::repo::credential(&mut *tx, ws, &path)
            .await?
            .ok_or(AppError::Unavailable("Globadyme account is incomplete."))?;
        if key.id() != row.0 {
            return Err(AppError::Unavailable(
                "Payment credentials require their original storage key.",
            ));
        }
        credentials.push(key.open(ws, &path, &row.1)?);
    }
    tx.commit().await?;
    let username = &credentials[0];
    let password = &credentials[1];
    let client = state.payments.globadyme(ws,environment,&merchant_id,revision,username.expose().to_owned(),password.expose().to_owned())
        .await.map_err(|_| AppError::Unavailable("Globadyme environment is unavailable; production origin requires official verification."))?;
    Ok(Account {
        environment: environment.into(),
        merchant_id,
        revision,
        client,
    })
}

/// First use and configuration changes share account lock; stale selections never submit.
pub async fn bind(conn: &mut PgConnection, ws: Uuid, account: &Account) -> Result<(), AppError> {
    let config: Option<Value> = sqlx::query_scalar(
        "SELECT data->'integrations'->'payment' FROM settings WHERE workspace_id=$1 FOR SHARE",
    )
    .bind(ws)
    .fetch_optional(&mut *conn)
    .await?;
    let config = config.unwrap_or(Value::Null);
    if config["active_provider"].as_str() != Some("globadyme")
        || config["enabled"].as_bool() != Some(true)
        || config["environment"].as_str().unwrap_or("sandbox") != account.environment
        || config["payment_providers"]["globadyme"][&account.environment]["merchant_id"].as_str()
            != Some(account.merchant_id.as_str())
    {
        return Err(AppError::Conflict(
            "Payment configuration changed; refresh before paying.",
        ));
    }
    bind_history(conn, ws, account).await
}

/// Historical obligations retain account access when new collections are disabled.
pub async fn bind_history(
    conn: &mut PgConnection,
    ws: Uuid,
    account: &Account,
) -> Result<(), AppError> {
    let row: Option<(String,i64)> = sqlx::query_as("SELECT merchant_id,credential_revision FROM payment_accounts WHERE workspace_id=$1 AND environment=$2 FOR UPDATE")
        .bind(ws).bind(&account.environment).fetch_optional(&mut *conn).await?;
    if row
        .as_ref()
        .is_none_or(|(m, r)| m != &account.merchant_id || *r != account.revision)
    {
        return Err(AppError::Conflict(
            "Payment configuration changed; refresh before paying.",
        ));
    }
    sqlx::query("UPDATE payment_accounts SET used_at=coalesce(used_at,now()) WHERE workspace_id=$1 AND environment=$2")
        .bind(ws).bind(&account.environment).execute(conn).await?;
    Ok(())
}

/// Sync account identities inside settings transaction, before credential replacements.
pub async fn sync(
    state: &AppState,
    conn: &mut PgConnection,
    ws: Uuid,
    user: Uuid,
    sections: &Value,
    changes: &[(&'static str, Change)],
    clears: &mut Vec<String>,
) -> Result<(), AppError> {
    let Some(integrations) = sections.get("integrations") else {
        return Ok(());
    };
    for env in ["sandbox", "production"] {
        let slot = &integrations["payment"]["payment_providers"]["globadyme"][env];
        let row: Option<(String,Vec<u8>,Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as("SELECT merchant_id,principal_hash,used_at FROM payment_accounts WHERE workspace_id=$1 AND environment=$2 FOR UPDATE")
            .bind(ws).bind(env).fetch_optional(&mut *conn).await?;
        let used = row.as_ref().is_some_and(|r| r.2.is_some());
        let mut revision_changed = false;
        let mut principal = row.as_ref().map(|r| r.1.clone());
        for field in [
            "username",
            "password",
            "callback_secret",
            "previous_callback_secret",
        ] {
            let path = path(env, field);
            let change = changes.iter().find(|(p, _)| *p == path).map(|(_, c)| c);
            if field == "previous_callback_secret"
                && matches!(change, Some(Change::Clear))
                && !clears.contains(&path)
            {
                continue;
            }
            let stored = settings::repo::credential(&mut *conn, ws, &path).await?;
            if matches!(change, Some(Change::Clear)) && stored.is_some() {
                revision_changed = true;
            }
            match change {
                Some(Change::Clear)
                    if stored.is_some() && (used || field == "previous_callback_secret") =>
                {
                    return Err(AppError::field(
                        &path,
                        "Stored payment credentials must be retained; callback retirement requires audited resolution.",
                    ));
                }
                Some(Change::Set(_)) if field == "previous_callback_secret" => {
                    return Err(AppError::field(
                        &path,
                        "Previous callback secret is managed by rotation.",
                    ));
                }
                Some(Change::Set(plain)) => {
                    revision_changed = true;
                    if field == "username" {
                        let hash = token::hash(plain.expose()).to_vec();
                        if used && principal.as_ref() != Some(&hash) {
                            return Err(AppError::field(
                                &path,
                                "Used payment login principal is immutable.",
                            ));
                        }
                        principal = Some(hash);
                    }
                    if field == "callback_secret"
                        && let Some(stored) = stored
                    {
                        let key = state
                            .credentials
                            .as_ref()
                            .ok_or(AppError::Unavailable("Credential storage is unavailable."))?;
                        if key.id() != stored.0 {
                            return Err(AppError::Conflict(
                                "Original credential storage key is required for rotation.",
                            ));
                        }
                        let old = key.open(ws, &path, &stored.1)?;
                        if old.expose() != plain.expose() {
                            let previous = self::path(env, "previous_callback_secret");
                            if settings::repo::credential(&mut *conn, ws, &previous)
                                .await?
                                .is_some()
                            {
                                return Err(AppError::field(
                                    &path,
                                    "Retire previous callback secret with audit evidence before rotating again.",
                                ));
                            }
                            let sealed = key.seal(ws, &previous, old.expose())?;
                            settings::repo::put_credential(
                                &mut *conn,
                                ws,
                                &previous,
                                key.id(),
                                &sealed,
                                user,
                            )
                            .await?;
                            clears.retain(|p| p != &previous);
                            audit(
                                conn,
                                ws,
                                user,
                                env,
                                "callback_rotated",
                                "Callback secret rotated",
                                "Previous secret retained",
                            )
                            .await?;
                        }
                    }
                }
                _ => {}
            }
        }
        let merchant = slot["merchant_id"]
            .as_str()
            .filter(|m| !m.trim().is_empty());
        if used && merchant != row.as_ref().map(|r| r.0.as_str()) {
            return Err(AppError::field(
                &path(env, "merchant_id"),
                "Used merchant identity is immutable.",
            ));
        }
        if let (Some(merchant), Some(principal)) = (merchant, principal) {
            let result = sqlx::query("INSERT INTO payment_accounts(workspace_id,environment,merchant_id,principal_hash) VALUES($1,$2,$3,$4) ON CONFLICT(workspace_id,environment) DO UPDATE SET merchant_id=excluded.merchant_id,principal_hash=excluded.principal_hash,credential_revision=payment_accounts.credential_revision+CASE WHEN $5 THEN 1 ELSE 0 END,updated_at=now()")
                .bind(ws).bind(env).bind(merchant).bind(principal).bind(revision_changed).execute(&mut *conn).await;
            match result {
                Ok(_) => {}
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                    return Err(AppError::field(
                        &path(env, "merchant_id"),
                        "Merchant is already bound to another workspace in this environment.",
                    ));
                }
                Err(e) => return Err(e.into()),
            }
            audit(
                conn,
                ws,
                user,
                env,
                "account_configured",
                "Payment configuration saved",
                "Workspace settings update",
            )
            .await?;
        } else if row.is_some() {
            sqlx::query("UPDATE payment_accounts SET credential_revision=credential_revision+1,updated_at=now() WHERE workspace_id=$1 AND environment=$2")
                .bind(ws).bind(env).execute(&mut *conn).await?;
        }
    }
    Ok(())
}

/// Append operator/account audit without secrets.
pub async fn audit(
    conn: &mut PgConnection,
    ws: Uuid,
    user: Uuid,
    env: &str,
    action: &str,
    reason: &str,
    evidence: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO payment_audit(workspace_id,user_id,environment,action,reason,evidence) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(ws).bind(user).bind(env).bind(action).bind(reason).bind(evidence).execute(conn).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_classification_preserves_database_failures() {
        assert!(matches!(
            setup_error(AppError::Database(sqlx::Error::PoolClosed)),
            AppError::Database(_)
        ));
        assert!(matches!(
            setup_error(AppError::Unavailable("Missing account")),
            AppError::PaymentSetupRequired
        ));
        assert!(matches!(
            setup_error(AppError::Internal(
                "Sealed credential failed to open".into()
            )),
            AppError::PaymentSetupRequired
        ));
    }
}
