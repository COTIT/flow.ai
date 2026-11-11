//! Provider registry: selection controls new collections; bound history retains providers.

use super::{GlobadymeEnvironment, Provider, globadyme::Globadyme};
use crate::outbound::Outbound;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

type AccountKey = (Uuid, String, String, i64);

/// Payment clients shared by HTTP and recovery workers.
#[derive(Clone, Default)]
pub struct Payments {
    primary: Option<Provider>,
    retained: Vec<Provider>,
    sandbox_url: Option<String>,
    globadyme_environment: Option<GlobadymeEnvironment>,
    clients: Arc<Mutex<HashMap<AccountKey, Globadyme>>>,
}

impl std::fmt::Debug for Payments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Payments(***)")
    }
}

impl From<Provider> for Payments {
    fn from(provider: Provider) -> Self {
        Self {
            primary: Some(provider),
            ..Self::default()
        }
    }
}

impl From<Option<Provider>> for Payments {
    fn from(primary: Option<Provider>) -> Self {
        Self {
            primary,
            ..Self::default()
        }
    }
}

impl Payments {
    /// Default provider for legacy collections.
    pub fn as_ref(&self) -> Option<&Provider> {
        self.primary.as_ref()
    }
    /// No legacy collection provider configured.
    pub fn is_none(&self) -> bool {
        self.primary.is_none()
    }
    /// Pin Globadyme to the operator's deployment realm; None disables all its clients.
    pub fn with_globadyme_environment(mut self, environment: Option<GlobadymeEnvironment>) -> Self {
        self.globadyme_environment = environment;
        self
    }
    pub fn globadyme_environment(&self) -> Option<GlobadymeEnvironment> {
        self.globadyme_environment
    }
    /// Keep configured provider available after selection changes.
    pub fn retain(mut self, provider: Provider) -> Self {
        if self.provider(provider.name()).is_none() {
            self.retained.push(provider);
        }
        self
    }
    /// Historical provider, independent of current selection.
    pub fn provider(&self, name: &str) -> Option<&Provider> {
        self.primary
            .iter()
            .chain(&self.retained)
            .find(|p| p.name() == name)
    }
    /// Configured legacy providers for bounded recovery sweeps.
    pub fn providers(&self) -> impl Iterator<Item = &Provider> {
        self.primary.iter().chain(&self.retained)
    }
    /// Debug-only loopback endpoint for isolated protocol tests.
    pub fn with_globadyme_url(mut self, url: &str) -> Result<Self, String> {
        let parsed = reqwest::Url::parse(url).map_err(|_| "invalid Globadyme test URL")?;
        if !cfg!(debug_assertions)
            || !matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
            || !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err("Globadyme test URL must be loopback in debug builds".into());
        }
        self.sandbox_url = Some(url.trim_end_matches('/').to_owned());
        Ok(self)
    }
    /// Account-scoped token cache; revision changes invalidate older clients.
    pub async fn globadyme(
        &self,
        workspace: Uuid,
        environment: &str,
        merchant: &str,
        revision: i64,
        username: String,
        password: String,
    ) -> Result<Globadyme, String> {
        if self.globadyme_environment.map(GlobadymeEnvironment::as_str) != Some(environment) {
            return Err("Globadyme account environment does not match this deployment".into());
        }
        if environment != "sandbox" {
            return Err("Globadyme production origin has not been verified".into());
        }
        let key = (
            workspace,
            environment.to_owned(),
            merchant.to_owned(),
            revision,
        );
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }
        let url = self
            .sandbox_url
            .as_deref()
            .unwrap_or("https://api.sbx.globadyme.com/api");
        let trusted: Vec<String> = self
            .sandbox_url
            .as_deref()
            .and_then(crate::outbound::host_of)
            .into_iter()
            .collect();
        let client = Globadyme::new(
            url.to_owned(),
            username,
            password,
            revision,
            Outbound::new(&trusted)?,
        )?;
        if clients
            .keys()
            .any(|(ws, env, _, rev)| *ws == workspace && env == environment && *rev > revision)
        {
            return Ok(client);
        }
        clients.retain(|(ws, env, _, _), _| *ws != workspace || env != environment);
        clients.insert(key, client.clone());
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn client(payments: &Payments, environment: &str) -> Result<Globadyme, String> {
        payments
            .globadyme(
                Uuid::nil(),
                environment,
                "merchant",
                1,
                "user".into(),
                "password".into(),
            )
            .await
    }

    #[tokio::test]
    async fn missing_or_mismatched_realm_rejects_before_cache_or_client_creation() {
        let disabled = Payments::default();
        assert_eq!(disabled.globadyme_environment(), None);
        for environment in ["sandbox", "production", "unknown"] {
            assert!(
                client(&disabled, environment)
                    .await
                    .unwrap_err()
                    .contains("deployment")
            );
        }
        let sandbox = disabled.with_globadyme_environment(Some(GlobadymeEnvironment::Sandbox));
        assert_eq!(
            sandbox.globadyme_environment(),
            Some(GlobadymeEnvironment::Sandbox)
        );
        assert!(
            client(&sandbox, "production")
                .await
                .unwrap_err()
                .contains("deployment")
        );
        assert!(sandbox.clients.lock().await.is_empty());
        client(&sandbox, "sandbox").await.unwrap();
        let disabled = sandbox.with_globadyme_environment(None);
        assert!(
            !disabled.clients.lock().await.is_empty(),
            "exercise cached-account rejection"
        );
        assert!(
            client(&disabled, "sandbox")
                .await
                .unwrap_err()
                .contains("deployment")
        );
    }

    #[tokio::test]
    async fn production_pin_keeps_unverified_origin_closed() {
        let production =
            Payments::default().with_globadyme_environment(Some(GlobadymeEnvironment::Production));
        assert!(
            client(&production, "sandbox")
                .await
                .unwrap_err()
                .contains("deployment")
        );
        assert!(
            client(&production, "production")
                .await
                .unwrap_err()
                .contains("origin has not been verified")
        );
        assert!(production.clients.lock().await.is_empty());
    }
}
