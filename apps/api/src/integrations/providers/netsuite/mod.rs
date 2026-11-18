//! NetSuite REST adapter (WF-01 target, C3b): M2M auth, purchase order sink, probe. Verified
//! against the documented contract in `docs/integrations/facts/netsuite-rest.md`, never against a
//! live account (LIVE gates L2–L6).

use std::{sync::Arc, time::Duration};

use reqwest::header::HeaderMap;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    crypto::CredentialKey,
    integrations::capability::{ErrorClass, ProviderError},
    modules::auth::token::PlainSecret,
    outbound::Outbound,
};

pub mod auth;
pub mod errors;
pub mod probe;
pub mod sink;

/// Per-request timeout in production. NetSuite's own cap is 15 minutes (N5 l.244).
const TIMEOUT: Duration = Duration::from_secs(60);
/// Record service path (l.25).
pub const RECORD_PATH: &str = "/services/rest/record/v1";

/// Where the private key comes from.
pub enum KeySource {
    /// PEM held in memory (tests).
    Pem(PlainSecret),
    /// Sealed `private_key` of a connection, opened only when a token is fetched.
    Sealed {
        db: PgPool,
        key: Arc<CredentialKey>,
        ws: Uuid,
        connection_id: Uuid,
    },
}

/// A NetSuite client bound to one account and integration.
pub struct Client {
    outbound: Outbound,
    /// Scheme + host, no trailing slash: `https://<rest_domain>` in production.
    base: String,
    client_id: String,
    certificate_id: String,
    key: KeySource,
    timeout: Duration,
}

/// One HTTP reply, body read in full.
pub(crate) struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Client {
    /// Client with a PEM key in memory. `base` is scheme + host (tests point it at a fake).
    pub fn with_pem(
        outbound: Outbound,
        base: &str,
        client_id: &str,
        certificate_id: &str,
        pem: String,
    ) -> Self {
        Self::new(
            outbound,
            base,
            client_id,
            certificate_id,
            KeySource::Pem(PlainSecret::new(pem)),
        )
    }

    /// Client for a connection: config `rest_domain`, `client_id`, `certificate_id` (contracts
    /// §4.5); the key stays sealed until a token is fetched.
    pub fn for_connection(
        outbound: Outbound,
        config: &serde_json::Value,
        key: KeySource,
    ) -> Result<Self, ProviderError> {
        let field = |name: &str| {
            config[name]
                .as_str()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    errors::err(
                        ErrorClass::Validation,
                        format!("NetSuite config has no {name}."),
                        None,
                        None,
                    )
                })
        };
        Ok(Self::new(
            outbound,
            &format!("https://{}", field("rest_domain")?),
            field("client_id")?,
            field("certificate_id")?,
            key,
        ))
    }

    fn new(
        outbound: Outbound,
        base: &str,
        client_id: &str,
        certificate_id: &str,
        key: KeySource,
    ) -> Self {
        Self {
            outbound,
            base: base.trim_end_matches('/').to_owned(),
            client_id: client_id.to_owned(),
            certificate_id: certificate_id.to_owned(),
            key,
            timeout: TIMEOUT,
        }
    }

    /// Same client with another per-request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Send and read the whole reply. A body lost after the status line is Ambiguous.
    pub(crate) async fn send(&self, req: reqwest::RequestBuilder) -> Result<Reply, ProviderError> {
        let res = self
            .outbound
            .execute(req)
            .await
            .map_err(errors::transport)?;
        let status = res.status().as_u16();
        let headers = res.headers().clone();
        let body = res.bytes().await.map_err(|_| {
            errors::err(
                ErrorClass::Ambiguous,
                "The reply was cut off.",
                None,
                Some(status),
            )
        })?;
        Ok(Reply {
            status,
            headers,
            body: body.to_vec(),
        })
    }

    /// REST request to `path` (after the host) with the bearer token.
    fn rest(
        &self,
        method: reqwest::Method,
        path: &str,
        token: &PlainSecret,
    ) -> reqwest::RequestBuilder {
        self.outbound
            .request(method, &format!("{}{path}", self.base))
            .timeout(self.timeout)
            .header("Authorization", format!("Bearer {}", token.expose()))
    }

    /// GET JSON. A 401 drops the token and retries once (reads are safe to repeat).
    pub(crate) async fn get_json(&self, path: &str) -> Result<serde_json::Value, ProviderError> {
        let mut retried = false;
        loop {
            let token = self.token().await?;
            let reply = self
                .send(self.rest(reqwest::Method::GET, path, &token))
                .await
                .map_err(errors::for_read)?;
            match reply.status {
                200 => {
                    return serde_json::from_slice(&reply.body).map_err(|_| {
                        errors::err(
                            ErrorClass::Unknown,
                            "NetSuite sent a reply that is not JSON.",
                            None,
                            Some(200),
                        )
                    });
                }
                401 if !retried => {
                    self.forget_token();
                    retried = true;
                }
                s => return Err(errors::for_read(errors::from_status(s, &reply.body))),
            }
        }
    }
}
