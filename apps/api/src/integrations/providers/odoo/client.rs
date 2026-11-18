//! JSON-2 transport: `POST {base_url}/json/2/<model>/<method>` with a bearer key, optional
//! `X-Odoo-Database` and an object body of named args (facts L24-L30; an array body is
//! silently dropped, L45-53). The key is opened from `integration_secrets` on first use.

use std::{sync::Arc, time::Duration};

use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::OnceCell;
use uuid::Uuid;

use super::errors;
use crate::{
    crypto::CredentialKey,
    error::AppError,
    integrations::{
        capability::{ErrorClass, ProviderError},
        connections::{repo::ConnectionRow, secrets},
        providers::ConnCtx,
    },
    modules::auth::token::PlainSecret,
    outbound::Outbound,
};

/// Whole-call limit; reads only, so a slow call is retried later.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Where the API key comes from when it isn't loaded yet.
struct Opener {
    db: PgPool,
    credentials: Option<Arc<CredentialKey>>,
    ws: Uuid,
    connection_id: Uuid,
}

/// Odoo client bound to one connection.
pub struct Client {
    outbound: Outbound,
    base_url: String,
    database: Option<String>,
    key: OnceCell<PlainSecret>,
    opener: Option<Opener>,
    timeout: Duration,
}

fn auth(detail: &str) -> ProviderError {
    ProviderError {
        class: ErrorClass::Auth,
        detail: detail.into(),
        code: None,
        http_status: None,
    }
}

impl Client {
    /// Client for an `odoo` connection row (config `{base_url, database}`, secret `api_key`).
    pub fn for_connection(ctx: &ConnCtx, row: &ConnectionRow) -> Result<Self, ProviderError> {
        let text = |k: &str| row.config.get(k).and_then(Value::as_str).map(str::to_owned);
        let base_url = text("base_url").ok_or_else(|| ProviderError {
            class: ErrorClass::Validation,
            detail: "The connection has no Odoo address.".into(),
            code: None,
            http_status: None,
        })?;
        Ok(Self {
            outbound: ctx.outbound.clone(),
            base_url,
            database: text("database").filter(|d| !d.is_empty()),
            key: OnceCell::new(),
            opener: Some(Opener {
                db: ctx.db.clone(),
                credentials: ctx.credentials.clone(),
                ws: row.workspace_id,
                connection_id: row.id,
            }),
            timeout: TIMEOUT,
        })
    }

    /// Client with a known key (tests).
    #[cfg(test)]
    pub(super) fn with_key(
        outbound: Outbound,
        base_url: String,
        database: Option<String>,
        key: &str,
        timeout: Duration,
    ) -> Self {
        Self {
            outbound,
            base_url,
            database,
            key: OnceCell::new_with(Some(PlainSecret::new(key.to_owned()))),
            opener: None,
            timeout,
        }
    }

    async fn key(&self) -> Result<&PlainSecret, ProviderError> {
        self.key
            .get_or_try_init(|| async {
                let o = self
                    .opener
                    .as_ref()
                    .ok_or_else(|| auth("No API key is stored for this connection."))?;
                let cred = o.credentials.as_deref().ok_or_else(|| {
                    auth("The server has no credentials key to open the API key.")
                })?;
                secrets::open(&o.db, cred, o.ws, o.connection_id, "api_key")
                    .await
                    .map_err(|e| match e {
                        AppError::NotFound => auth("No API key is stored for this connection."),
                        _ => auth("The stored API key could not be opened."),
                    })
            })
            .await
    }

    /// One JSON-2 call; `args` is the object of named arguments. Returns the parsed 2xx body.
    pub(super) async fn call(
        &self,
        model: &str,
        method: &str,
        args: Value,
    ) -> Result<Value, ProviderError> {
        let key = self.key().await?;
        let url = format!("{}/json/2/{model}/{method}", self.base_url);
        let mut req = self
            .outbound
            .request(reqwest::Method::POST, &url)
            .header("authorization", format!("bearer {}", key.expose()))
            .header("content-type", "application/json; charset=utf-8")
            .header("user-agent", "verzo")
            .timeout(self.timeout)
            .body(args.to_string());
        if let Some(db) = &self.database {
            req = req.header("x-odoo-database", db);
        }
        let resp = self.outbound.send(req).await.map_err(errors::unsent)?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .await
            .map_err(|e| errors::unsent(e.without_url().to_string()))?;
        if !(200..300).contains(&status) {
            return Err(errors::from_status(status, &body));
        }
        serde_json::from_slice(&body).map_err(|_| errors::unreadable("not JSON"))
    }
}
