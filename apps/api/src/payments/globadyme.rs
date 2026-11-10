//! Globadyme's account-bound API. Financial POSTs are sent once; callers must commit
//! their dispatch claim first and retain ambiguous outcomes for reconciliation.

use std::{sync::Arc, time::Duration};

use base64::Engine;
use reqwest::{Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha512, digest::CtOutput};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use super::Error;
use crate::outbound::Outbound;

pub const SANDBOX_API_URL: &str = "https://api.sbx.globadyme.com/api";
const USER_AGENT: &str = "Genesy/0.1 Globadyme";
const MAX_BODY: usize = 2 * 1024 * 1024;

/// Immutable account revision; never share this client with another account or revision.
#[derive(Clone)]
pub struct Globadyme {
    account: Arc<Account>,
    token: Arc<Mutex<Option<Token>>>,
    http: Outbound,
    timeout: Duration,
}

struct Account {
    api_url: String,
    username: String,
    password: Zeroizing<String>,
    revision: i64,
}

struct Token {
    access: Zeroizing<String>,
    refresh: Zeroizing<String>,
    expires_at: Option<i64>,
}

impl std::fmt::Debug for Globadyme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Globadyme")
            .field("credential_revision", &self.account.revision)
            .finish_non_exhaustive()
    }
}

pub struct NewRequest<'a> {
    pub request_id: &'a str,
    pub merchant_id: &'a str,
    pub customer_id: &'a str,
    pub amount: i64,
    pub decimals: u32,
    pub currency: &'a str,
    pub country: &'a str,
    pub description: &'a str,
    pub account_holder_name: &'a str,
    pub email_address: &'a str,
    pub redirect_url: &'a str,
    pub link_validity_days: u32,
}

pub struct Requested {
    pub request_id: String,
    pub checkout_url: String,
    pub link_id: String,
}

impl std::fmt::Debug for Requested {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Requested { .. }")
    }
}

/// Each attempt is evidence, including unsuccessful attempts and settlement fields.
pub struct Transaction {
    pub id: String,
    pub created: String,
    pub status: String,
    pub amount: Decimal,
    pub currency: String,
    pub country: String,
    pub payment_method: Option<String>,
    pub account_holder_name: Option<String>,
    pub request_id: String,
    pub customer_id: Option<String>,
    pub settled: Option<bool>,
    pub settlement_date: Option<String>,
    pub settlement_id: Option<String>,
    pub raw: Value,
}

pub struct NewRefund<'a> {
    pub transaction_id: &'a str,
    pub amount: i64,
    pub decimals: u32,
    pub description: &'a str,
}

pub struct Refunded {
    pub raw: Value,
}

pub struct Chargeback {
    pub chargeback_id: String,
    pub transaction_id: String,
    pub merchant_id: String,
    pub created: String,
    pub status: String,
    pub amount: Decimal,
    pub currency: String,
    pub description: Option<String>,
    pub raw: Value,
}

pub struct ChargebackPage {
    pub total_pages: u64,
    pub total_elements: u64,
    pub current_page: u64,
    pub page_size: u64,
    pub last: bool,
    pub content: Vec<Chargeback>,
    pub raw: Value,
}

/// The published detail response has no more specific schema than an object.
pub struct ChargebackDetails {
    pub raw: Value,
}

impl Globadyme {
    /// Hosts come from operator configuration. Production URL approval happens upstream.
    pub fn new(
        api_url: String,
        username: String,
        password: String,
        revision: i64,
        http: Outbound,
    ) -> Result<Self, String> {
        let url = reqwest::Url::parse(&api_url).map_err(|_| "invalid Globadyme API URL")?;
        let loopback = cfg!(debug_assertions)
            && url.host_str().is_some_and(|h| {
                h.parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            });
        if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path().trim_end_matches('/') != "/api"
            || username.trim().is_empty()
            || password.is_empty()
            || revision < 1
        {
            return Err("invalid Globadyme account configuration".into());
        }
        Ok(Self {
            account: Arc::new(Account {
                api_url: api_url.trim_end_matches('/').to_owned(),
                username,
                password: Zeroizing::new(password),
                revision,
            }),
            token: Arc::new(Mutex::new(None)),
            http,
            timeout: Duration::from_secs(20),
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        token: Option<&str>,
    ) -> Result<(StatusCode, Vec<u8>, String), Error> {
        let mut request = self
            .http
            .request(method, &format!("{}{path}", self.account.api_url))
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json, image/png")
            .timeout(self.timeout);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }
        let mut response = self
            .http
            .send(request)
            .await
            .map_err(|_| Error::Unavailable("Globadyme transport failed".into()))?;
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_owned();
        if response
            .content_length()
            .is_some_and(|n| n > MAX_BODY as u64)
        {
            return Err(unexpected());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error::Unavailable("Globadyme response read failed".into()))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
                return Err(unexpected());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((status, bytes, content_type))
    }

    async fn authenticate(&self, refresh: Option<&str>) -> Result<Token, Error> {
        let (path, body) = match refresh {
            Some(token) => ("/api/auth/refresh-token", json!({"refreshToken":token})),
            None => (
                "/api/auth/sign-in",
                json!({"user":{"username":self.account.username,"password":self.account.password.as_str()}}),
            ),
        };
        let (status, bytes, _) = self.send(Method::POST, path, Some(&body), None).await?;
        let value = answer(status, &bytes, false)?;
        if value["tokenType"].as_str() != Some("Bearer") {
            return Err(unexpected());
        }
        let access = field(&value, "accessToken")?;
        let refresh = field(&value, "refreshToken")?;
        Ok(Token {
            expires_at: jwt_exp(&access),
            access: Zeroizing::new(access),
            refresh: Zeroizing::new(refresh),
        })
    }

    // Only authentication holds this mutex; financial requests never hold it.
    async fn access_token(&self, rejected: Option<&str>) -> Result<Zeroizing<String>, Error> {
        let mut cached = self.token.lock().await;
        let now = chrono::Utc::now().timestamp();
        if let Some(token) = cached.as_ref() {
            let rejected = rejected.is_some_and(|old| old == token.access.as_str());
            if !rejected && token.expires_at.is_some_and(|exp| exp > now + 30) {
                return Ok(token.access.clone());
            }
        }
        let fresh = if let Some(token) = cached.as_ref() {
            match self.authenticate(Some(&token.refresh)).await {
                Ok(token) => token,
                Err(Error::Refused(_)) => self.authenticate(None).await?,
                Err(e) => return Err(e),
            }
        } else {
            self.authenticate(None).await?
        };
        *cached = Some(fresh);
        if cached
            .as_ref()
            .is_some_and(|t| t.expires_at.is_some_and(|exp| exp <= now + 30))
        {
            let refresh = cached.as_ref().ok_or_else(unexpected)?.refresh.clone();
            let fresh = self.authenticate(Some(&refresh)).await?;
            if fresh.expires_at.is_some_and(|exp| exp <= now + 30) {
                return Err(Error::Unavailable("Globadyme token already expired".into()));
            }
            *cached = Some(fresh);
        }
        Ok(cached.as_ref().ok_or_else(unexpected)?.access.clone())
    }

    async fn get(&self, path: &str) -> Result<(Vec<u8>, String), Error> {
        let token = self.access_token(None).await?;
        let (mut status, mut bytes, mut content_type) =
            self.send(Method::GET, path, None, Some(&token)).await?;
        if matches!(status.as_u16(), 401 | 403) {
            let token = self.access_token(Some(&token)).await?;
            (status, bytes, content_type) =
                self.send(Method::GET, path, None, Some(&token)).await?;
        }
        if !status.is_success() {
            answer(status, &bytes, true)?;
        }
        Ok((bytes, content_type))
    }

    /// One financial POST. No retries, including authentication failures.
    async fn post(&self, path: &str, body: &Value) -> Result<Value, Error> {
        let token = self.access_token(None).await?;
        let (status, bytes, _) = self
            .send(Method::POST, path, Some(body), Some(&token))
            .await?;
        if matches!(status.as_u16(), 401 | 403) {
            let mut cached = self.token.lock().await;
            if let Some(cached) = cached
                .as_mut()
                .filter(|t| t.access.as_str() == token.as_str())
            {
                cached.expires_at = Some(0);
            }
        }
        answer(status, &bytes, false)
    }

    pub async fn request(&self, p: &NewRequest<'_>) -> Result<Requested, Error> {
        let amount = amount(p.amount, p.decimals)?;
        if !id_ok(p.request_id)
            || !id_ok(p.merchant_id)
            || !id_ok(p.customer_id)
            || p.currency.len() != 3
            || !p.currency.bytes().all(|c| c.is_ascii_uppercase())
            || p.country.len() != 2
            || !p.country.bytes().all(|c| c.is_ascii_uppercase())
            || p.account_holder_name.trim().is_empty()
            || p.link_validity_days == 0
            || p.link_validity_days > i32::MAX as u32
        {
            return Err(Error::Refused("invalid Globadyme payment input".into()));
        }
        let redirect = reqwest::Url::parse(p.redirect_url)
            .map_err(|_| Error::Refused("invalid return URL".into()))?;
        if redirect.scheme() != "https"
            || redirect.host_str().is_none()
            || !redirect.username().is_empty()
            || redirect.password().is_some()
        {
            return Err(Error::Refused("invalid return URL".into()));
        }
        let body = json!({"requestId":p.request_id,"merchantId":p.merchant_id,"customerId":p.customer_id,"amount":amount.to_string(),"currency":p.currency,"country":p.country,"description":p.description,"accountHolderName":p.account_holder_name,"emailAddress":p.email_address,"redirectUrlSuccess":p.redirect_url,"redirectUrlFailed":p.redirect_url,"redirectUrlAborted":p.redirect_url,"transactionRecurrenceType":"ONETIME","paymentLink":true,"linkValidityDays":p.link_validity_days});
        let result = self
            .post("/globadymeGateway/transaction/v2/requestPayment", &body)
            .await?;
        if result["message"].as_str() != Some("OK") {
            return Err(unexpected());
        }
        let details = &result["details"];
        let request_id = field(details, "requestId")?;
        if request_id != p.request_id {
            return Err(unexpected());
        }
        let checkout_url = field(details, "redirectUrl")?;
        let link_id = link_id(&checkout_url)?;
        if let Some(link) = details["linkUrl"].as_str()
            && link_id != self::link_id(link)?
        {
            return Err(unexpected());
        }
        Ok(Requested {
            request_id,
            checkout_url,
            link_id,
        })
    }

    pub async fn fetch(&self, request_id: &str) -> Result<Vec<Transaction>, Error> {
        check_id(request_id)?;
        let (bytes, _) = self
            .get(&format!(
                "/globadymeGateway/transaction/getPaymentsByRequestId/{request_id}"
            ))
            .await?;
        let rows: Value = serde_json::from_slice(&bytes).map_err(|_| unexpected())?;
        rows.as_array()
            .ok_or_else(unexpected)?
            .iter()
            .map(transaction)
            .collect()
    }

    /// Only an explicit SUCCEEDED response proves refund success; no lookup is invented.
    pub async fn refund(&self, r: &NewRefund<'_>) -> Result<Refunded, Error> {
        check_id(r.transaction_id)?;
        let exact = amount(r.amount, r.decimals)?
            .to_string()
            .parse::<serde_json::Number>()
            .map_err(|_| Error::Refused("invalid refund amount".into()))?;
        let body =
            json!({"transactionId":r.transaction_id,"amount":exact,"description":r.description});
        let result = self
            .post("/globadymeGateway/refund/initiateRefund", &body)
            .await?;
        if result["message"].as_str() != Some("refund")
            || result["details"]["status"].as_str() != Some("SUCCEEDED")
        {
            return Err(unexpected());
        }
        Ok(Refunded { raw: result })
    }

    pub async fn qr(&self, link_id: &str) -> Result<Vec<u8>, Error> {
        check_id(link_id)?;
        let (bytes, content_type) = self
            .get(&format!(
                "/globadymeGateway/transaction/qrcode?id={link_id}"
            ))
            .await?;
        if content_type.split(';').next() != Some("image/png")
            || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        {
            return Err(unexpected());
        }
        Ok(bytes)
    }

    pub async fn chargebacks(&self, page: u32, limit: u32) -> Result<ChargebackPage, Error> {
        if page > i32::MAX as u32 || limit == 0 || limit > 100 {
            return Err(Error::Refused("invalid chargeback pagination".into()));
        }
        let (bytes, _) = self
            .get(&format!(
                "/globadymeGateway/chargeback/getChargebacks?pageNumber={page}&pageSize={limit}"
            ))
            .await?;
        let raw: Value = serde_json::from_slice(&bytes).map_err(|_| unexpected())?;
        let content = raw["content"]
            .as_array()
            .ok_or_else(unexpected)?
            .iter()
            .map(chargeback)
            .collect::<Result<_, _>>()?;
        Ok(ChargebackPage {
            total_pages: unsigned(&raw, "totalPages")?,
            total_elements: unsigned(&raw, "totalElements")?,
            current_page: unsigned(&raw, "currentPage")?,
            page_size: unsigned(&raw, "pageSize")?,
            last: raw["last"].as_bool().ok_or_else(unexpected)?,
            content,
            raw,
        })
    }

    pub async fn chargeback(&self, id: &str) -> Result<ChargebackDetails, Error> {
        check_id(id)?;
        let (bytes, _) = self
            .get(&format!(
                "/globadymeGateway/chargeback/getChargebackDetails/{id}"
            ))
            .await?;
        let raw: Value = serde_json::from_slice(&bytes).map_err(|_| unexpected())?;
        if !raw.is_object() || !raw["details"].is_object() {
            return Err(unexpected());
        }
        Ok(ChargebackDetails { raw })
    }
}

fn unexpected() -> Error {
    Error::Unavailable("unexpected Globadyme response".into())
}

fn answer(status: StatusCode, bytes: &[u8], read: bool) -> Result<Value, Error> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| unexpected())?;
    if status.is_success() {
        return Ok(value);
    }
    if read && status == StatusCode::NOT_FOUND {
        return Err(Error::NotFound);
    }
    // ErrorDTO with matching status establishes an explicit rejection, not acceptance.
    if matches!(status.as_u16(), 400 | 403)
        && value["httpStatus"].as_u64() == Some(status.as_u16() as u64)
        && value["message"]
            .as_str()
            .is_some_and(|m| !m.trim().is_empty())
    {
        return Err(Error::Refused(format!(
            "Globadyme rejected request ({status})"
        )));
    }
    Err(Error::Unavailable(format!("Globadyme answered {status}")))
}

fn field(value: &Value, key: &str) -> Result<String, Error> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 2048)
        .map(str::to_owned)
        .ok_or_else(unexpected)
}

fn optional(value: &Value, key: &str) -> Result<Option<String>, Error> {
    match &value[key] {
        Value::Null => Ok(None),
        Value::String(s) if s.len() <= 2048 => Ok(Some(s.clone())),
        _ => Err(unexpected()),
    }
}

fn unsigned(value: &Value, key: &str) -> Result<u64, Error> {
    value[key].as_u64().ok_or_else(unexpected)
}

fn decimal(value: &Value) -> Result<Decimal, Error> {
    let Value::Number(n) = value else {
        return Err(unexpected());
    };
    let text = n.to_string();
    if let Some((mantissa, _)) = text.split_once(['e', 'E']) {
        // from_scientific parses its mantissa with rounding; reject inexact input first.
        Decimal::from_str_exact(mantissa).map_err(|_| unexpected())?;
        Decimal::from_scientific(&text).map_err(|_| unexpected())
    } else {
        Decimal::from_str_exact(&text).map_err(|_| unexpected())
    }
}

fn amount(minor: i64, decimals: u32) -> Result<Decimal, Error> {
    if minor <= 0 || decimals != 2 {
        return Err(Error::Refused(
            "Globadyme requires a positive two-decimal amount".into(),
        ));
    }
    Ok(Decimal::new(minor, 2))
}

fn transaction(v: &Value) -> Result<Transaction, Error> {
    let settled = match &v["settled"] {
        Value::Null => None,
        Value::Bool(b) => Some(*b),
        _ => return Err(unexpected()),
    };
    Ok(Transaction {
        id: field(v, "id")?,
        created: field(v, "created")?,
        status: field(v, "status")?,
        amount: decimal(&v["amount"])?,
        currency: field(v, "currency")?,
        country: field(v, "country")?,
        payment_method: optional(v, "paymentMethod")?,
        account_holder_name: optional(v, "accountholderName")?,
        request_id: field(v, "requestId")?,
        customer_id: optional(v, "customerId")?,
        settled,
        settlement_date: optional(v, "settlementDate")?,
        settlement_id: optional(v, "settlementId")?,
        raw: v.clone(),
    })
}

fn chargeback(v: &Value) -> Result<Chargeback, Error> {
    Ok(Chargeback {
        chargeback_id: field(v, "chargebackId")?,
        transaction_id: field(v, "transactionId")?,
        merchant_id: field(v, "merchantId")?,
        created: field(v, "created")?,
        status: field(v, "status")?,
        amount: decimal(&v["amount"])?,
        currency: field(v, "currency")?,
        description: optional(v, "description")?,
        raw: v.clone(),
    })
}

fn id_ok(id: &str) -> bool {
    (1..=255).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn check_id(id: &str) -> Result<(), Error> {
    if id_ok(id) {
        Ok(())
    } else {
        Err(Error::Refused("invalid Globadyme identifier".into()))
    }
}

fn link_id(link: &str) -> Result<String, Error> {
    let url = reqwest::Url::parse(link).map_err(|_| unexpected())?;
    if url.scheme() != "https"
        || url.host_str() != Some("api.sbx.globadyme.com")
        || url.port().is_some()
        || url.path() != "/api/link"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(unexpected());
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    if pairs.len() != 1 || pairs[0].0 != "id" || !id_ok(&pairs[0].1) {
        return Err(unexpected());
    }
    Ok(pairs[0].1.to_string())
}

fn jwt_exp(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice::<Value>(&bytes).ok()?["exp"].as_i64()
}

/// Verify SHA-512(raw body || secret) in constant time before reading untrusted JSON.
pub fn verify_callback(
    raw: &[u8],
    provided: &str,
    current: &str,
    previous: Option<&str>,
) -> Option<String> {
    if provided.len() != 128 {
        return None;
    }
    let supplied: [u8; 64] = hex::decode(provided).ok()?.try_into().ok()?;
    let supplied = CtOutput::<Sha512>::new(supplied.into());
    let mut valid = false;
    for secret in [Some(current), previous]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
    {
        let mut hash = Sha512::new();
        hash.update(raw);
        hash.update(secret.as_bytes());
        valid |= CtOutput::<Sha512>::new(hash.finalize()) == supplied;
    }
    if !valid {
        return None;
    }
    let value: Value = serde_json::from_slice(raw).ok()?;
    let id = value["requestId"].as_str()?;
    id_ok(id).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Bytes,
        extract::State,
        http::{HeaderMap, StatusCode, Uri},
        response::IntoResponse,
        routing::any,
    };
    use base64::Engine;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct Fake {
        seen: StdMutex<Vec<(String, String, Value)>>,
        expire_get: std::sync::atomic::AtomicBool,
        fail_post: std::sync::atomic::AtomicU16,
        delay_post: std::sync::atomic::AtomicBool,
        malformed_post: std::sync::atomic::AtomicBool,
        oversized_post: std::sync::atomic::AtomicBool,
        pending_refund: std::sync::atomic::AtomicBool,
        expired_token: std::sync::atomic::AtomicBool,
    }

    fn jwt(exp: i64) -> String {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(json!({"exp": exp}).to_string());
        format!("header.{payload}.signature")
    }

    async fn handler(
        State(f): State<Arc<Fake>>,
        uri: Uri,
        headers: HeaderMap,
        body: Bytes,
    ) -> axum::response::Response {
        use std::sync::atomic::Ordering::SeqCst;
        assert_eq!(headers["user-agent"], USER_AGENT);
        let path = uri.path();
        let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        f.seen.lock().unwrap().push((
            uri.to_string(),
            headers
                .get("authorization")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("")
                .to_owned(),
            value,
        ));
        if path.ends_with("/api/auth/sign-in") || path.ends_with("/api/auth/refresh-token") {
            let exp = if f.expired_token.swap(false, SeqCst) {
                0
            } else {
                chrono::Utc::now().timestamp() + 3600
            };
            return axum::Json(json!({"accessToken":jwt(exp),"refreshToken":"refresh-secret","tokenType":"Bearer"})).into_response();
        }
        assert!(
            headers
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("Bearer header.")
        );
        if path.ends_with("requestPayment") || path.ends_with("initiateRefund") {
            if f.delay_post.load(SeqCst) {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            let status = f.fail_post.load(SeqCst);
            if status != 0 {
                return (
                    StatusCode::from_u16(status).unwrap(),
                    axum::Json(json!({"message":"Forbidden","httpStatus":status})),
                )
                    .into_response();
            }
            if f.malformed_post.load(SeqCst) {
                return (StatusCode::OK, "not json").into_response();
            }
            if f.oversized_post.load(SeqCst) {
                return (StatusCode::OK, vec![b' '; MAX_BODY + 1]).into_response();
            }
            let result = if path.ends_with("initiateRefund") {
                json!({"message":"refund","details":{"status":if f.pending_refund.load(SeqCst) {"PENDING"} else {"SUCCEEDED"}}})
            } else {
                json!({"message":"OK","details":{"requestId":"req-1","redirectUrl":"https://api.sbx.globadyme.com/api/link?id=link_1","linkUrl":"https://api.sbx.globadyme.com/api/link?id=link_1"}})
            };
            return axum::Json(result).into_response();
        }
        if f.expire_get.swap(false, SeqCst) {
            return (
                StatusCode::FORBIDDEN,
                axum::Json(json!({"message":"Forbidden","httpStatus":403})),
            )
                .into_response();
        }
        if path.ends_with("qrcode") {
            return (
                [("content-type", "image/png")],
                b"\x89PNG\r\n\x1a\nfixture".to_vec(),
            )
                .into_response();
        }
        if path.ends_with("getChargebacks") {
            return axum::Json(json!({"totalPages":1,"totalElements":1,"currentPage":0,"pageSize":50,"elements":1,"first":true,"last":true,"content":[{"chargebackId":"cb-1","transactionId":"tx-1","merchantId":"merchant","amount":12.50,"currency":"EUR","status":"OPEN","created":"2026-11-10T00:00:00Z"}]})).into_response();
        }
        if path.contains("getChargebackDetails") {
            return axum::Json(
                json!({"message":"OK","details":{"chargebackId":"cb-1","status":"OPEN"}}),
            )
            .into_response();
        }
        ([("content-type", "application/json")], r#"[{"id":"tx-1","created":"2026-11-10T00:00:00Z","status":"PROCESSED","amount":90071992547409.91,"currency":"EUR","country":"NL","requestId":"req-1","customerId":"order-1","settled":true,"settlementId":"settle-1","settlementDate":"2026-11-11T00:00:00Z"},{"id":"tx-2","created":"2026-11-10T00:01:00Z","status":"DECLINED","amount":12.50,"currency":"EUR","country":"NL","requestId":"req-1"}]"#).into_response()
    }

    async fn world() -> (Globadyme, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        let app = Router::new()
            .fallback(any(handler))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/api", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Globadyme::new(
            url,
            "user-secret".into(),
            "password-secret".into(),
            7,
            Outbound::new(&["127.0.0.1".into()]).unwrap(),
        )
        .unwrap()
        .with_timeout(Duration::from_millis(100));
        (client, fake)
    }

    fn request() -> NewRequest<'static> {
        NewRequest {
            request_id: "req-1",
            merchant_id: "merchant",
            customer_id: "order-1",
            amount: 1250,
            decimals: 2,
            currency: "EUR",
            country: "NL",
            description: "Order 1",
            account_holder_name: "Kim",
            email_address: "kim@example.com",
            redirect_url: "https://shop.example.com/order",
            link_validity_days: 1,
        }
    }

    #[tokio::test]
    async fn exact_payment_shape_auth_paths_and_shared_revision_cache() {
        let (client, fake) = world().await;
        let result = client.request(&request()).await.unwrap();
        assert_eq!(
            (result.request_id.as_str(), result.link_id.as_str()),
            ("req-1", "link_1")
        );
        client.clone().fetch("req-1").await.unwrap();
        let seen = fake.seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0].0, "/api/api/auth/sign-in");
        assert_eq!(
            seen[0].2,
            json!({"user":{"username":"user-secret","password":"password-secret"}})
        );
        assert_eq!(seen[1].2["amount"], "12.50");
        assert_eq!(seen[1].2["requestId"], "req-1");
        assert_eq!(seen[1].2["paymentLink"], true);
        assert_eq!(seen[1].2["transactionRecurrenceType"], "ONETIME");
        assert_eq!(seen[1].2["redirectUrlFailed"], request().redirect_url);
        let debug = format!("{client:?}");
        assert!(
            !debug.contains("password-secret")
                && !debug.contains("user-secret")
                && !debug.contains("refresh-secret")
        );
    }

    #[tokio::test]
    async fn expiry_refreshes_before_post_and_get_may_refresh_once() {
        use std::sync::atomic::Ordering::SeqCst;
        let (client, fake) = world().await;
        fake.expired_token.store(true, SeqCst);
        client.fetch("req-1").await.unwrap();
        client.request(&request()).await.unwrap();
        fake.expire_get.store(true, SeqCst);
        client.fetch("req-1").await.unwrap();
        let seen = fake.seen.lock().unwrap();
        assert_eq!(
            seen.iter()
                .filter(|r| r.0.ends_with("refresh-token"))
                .count(),
            2
        );
        assert_eq!(
            seen.iter()
                .filter(|r| r.0.ends_with("requestPayment"))
                .count(),
            1
        );
        assert!(
            seen.iter()
                .filter(|r| r.0.ends_with("refresh-token"))
                .all(|r| r.2 == json!({"refreshToken":"refresh-secret"}))
        );
    }

    #[tokio::test]
    async fn financial_post_is_never_replayed_on_rejection_timeout_or_bad_body() {
        use std::sync::atomic::Ordering::SeqCst;
        for refund in [false, true] {
            for mode in 0..4 {
                let (client, fake) = world().await;
                match mode {
                    0 => fake.fail_post.store(403, SeqCst),
                    1 => fake.delay_post.store(true, SeqCst),
                    2 => fake.malformed_post.store(true, SeqCst),
                    _ => fake.oversized_post.store(true, SeqCst),
                }
                let result = if refund {
                    client
                        .refund(&NewRefund {
                            transaction_id: "tx-1",
                            amount: 1250,
                            decimals: 2,
                            description: "Refund",
                        })
                        .await
                        .map(|_| ())
                } else {
                    client.request(&request()).await.map(|_| ())
                };
                let err = result.unwrap_err();
                assert_eq!(matches!(err, Error::Refused(_)), mode == 0);
                assert_eq!(
                    fake.seen
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|r| r.0.ends_with(if refund {
                            "initiateRefund"
                        } else {
                            "requestPayment"
                        }))
                        .count(),
                    1
                );
            }
        }
    }

    #[tokio::test]
    async fn refund_requires_positive_success_evidence() {
        let (client, fake) = world().await;
        fake.pending_refund
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(
            client
                .refund(&NewRefund {
                    transaction_id: "tx-1",
                    amount: 1250,
                    decimals: 2,
                    description: "Refund"
                })
                .await,
            Err(Error::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn authentication_cache_is_single_flight_and_revision_bound() {
        let (client, fake) = world().await;
        let (first, second) = tokio::join!(client.fetch("req-1"), client.fetch("req-1"));
        assert!(first.is_ok() && second.is_ok());
        assert_eq!(
            fake.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.0.ends_with("sign-in"))
                .count(),
            1
        );
        let revised = Globadyme::new(
            client.account.api_url.clone(),
            "new-user".into(),
            "new-password".into(),
            8,
            client.http.clone(),
        )
        .unwrap();
        revised.fetch("req-1").await.unwrap();
        assert_eq!(
            fake.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.0.ends_with("sign-in"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn preserves_every_transaction_exact_amount_and_settlement() {
        let (client, _) = world().await;
        let tx = client.fetch("req-1").await.unwrap();
        assert_eq!(tx.len(), 2);
        assert_eq!(tx[0].amount.to_string(), "90071992547409.91");
        assert_eq!(tx[0].settled, Some(true));
        assert_eq!(tx[0].settlement_id.as_deref(), Some("settle-1"));
        assert_eq!(tx[1].status, "DECLINED");
        assert_eq!(tx[1].settled, None);
    }

    #[tokio::test]
    async fn refund_uses_exact_number_qr_fixed_endpoint_chargebacks_paginate() {
        let (client, fake) = world().await;
        client
            .refund(&NewRefund {
                transaction_id: "tx-1",
                amount: i64::MAX,
                decimals: 2,
                description: "Refund",
            })
            .await
            .unwrap();
        assert!(client.qr("link_1").await.unwrap().starts_with(b"\x89PNG"));
        let page = client.chargebacks(0, 50).await.unwrap();
        assert_eq!(page.content[0].chargeback_id, "cb-1");
        client.chargeback("cb-1").await.unwrap();
        let seen = fake.seen.lock().unwrap();
        assert_eq!(seen[1].2["amount"].to_string(), "92233720368547758.07");
        assert!(
            seen.iter()
                .any(|r| r.0 == "/api/globadymeGateway/transaction/qrcode?id=link_1")
        );
        assert!(
            seen.iter().any(|r| r.0
                == "/api/globadymeGateway/chargeback/getChargebacks?pageNumber=0&pageSize=50")
        );
    }

    #[tokio::test]
    async fn rejects_unsupported_money_and_unsafe_identifiers_before_auth() {
        let (client, fake) = world().await;
        for (amount, decimals) in [(0, 2), (-1, 2), (100, 0), (100, 3)] {
            let mut input = request();
            input.amount = amount;
            input.decimals = decimals;
            assert!(matches!(
                client.request(&input).await,
                Err(Error::Refused(_))
            ));
        }
        assert!(client.fetch("../elsewhere").await.is_err());
        assert!(client.qr("https://evil.example").await.is_err());
        assert!(fake.seen.lock().unwrap().is_empty());
        assert!(link_id("https://evil.example/api/link?id=one").is_err());
        assert!(link_id("https://api.sbx.globadyme.com/api/link?id=one&id=two").is_err());
    }

    #[test]
    fn verifies_raw_callback_before_parsing_with_rotation() {
        let raw = br#"{"requestId":"req-1"}"#;
        let sign = |secret: &str| {
            let mut h = Sha512::new();
            h.update(raw);
            h.update(secret.as_bytes());
            hex::encode(h.finalize())
        };
        assert_eq!(
            verify_callback(raw, &sign("current"), "current", Some("previous")),
            Some("req-1".into())
        );
        assert_eq!(
            verify_callback(raw, &sign("previous"), "current", Some("previous")),
            Some("req-1".into())
        );
        assert!(verify_callback(b"bad", &sign("current"), "current", None).is_none());
        assert!(verify_callback(raw, "00", "current", None).is_none());
        assert!(verify_callback(raw, &sign("wrong"), "current", None).is_none());
        assert!(verify_callback(raw, &sign(""), "", None).is_none());
    }

    #[test]
    fn response_numbers_never_round() {
        let exact: Value = serde_json::from_str("123.456e-2").unwrap();
        assert_eq!(decimal(&exact).unwrap().to_string(), "1.23456");
        for value in [
            "1.00000000000000000000000000001",
            "1.00000000000000000000000000001e2",
            "1e-29",
            "79228162514264337593543950336",
        ] {
            assert!(
                decimal(&serde_json::from_str::<Value>(value).unwrap()).is_err(),
                "{value}"
            );
        }
    }
}
