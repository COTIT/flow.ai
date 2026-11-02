//! Stripe adapter: a payment is a Checkout Session (Stripe's hosted page), a refund is a Refund
//! of the session's PaymentIntent. Form-encoded calls with the secret key; webhooks are checked
//! against the endpoint's signing secret before the body is read, then only the object id is
//! taken. One platform account for every workspace; the workspace travels in metadata.

use std::{sync::Arc, time::Duration};

use axum::http::{HeaderMap, header::CONTENT_TYPE};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::Method;
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

use super::{Error, Intent, NewIntent, NewRefund, Status};
use crate::outbound::Outbound;

/// Stripe's API origin.
pub const API_URL: &str = "https://api.stripe.com";
/// Pinned API version: the session and refund fields read here are stable since.
const VERSION: &str = "2024-06-20";
const TIMEOUT: Duration = Duration::from_secs(20);
/// Stripe's own signature tolerance.
const TOLERANCE_SECS: i64 = 300;

/// Stripe settings, checked by `Config`.
#[derive(Clone)]
pub struct Settings {
    /// `STRIPE_API_URL`: Stripe's, or a loopback fake.
    pub api_url: String,
    /// `STRIPE_SECRET_KEY` (`sk_…` or restricted `rk_…`).
    pub secret_key: String,
    /// `STRIPE_WEBHOOK_SECRET` (`whsec_…`) of the dashboard endpoint.
    pub webhook_secret: String,
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("api_url", &self.api_url)
            .field("secret_key", &"***")
            .field("webhook_secret", &"***")
            .finish()
    }
}

/// The Stripe provider; clones share one client.
#[derive(Clone)]
pub struct Stripe {
    settings: Arc<Settings>,
    http: Outbound,
    timeout: Duration,
}

impl std::fmt::Debug for Stripe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Stripe")
    }
}

impl Stripe {
    /// A client for `settings`; an overridden `api_url` (a loopback fake, checked by `Config`)
    /// is trusted by the outbound guard.
    pub fn new(settings: Settings) -> Result<Self, String> {
        let trusted: Vec<String> = Some(settings.api_url.as_str())
            .filter(|u| *u != API_URL)
            .and_then(crate::outbound::host_of)
            .into_iter()
            .collect();
        Ok(Self {
            settings: Arc::new(settings),
            http: Outbound::new(&trusted)?,
            timeout: TIMEOUT,
        })
    }

    /// Per-call timeout (default 20 s); tests shorten it.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// One API call; Stripe's JSON answer, or the error mapped to retry or not.
    async fn call(
        &self,
        method: Method,
        path: &str,
        form: Option<&[(String, String)]>,
        idempotency_key: Option<&str>,
    ) -> Result<Value, Error> {
        let mut req = self
            .http
            .request(method, &format!("{}{path}", self.settings.api_url))
            .bearer_auth(&self.settings.secret_key)
            .header("Stripe-Version", VERSION)
            .timeout(self.timeout);
        if let Some(key) = idempotency_key {
            req = req.header("Idempotency-Key", key);
        }
        if let Some(form) = form {
            req = req
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(encode(form));
        }
        let res = self.http.send(req).await.map_err(Error::Unavailable)?;
        let status = res.status();
        let bytes = res
            .bytes()
            .await
            .map_err(|e| Error::Unavailable(format!("read: {}", e.without_url())))?;
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        match status.as_u16() {
            200..=299 => Ok(body),
            404 => Err(Error::NotFound),
            // 409: the same idempotency key is in flight; 429: rate limited.
            409 | 429 | 500.. => Err(Error::Unavailable(format!("stripe answered {status}"))),
            _ => {
                let msg = body["error"]["message"].as_str().unwrap_or("refused");
                Err(Error::Refused(msg.chars().take(200).collect()))
            }
        }
    }

    async fn get(&self, path: &str) -> Result<Value, Error> {
        self.call(Method::GET, path, None, None).await
    }

    pub(super) async fn create(&self, p: &NewIntent<'_>) -> Result<Intent, Error> {
        let mut form = pairs(&[
            ("mode", "payment"),
            ("line_items[0][quantity]", "1"),
            (
                "line_items[0][price_data][product_data][name]",
                p.description,
            ),
            ("success_url", p.redirect_url),
            ("cancel_url", p.redirect_url),
        ]);
        form.extend([
            (
                "line_items[0][price_data][currency]".into(),
                p.currency.to_ascii_lowercase(),
            ),
            // shortcut: Stripe's minor units match ours except HUF, TWD, ISK and UGX; check
            // Stripe's currency list before a shop sells in one of them.
            (
                "line_items[0][price_data][unit_amount]".into(),
                p.amount.to_string(),
            ),
        ]);
        if let Some(at) = p.expires_at {
            let at = expires_at(at.timestamp(), chrono::Utc::now().timestamp());
            form.push(("expires_at".into(), at.to_string()));
        }
        for (k, v) in metadata(&p.metadata) {
            form.push((format!("metadata[{k}]"), v.clone()));
            form.push((format!("payment_intent_data[metadata][{k}]"), v));
        }
        let v = self
            .call(
                Method::POST,
                "/v1/checkout/sessions",
                Some(&form),
                Some(p.idempotency_key),
            )
            .await?;
        session(&v).ok_or_else(unexpected)
    }

    /// The session, NotFound when its metadata names another workspace.
    async fn session_of(&self, workspace_id: Uuid, id: &str) -> Result<Value, Error> {
        if !id_ok(id) {
            return Err(Error::NotFound);
        }
        let v = self.get(&format!("/v1/checkout/sessions/{id}")).await?;
        if v["metadata"]["workspace_id"] != workspace_id.to_string().as_str() {
            return Err(Error::NotFound);
        }
        Ok(v)
    }

    pub(super) async fn fetch(&self, workspace_id: Uuid, id: &str) -> Result<Intent, Error> {
        let v = self.session_of(workspace_id, id).await?;
        session(&v).ok_or_else(unexpected)
    }

    pub(super) async fn refund(&self, r: &NewRefund<'_>) -> Result<Intent, Error> {
        let s = self.session_of(r.workspace_id, r.payment_id).await?;
        let Some(intent) = s["payment_intent"].as_str() else {
            return Err(Error::Refused("payment not paid".into()));
        };
        let form = pairs(&[
            ("payment_intent", intent),
            ("metadata[workspace_id]", &r.workspace_id.to_string()),
            ("metadata[checkout_session]", r.payment_id),
            ("amount", &r.amount.to_string()),
        ]);
        let v = self
            .call(
                Method::POST,
                "/v1/refunds",
                Some(&form),
                Some(r.idempotency_key),
            )
            .await?;
        refund(&v).ok_or_else(unexpected)
    }

    pub(super) async fn fetch_refund(
        &self,
        workspace_id: Uuid,
        payment_id: &str,
        refund_id: &str,
    ) -> Result<Intent, Error> {
        if !id_ok(refund_id) {
            return Err(Error::NotFound);
        }
        let v = self.get(&format!("/v1/refunds/{refund_id}")).await?;
        let m = &v["metadata"];
        if m["workspace_id"] != workspace_id.to_string().as_str()
            || m["checkout_session"] != payment_id
        {
            return Err(Error::NotFound);
        }
        refund(&v).ok_or_else(unexpected)
    }

    /// The session or refund id of a signed event; None (logged) when the signature fails.
    pub(super) fn webhook_id(&self, headers: &HeaderMap, body: &[u8]) -> Option<String> {
        let header = headers
            .get("stripe-signature")
            .and_then(|h| h.to_str().ok());
        let now = chrono::Utc::now().timestamp();
        if !header.is_some_and(|h| verify(&self.settings.webhook_secret, h, body, now)) {
            tracing::warn!("stripe webhook: missing, wrong or stale signature; ignored");
            return None;
        }
        event_id(body)
    }
}

/// True when `header` (`t=…,v1=…`) signs `body` with `secret` within 5 minutes of `now`.
pub fn verify(secret: &str, header: &str, body: &[u8], now: i64) -> bool {
    let mut t = None;
    let mut sigs = Vec::new();
    for part in header.split(',') {
        match part.split_once('=') {
            Some(("t", v)) => t = v.parse::<i64>().ok(),
            Some(("v1", v)) => sigs.push(v),
            _ => {}
        }
    }
    let Some(t) = t.filter(|t| (now - t).abs() <= TOLERANCE_SECS) else {
        return false;
    };
    sigs.iter().any(|s| {
        let (Ok(raw), Some(mac)) = (hex::decode(s), mac(secret, t, body)) else {
            return false;
        };
        mac.verify_slice(&raw).is_ok()
    })
}

/// The `Stripe-Signature` value for `body` at `t` (fakes and tests sign with it).
pub fn sign(secret: &str, t: i64, body: &[u8]) -> String {
    let sig = mac(secret, t, body).map(|m| hex::encode(m.finalize().into_bytes()));
    format!("t={t},v1={}", sig.unwrap_or_default())
}

/// HMAC-SHA256 over `"{t}.{body}"`, keyed by the whole `whsec_…` string as Stripe does.
fn mac(secret: &str, t: i64, body: &[u8]) -> Option<Hmac<Sha256>> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(format!("{t}.").as_bytes());
    mac.update(body);
    Some(mac)
}

/// A signed event's object id when it is a Checkout Session or a Refund.
fn event_id(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let o = &v["data"]["object"];
    let id = o["id"].as_str()?;
    let kind = matches!(o["object"].as_str(), Some("checkout.session" | "refund"));
    (kind && id_ok(id)).then(|| id.to_owned())
}

/// Stripe ids: letters, digits, `_`, at most 255 (safe in a URL path).
fn id_ok(id: &str) -> bool {
    (1..=255).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn session(v: &Value) -> Option<Intent> {
    let status = match (v["status"].as_str()?, v["payment_status"].as_str()) {
        ("complete", Some("paid" | "no_payment_required")) => Status::Paid,
        ("open" | "complete", _) => Status::Open,
        ("expired", _) => Status::Expired,
        _ => return None,
    };
    Some(Intent {
        id: v["id"].as_str()?.to_owned(),
        status,
        amount: v["amount_total"].as_i64()?,
        currency: v["currency"].as_str()?.to_ascii_uppercase(),
        checkout_url: v["url"].as_str().map(str::to_owned),
        metadata: v["metadata"].clone(),
    })
}

fn refund(v: &Value) -> Option<Intent> {
    let status = match v["status"].as_str()? {
        "pending" | "requires_action" => Status::Open,
        "succeeded" => Status::Refunded,
        "failed" => Status::Failed,
        "canceled" => Status::Canceled,
        _ => return None,
    };
    Some(Intent {
        id: v["id"].as_str()?.to_owned(),
        status,
        amount: v["amount"].as_i64()?,
        currency: v["currency"].as_str()?.to_ascii_uppercase(),
        checkout_url: None,
        metadata: v["metadata"].clone(),
    })
}

/// Our expiry (unix s) moved into Stripe's window: 30 min to 24 h ahead, a minute of margin.
fn expires_at(at: i64, now: i64) -> i64 {
    at.clamp(now + 31 * 60, now + 24 * 3600 - 60)
}

/// Metadata values as Stripe's strings.
fn metadata(m: &Value) -> Vec<(String, String)> {
    m.as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_str().map_or_else(|| v.to_string(), str::to_owned),
            )
        })
        .collect()
}

fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
    p.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// `application/x-www-form-urlencoded` body (reqwest's `form` feature is off).
fn encode(form: &[(String, String)]) -> String {
    let part = |s: &str| {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'*' => {
                    (b as char).to_string()
                }
                b' ' => "+".into(),
                _ => format!("%{b:02X}"),
            })
            .collect::<String>()
    };
    form.iter()
        .map(|(k, v)| format!("{}={}", part(k), part(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn unexpected() -> Error {
    Error::Unavailable("unexpected Stripe answer".into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const SECRET: &str = "whsec_test_secret";

    #[test]
    fn signatures_need_the_secret_the_body_and_a_fresh_time() {
        let body = br#"{"id":"evt_1"}"#;
        let header = sign(SECRET, 1_000, body);
        assert!(header.starts_with("t=1000,v1="), "{header}");
        assert!(verify(SECRET, &header, body, 1_000));
        assert!(verify(SECRET, &header, body, 1_300));
        assert!(verify(SECRET, &header, body, 700));
        assert!(!verify(SECRET, &header, body, 1_301), "stale");
        assert!(!verify(SECRET, &header, body, 699), "from the future");
        assert!(!verify("whsec_other", &header, body, 1_000), "wrong secret");
        assert!(
            !verify(SECRET, &header, br#"{"id":"evt_2"}"#, 1_000),
            "body changed"
        );
        let v1 = header.split_once(",v1=").unwrap().1;
        assert!(!verify(SECRET, &format!("v1={v1}"), body, 1_000), "no t");
        assert!(
            !verify(SECRET, &format!("t=999,v1={v1}"), body, 1_000),
            "t changed"
        );
        assert!(!verify(SECRET, "t=1000,v1=zz", body, 1_000), "not hex");
        assert!(!verify(SECRET, "", body, 1_000));
        // Stripe sends one v1 per active secret while one is rolled; any may match.
        let two = format!("t=1000,v1={},v0=abc,v1={v1}", "00".repeat(32));
        assert!(verify(SECRET, &two, body, 1_000));
    }

    #[test]
    fn events_name_a_session_or_refund_id_only() {
        let ev = |object: &str, id: &str| {
            serde_json::to_vec(&json!({ "id": "evt_1", "type": "x",
                "data": { "object": { "id": id, "object": object,
                                      "metadata": { "workspace_id": "elsewhere" } } } }))
            .unwrap()
        };
        assert_eq!(
            event_id(&ev("checkout.session", "cs_test_a1")),
            Some("cs_test_a1".into())
        );
        assert_eq!(event_id(&ev("refund", "re_1")), Some("re_1".into()));
        assert_eq!(event_id(&ev("charge", "ch_1")), None);
        assert_eq!(event_id(&ev("refund", "re_1/../x")), None);
        assert_eq!(event_id(&ev("refund", &"r".repeat(256))), None);
        assert_eq!(event_id(b"id=fk_1"), None);
    }

    #[test]
    fn sessions_map_to_status_amount_and_upper_case_currency() {
        let s = |status: &str, paid: &str| {
            json!({ "id": "cs_1", "object": "checkout.session", "status": status,
                    "payment_status": paid, "amount_total": 1250, "currency": "eur",
                    "url": if status == "open" { json!("https://checkout.stripe.com/c/1") } else { Value::Null },
                    "metadata": { "order_id": "o" } })
        };
        let open = session(&s("open", "unpaid")).unwrap();
        assert_eq!(
            (open.status, open.amount, open.currency.as_str()),
            (Status::Open, 1250, "EUR")
        );
        assert_eq!(
            open.checkout_url.as_deref(),
            Some("https://checkout.stripe.com/c/1")
        );
        assert_eq!(open.metadata, json!({ "order_id": "o" }));
        for (status, paid, want) in [
            ("complete", "paid", Status::Paid),
            ("complete", "no_payment_required", Status::Paid),
            // Async methods (SEPA debit): complete, money not in yet.
            ("complete", "unpaid", Status::Open),
            ("expired", "unpaid", Status::Expired),
        ] {
            assert_eq!(session(&s(status, paid)).unwrap().status, want, "{status}");
        }
        assert!(session(&s("weird", "paid")).is_none());
        assert!(session(&json!({ "id": "cs_1" })).is_none());
    }

    #[test]
    fn refunds_map_to_status() {
        let r = |status: &str| {
            json!({ "id": "re_1", "object": "refund", "status": status, "amount": 500,
                    "currency": "eur", "metadata": {} })
        };
        for (status, want) in [
            ("pending", Status::Open),
            ("requires_action", Status::Open),
            ("succeeded", Status::Refunded),
            ("failed", Status::Failed),
            ("canceled", Status::Canceled),
        ] {
            let i = refund(&r(status)).unwrap();
            assert_eq!(
                (i.status, i.amount, i.currency.as_str()),
                (want, 500, "EUR")
            );
        }
        assert!(refund(&r("weird")).is_none());
    }

    #[test]
    fn form_bodies_escape_brackets_spaces_and_symbols() {
        let form = pairs(&[("metadata[a b]", "x&y=z é"), ("mode", "payment")]);
        assert_eq!(
            encode(&form),
            "metadata%5Ba+b%5D=x%26y%3Dz+%C3%A9&mode=payment"
        );
    }

    #[test]
    fn session_expiry_stays_inside_stripes_window() {
        let now = 1_000_000;
        // Stripe takes 30 min to 24 h ahead; a minute of margin on both ends.
        assert_eq!(expires_at(now + 60, now), now + 31 * 60);
        assert_eq!(expires_at(now + 3600, now), now + 3600);
        assert_eq!(expires_at(now + 48 * 3600, now), now + 24 * 3600 - 60);
    }
}
