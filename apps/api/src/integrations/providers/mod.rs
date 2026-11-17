//! Provider dispatch (K20 style): one enum variant per adapter, chosen per connection row.
//! C3a adds `SourceClient::Odoo`, C3b `SinkClient::NetSuite`; until then live mode has no
//! adapter and every capability check fails with a readable detail.

use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    crypto::CredentialKey,
    integrations::{
        capability::{CapabilityCheck, ConnectionProbe, ErrorClass, ProviderError},
        connections::repo::ConnectionRow,
    },
    outbound::Outbound,
};

pub mod fake;

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
        fake_client(ctx, row).map(Self::Fake).ok_or_else(not_built)
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
