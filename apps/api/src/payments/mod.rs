//! Provider-neutral payments: create a payment intent, fetch its status, refund it. Callers
//! (storefront checkout, the webhook drain, the sweep) only see [`Provider`]; a provider's
//! status is never trusted from a webhook body, always re-fetched. Providers: the in-process
//! fake (debug builds) and Stripe; adding one = one `Provider` variant + one file.

pub mod fake;
pub mod stripe;

use axum::http::HeaderMap;
use serde_json::Value;
use uuid::Uuid;

/// A payment's or refund's state, mapped from the provider's own words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Waiting for the customer (payment) or the provider (refund).
    Open,
    Paid,
    Refunded,
    Failed,
    Expired,
    Canceled,
}

impl Status {
    /// The `order_payments.status` word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Paid => "paid",
            Self::Refunded => "refunded",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Canceled => "canceled",
        }
    }

    /// No later change is possible.
    pub fn is_final(self) -> bool {
        !matches!(self, Self::Open)
    }
}

/// A payment or refund as the provider reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intent {
    /// The provider's id.
    pub id: String,
    pub status: Status,
    /// Minor units.
    pub amount: i64,
    /// ISO 4217, upper case.
    pub currency: String,
    /// Where the customer pays (open payments only).
    pub checkout_url: Option<String>,
    /// What we sent on create (`workspace_id`, `order_id`, `payment_id`), echoed back.
    pub metadata: Value,
}

/// A payment to create.
pub struct NewIntent<'a> {
    pub workspace_id: Uuid,
    /// Minor units of `currency`.
    pub amount: i64,
    pub currency: &'a str,
    /// Decimals of `currency` (providers that want `"12.50"`).
    pub decimals: u32,
    pub description: &'a str,
    /// Where the customer lands after paying.
    pub redirect_url: &'a str,
    /// Where the provider posts its status-changed call.
    pub webhook_url: &'a str,
    pub metadata: Value,
    /// Sent to the provider so a retried create returns the same payment (our row id).
    pub idempotency_key: &'a str,
    /// When our order stops waiting for payment; providers that can end the checkout then do.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A refund to create.
pub struct NewRefund<'a> {
    pub workspace_id: Uuid,
    /// The provider's payment id.
    pub payment_id: &'a str,
    pub amount: i64,
    pub currency: &'a str,
    pub decimals: u32,
    pub idempotency_key: &'a str,
}

/// Why a provider call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The provider said no (bad request, bad credentials): retrying won't help.
    #[error("provider refused: {0}")]
    Refused(String),
    /// The provider doesn't know this id.
    #[error("unknown at the provider")]
    NotFound,
    /// Timeout, 5xx, rate limit: retry later.
    #[error("provider unavailable: {0}")]
    Unavailable(String),
}

/// The configured provider. One variant per adapter.
#[derive(Clone, Debug)]
pub enum Provider {
    /// In-process, scriptable, debug builds only (`PAYMENTS_PROVIDER=fake`).
    Fake(fake::Fake),
    /// Stripe Checkout (`PAYMENTS_PROVIDER=stripe`).
    Stripe(stripe::Stripe),
}

impl Provider {
    /// Name used in URLs and `order_payments.provider`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Fake(_) => "fake",
            Self::Stripe(_) => "stripe",
        }
    }

    /// Create a payment; the same `idempotency_key` returns the same payment.
    pub async fn create(&self, p: &NewIntent<'_>) -> Result<Intent, Error> {
        match self {
            Self::Fake(f) => f.create(p),
            Self::Stripe(s) => s.create(p).await,
        }
    }

    /// The payment's current state at the provider.
    pub async fn fetch(&self, workspace_id: Uuid, id: &str) -> Result<Intent, Error> {
        match self {
            Self::Fake(f) => f.fetch(workspace_id, id),
            Self::Stripe(s) => s.fetch(workspace_id, id).await,
        }
    }

    /// Refund part or all of a paid payment; the same `idempotency_key` returns the same refund.
    pub async fn refund(&self, r: &NewRefund<'_>) -> Result<Intent, Error> {
        match self {
            Self::Fake(f) => f.refund(r),
            Self::Stripe(s) => s.refund(r).await,
        }
    }

    /// The payment or refund id a status-changed webhook names; None when the body isn't one
    /// or its signature fails (providers that sign check before reading the body). Only the
    /// id is read: the status is always re-fetched.
    pub fn webhook_id(&self, headers: &HeaderMap, body: &[u8]) -> Option<String> {
        match self {
            Self::Fake(_) => fake::webhook_id(body),
            Self::Stripe(s) => s.webhook_id(headers, body),
        }
    }

    /// A refund's current state.
    pub async fn fetch_refund(
        &self,
        workspace_id: Uuid,
        payment_id: &str,
        refund_id: &str,
    ) -> Result<Intent, Error> {
        match self {
            Self::Fake(f) => f.fetch_refund(workspace_id, payment_id, refund_id),
            Self::Stripe(s) => s.fetch_refund(workspace_id, payment_id, refund_id).await,
        }
    }
}

/// `PAYMENTS_PROVIDER`: unset or blank = none; `fake` only in debug builds (`release` false);
/// `stripe` with its settings (`Config` reads them only when it is chosen).
pub fn select(
    name: Option<&str>,
    release: bool,
    stripe: Option<stripe::Settings>,
) -> Result<Option<Provider>, String> {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        None => Ok(None),
        Some("fake") if release => {
            Err("PAYMENTS_PROVIDER=fake is refused in a release build".into())
        }
        Some("fake") => Ok(Some(Provider::Fake(fake::Fake::default()))),
        Some("stripe") => {
            let settings = stripe.ok_or("PAYMENTS_PROVIDER=stripe needs its STRIPE_* settings")?;
            Ok(Some(Provider::Stripe(stripe::Stripe::new(settings)?)))
        }
        Some(other) => Err(format!("PAYMENTS_PROVIDER {other} is not a known provider")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_takes_fake_in_debug_only_and_refuses_unknown_names() {
        assert!(select(None, false, None).unwrap().is_none());
        assert!(select(Some("  "), true, None).unwrap().is_none());
        let fake = select(Some("fake"), false, None).unwrap().unwrap();
        assert_eq!(fake.name(), "fake");
        assert!(
            select(Some("fake"), true, None)
                .unwrap_err()
                .contains("release")
        );
        assert!(select(Some("mollie"), false, None).is_err());
        assert!(
            select(Some("stripe"), true, None)
                .unwrap_err()
                .contains("STRIPE_")
        );
        let settings = stripe::Settings {
            api_url: stripe::API_URL.into(),
            secret_key: "sk_test_x".into(),
            webhook_secret: "whsec_x".into(),
        };
        let s = select(Some("stripe"), true, Some(settings))
            .unwrap()
            .unwrap();
        assert_eq!(s.name(), "stripe");
    }

    #[test]
    fn only_open_is_not_final() {
        for s in [
            Status::Paid,
            Status::Refunded,
            Status::Failed,
            Status::Expired,
            Status::Canceled,
        ] {
            assert!(s.is_final(), "{s:?}");
        }
        assert!(!Status::Open.is_final());
    }
}
