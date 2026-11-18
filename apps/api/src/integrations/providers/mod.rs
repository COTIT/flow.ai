//! Provider dispatch (K20 style): one enum variant per adapter, chosen per connection row.
//! Live `odoo` rows get `SourceClient::Odoo` (C3a); C3b adds `SinkClient::NetSuite`. A provider
//! without an adapter fails every capability check with a readable detail.

use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    crypto::CredentialKey,
    integrations::{
        capability::{
            CapabilityCheck, ConnectionProbe, Cursor, ErrorClass, Page, ProviderError,
            PurchaseRequest, PurchaseRequestSource, SourceControls, SourceRef,
        },
        connections::repo::ConnectionRow,
    },
    outbound::Outbound,
};

pub mod fake;
pub mod odoo;

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
    Odoo(odoo::Client),
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
        if let Some(f) = fake_client(ctx, row) {
            return Ok(Self::Fake(f));
        }
        match row.provider.as_str() {
            "odoo" => odoo::Client::for_connection(ctx, row).map(Self::Odoo),
            _ => Err(not_built()),
        }
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
            Self::Odoo(c) => c.test().await,
        }
    }
}

// The fake holds no purchase requests: nothing changed, every id gone, no controls.
impl PurchaseRequestSource for SourceClient {
    async fn list_changed(
        &self,
        company_ids: &[String],
        cursor: &Cursor,
        limit: u32,
    ) -> Result<Page<SourceRef>, ProviderError> {
        match self {
            Self::Fake(_) => Ok(Page {
                items: Vec::new(),
                next: None,
            }),
            Self::Odoo(c) => c.list_changed(company_ids, cursor, limit).await,
        }
    }

    async fn list_eligible_ids(
        &self,
        company_id: &str,
        after_id: i64,
        limit: u32,
    ) -> Result<Vec<String>, ProviderError> {
        match self {
            Self::Fake(_) => Ok(Vec::new()),
            Self::Odoo(c) => c.list_eligible_ids(company_id, after_id, limit).await,
        }
    }

    async fn read(
        &self,
        ids: &[String],
    ) -> Result<(Vec<PurchaseRequest>, Vec<String>), ProviderError> {
        match self {
            Self::Fake(_) => Ok((Vec::new(), ids.to_vec())),
            Self::Odoo(c) => c.read(ids).await,
        }
    }

    async fn read_controls(
        &self,
        company_ids: &[String],
    ) -> Result<Vec<SourceControls>, ProviderError> {
        match self {
            Self::Fake(_) => Ok(Vec::new()),
            Self::Odoo(c) => c.read_controls(company_ids).await,
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

    #[tokio::test]
    async fn fake_source_has_no_requests() {
        let c = SourceClient::Fake(fake::Client {
            fake: fake::Fake::default(),
            connection_id: "c".into(),
            provider: "odoo".into(),
        });
        let cursor = Cursor {
            ts: chrono::Utc::now(),
            id: 0,
        };
        let page = c.list_changed(&["1".into()], &cursor, 10).await.unwrap();
        assert!(page.items.is_empty() && page.next.is_none());
        assert!(c.list_eligible_ids("1", 0, 10).await.unwrap().is_empty());
        let (found, gone) = c.read(&["7".into()]).await.unwrap();
        assert!(found.is_empty());
        assert_eq!(gone, ["7"]);
        assert!(c.read_controls(&["1".into()]).await.unwrap().is_empty());
    }
}
