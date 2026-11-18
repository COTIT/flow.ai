//! Provider dispatch (K20 style): one enum variant per adapter, chosen per connection row.
//! C3a adds `SourceClient::Odoo`; C3b added `SinkClient::NetSuite`. A provider without an adapter
//! fails every capability check with a readable detail.

use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    crypto::CredentialKey,
    integrations::{
        capability::{
            CapabilityCheck, ConnectionProbe, CreateOutcome, ErrorClass, NewPo, ProviderError,
            PurchaseOrderSink, TargetPo,
        },
        connections::repo::ConnectionRow,
    },
    outbound::Outbound,
};

pub mod fake;
pub mod netsuite;

/// `INTEGRATIONS_PROVIDERS`: unset = live adapters; `fake` only in debug builds.
#[derive(Clone, Debug)]
pub enum Mode {
    Live,
    Fake(fake::Fake),
}

/// Mode from `INTEGRATIONS_PROVIDERS`; refuses `fake` in a release build and unknown names.
pub fn select(name: Option<&str>, release: bool) -> Result<Mode, String> {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        None => Ok(Mode::Live),
        Some("fake") if release => {
            Err("INTEGRATIONS_PROVIDERS=fake is refused in a release build".into())
        }
        Some("fake") => Ok(Mode::Fake(fake::Fake::default())),
        Some(other) => Err(format!("INTEGRATIONS_PROVIDERS {other} is not known")),
    }
}

/// What a provider client needs to reach its system.
pub struct ConnCtx {
    pub outbound: Outbound,
    pub credentials: Option<Arc<CredentialKey>>,
    pub db: PgPool,
    pub mode: Mode,
}

/// Source-side clients (Odoo).
pub enum SourceClient {
    Fake(fake::Client),
}

/// Target-side clients (NetSuite).
pub enum SinkClient {
    Fake(fake::Client),
    NetSuite(netsuite::Client),
}

fn not_built() -> ProviderError {
    ProviderError {
        class: ErrorClass::Unknown,
        detail: "This provider is not connected yet.".into(),
        code: None,
        http_status: None,
    }
}

fn fake_client(ctx: &ConnCtx, row: &ConnectionRow) -> Option<fake::Client> {
    match &ctx.mode {
        Mode::Fake(f) => Some(fake::Client {
            fake: f.clone(),
            connection_id: row.id.to_string(),
            provider: row.provider.clone(),
        }),
        Mode::Live => None,
    }
}

impl SourceClient {
    /// Client for a source connection row.
    pub fn for_connection(ctx: &ConnCtx, row: &ConnectionRow) -> Result<Self, ProviderError> {
        fake_client(ctx, row).map(Self::Fake).ok_or_else(not_built)
    }
}

impl SinkClient {
    /// Client for a target connection row.
    pub fn for_connection(ctx: &ConnCtx, row: &ConnectionRow) -> Result<Self, ProviderError> {
        if let Some(c) = fake_client(ctx, row) {
            return Ok(Self::Fake(c));
        }
        if row.provider != "netsuite" {
            return Err(not_built());
        }
        let key = ctx.credentials.clone().ok_or_else(|| ProviderError {
            class: ErrorClass::Unknown,
            detail: "CREDENTIALS_KEY is not set, so the private key can't be opened.".into(),
            code: None,
            http_status: None,
        })?;
        netsuite::Client::for_connection(
            ctx.outbound.clone(),
            &row.config,
            netsuite::KeySource::Sealed {
                db: ctx.db.clone(),
                key,
                ws: row.workspace_id,
                connection_id: row.id,
            },
        )
        .map(Self::NetSuite)
    }
}

fn fake_has_no_orders() -> ProviderError {
    ProviderError {
        class: ErrorClass::Unknown,
        detail: "The fake provider has no purchase orders.".into(),
        code: None,
        http_status: None,
    }
}

impl PurchaseOrderSink for SinkClient {
    async fn find_by_external_id(&self, eid: &str) -> Result<Option<TargetPo>, ProviderError> {
        match self {
            Self::Fake(_) => Err(fake_has_no_orders()),
            Self::NetSuite(c) => c.find_by_external_id(eid).await,
        }
    }

    async fn create(&self, po: &NewPo) -> CreateOutcome {
        match self {
            Self::Fake(_) => CreateOutcome::Rejected(fake_has_no_orders()),
            Self::NetSuite(c) => c.create(po).await,
        }
    }

    async fn read(&self, id: &str) -> Result<TargetPo, ProviderError> {
        match self {
            Self::Fake(_) => Err(fake_has_no_orders()),
            Self::NetSuite(c) => c.read(id).await,
        }
    }
}

impl ConnectionProbe for SourceClient {
    async fn test(&self) -> Vec<CapabilityCheck> {
        match self {
            Self::Fake(c) => c.fake.test(&c.connection_id, &c.provider),
        }
    }
}

impl ConnectionProbe for SinkClient {
    async fn test(&self) -> Vec<CapabilityCheck> {
        match self {
            Self::Fake(c) => c.fake.test(&c.connection_id, &c.provider),
            Self::NetSuite(c) => c.test().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_takes_fake_in_debug_only() {
        assert!(matches!(select(None, true), Ok(Mode::Live)));
        assert!(matches!(select(Some(" "), true), Ok(Mode::Live)));
        assert!(matches!(select(Some("fake"), false), Ok(Mode::Fake(_))));
        assert!(select(Some("fake"), true).unwrap_err().contains("release"));
        assert!(select(Some("odoo"), false).is_err());
    }
}
