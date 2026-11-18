//! NetSuite OAuth 2.0 client credentials (M2M): ES256 client assertion signed with `ring`, token
//! POST, and a process-wide token cache valid until expiry − 60 s. Facts: `docs/integrations/facts/
//! netsuite-rest.md` "JWT" (l.42-93). Tokens and keys are never logged or persisted.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use ring::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair},
};
use serde_json::json;

use super::{Client, KeySource, errors};
use crate::{
    integrations::{
        capability::{ErrorClass, ProviderError},
        connections::secrets,
    },
    modules::auth::token::PlainSecret,
};

/// Assertion lifetime: under the documented 60-minute `exp − iat` cap (l.66).
const ASSERTION_SECS: u64 = 1800;
/// A cached token is dropped this long before NetSuite's expiry.
const EARLY: Duration = Duration::from_secs(60);
/// Path of the token endpoint (l.48).
pub const TOKEN_PATH: &str = "/services/rest/auth/oauth2/v1/token";
// `client_assertion_type` pre-encoded for x-www-form-urlencoded (l.49-51).
const FORM_PREFIX: &str = "grant_type=client_credentials&client_assertion_type=urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer&client_assertion=";

type Cache = Mutex<HashMap<String, (Arc<PlainSecret>, Instant)>>;
static TOKENS: LazyLock<Cache> = LazyLock::new(Cache::default);

fn auth_err(detail: impl Into<String>) -> ProviderError {
    errors::err(ErrorClass::Auth, detail, None, None)
}

/// DER of a PKCS#8 `BEGIN PRIVATE KEY` PEM. SEC1 and encrypted keys are refused.
pub(crate) fn pkcs8_der(pem: &str) -> Result<Vec<u8>, ProviderError> {
    if pem.contains("BEGIN EC PRIVATE KEY") {
        return Err(auth_err(
            "The private key is SEC1. Convert it with `openssl pkcs8 -topk8 -nocrypt`.",
        ));
    }
    if !pem.contains("-----BEGIN PRIVATE KEY-----") {
        return Err(auth_err(
            "The private key must be an unencrypted PKCS#8 PEM (BEGIN PRIVATE KEY).",
        ));
    }
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("-----"))
        .collect();
    STANDARD
        .decode(body)
        .map_err(|_| auth_err("The private key PEM is not valid base64."))
}

/// Signed ES256 client assertion (l.56-68): header `typ`/`alg`/`kid`, claims `iss`, `scope`,
/// `aud`, `iat`, `exp`. Signature is raw R‖S (JWS, RFC 7518 §3.4); NetSuite's form is UNVERIFIED (l.82).
pub(crate) fn assertion(
    pem: &str,
    kid: &str,
    iss: &str,
    aud: &str,
    iat: u64,
) -> Result<String, ProviderError> {
    let der = pkcs8_der(pem)?;
    let rng = SystemRandom::new();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &rng)
        .map_err(|_| auth_err("The private key is not an EC P-256 PKCS#8 key."))?;
    let header = json!({"typ": "JWT", "alg": "ES256", "kid": kid});
    // Text form of `scope` (l.64); the sample's array form is UNVERIFIED (l.73).
    let claims = json!({
        "iss": iss,
        "scope": "rest_webservices",
        "aud": aud,
        "iat": iat,
        "exp": iat + ASSERTION_SECS,
    });
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let sig = pair
        .sign(&rng, input.as_bytes())
        .map_err(|_| auth_err("Signing the client assertion failed."))?;
    Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig.as_ref())))
}

impl Client {
    fn token_url(&self) -> String {
        format!("{}{TOKEN_PATH}", self.base)
    }

    fn cache_key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.token_url(),
            self.client_id,
            self.certificate_id
        )
    }

    /// Drop the cached token (after NetSuite refused it).
    pub(crate) fn forget_token(&self) {
        if let Ok(mut map) = TOKENS.lock() {
            map.remove(&self.cache_key());
        }
    }

    /// A token with at least 60 s left: cached, else fetched.
    pub(crate) async fn token(&self) -> Result<Arc<PlainSecret>, ProviderError> {
        let key = self.cache_key();
        if let Some((tok, until)) = TOKENS.lock().ok().and_then(|m| m.get(&key).cloned())
            && Instant::now() < until
        {
            return Ok(tok);
        }
        let (tok, ttl) = self.fetch_token().await?;
        let tok = Arc::new(tok);
        if let Some(until) = Instant::now().checked_add(ttl.saturating_sub(EARLY))
            && let Ok(mut map) = TOKENS.lock()
        {
            map.insert(key, (tok.clone(), until));
        }
        Ok(tok)
    }

    async fn pem(&self) -> Result<PlainSecret, ProviderError> {
        match &self.key {
            KeySource::Pem(p) => Ok(PlainSecret::new(p.expose().to_owned())),
            KeySource::Sealed {
                db,
                key,
                ws,
                connection_id,
            } => secrets::open(db, key, *ws, *connection_id, "private_key")
                .await
                .map_err(|e| match e {
                    crate::error::AppError::NotFound => {
                        auth_err("No private key is stored for this connection.")
                    }
                    _ => errors::err(
                        ErrorClass::Unknown,
                        "The private key could not be opened.",
                        None,
                        None,
                    ),
                }),
        }
    }

    /// Token POST (l.44-54). `invalid_grant` → fresh assertion, once; then Auth.
    async fn fetch_token(&self) -> Result<(PlainSecret, Duration), ProviderError> {
        let pem = self.pem().await?;
        let url = self.token_url();
        let mut last = None;
        for _ in 0..2 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let jwt = assertion(
                pem.expose(),
                &self.certificate_id,
                &self.client_id,
                &url,
                now,
            )?;
            let req = self
                .outbound
                .request(reqwest::Method::POST, &url)
                .timeout(self.timeout)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(format!("{FORM_PREFIX}{jwt}"));
            // A token call has no side effect: every transport failure is Transient.
            let reply = self.send(req).await.map_err(errors::for_read)?;
            let body: serde_json::Value = serde_json::from_slice(&reply.body).unwrap_or_default();
            if reply.status == 200 {
                let tok = body["access_token"]
                    .as_str()
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| auth_err("The token reply has no access_token."))?;
                let secs = body["expires_in"].as_u64().unwrap_or(3600);
                return Ok((PlainSecret::new(tok.to_owned()), Duration::from_secs(secs)));
            }
            let code = body["error"].as_str().map(str::to_owned);
            let class = match reply.status {
                429 | 500.. => ErrorClass::Transient,
                _ => ErrorClass::Auth,
            };
            let e = errors::err(
                class,
                format!(
                    "NetSuite refused the token request ({}).",
                    code.as_deref().unwrap_or("no error code")
                ),
                code.clone(),
                Some(reply.status),
            );
            if code.as_deref() != Some("invalid_grant") {
                return Err(e);
            }
            last = Some(e);
        }
        Err(last.unwrap_or_else(|| auth_err("NetSuite refused the token request.")))
    }
}

#[cfg(test)]
mod tests {
    use ring::signature::{ECDSA_P256_SHA256_FIXED, KeyPair, UnparsedPublicKey};

    use super::*;

    const KEY: &str = include_str!("../../../../tests/fixtures/netsuite_es256_test_key.pem");

    fn b64(s: &str) -> Vec<u8> {
        URL_SAFE_NO_PAD.decode(s).unwrap()
    }

    // RFC 7515 Appendix A.3: ES256 example JWS, verified with the published public key.
    // Proves the signing input + base64url + raw R‖S shape this module produces.
    #[test]
    fn rfc7515_a3_known_vector_verifies() {
        let x = b64("f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU");
        let y = b64("x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0");
        let public = [vec![4u8], x, y].concat();
        let input = "eyJhbGciOiJFUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ";
        let sig = b64(
            "DtEhU3ljbEg8L38VWAfUAqOyKAM6-Xx-F4GawxaepmXFCgfTjDxw5djxLa8ISlSApmWQxfKTUJqPP3-Kg6NU1Q",
        );
        assert_eq!(sig.len(), 64, "raw R‖S");
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, &public)
            .verify(input.as_bytes(), &sig)
            .expect("RFC 7515 A.3 signature verifies");
    }

    #[test]
    fn assertion_matches_netsuite_fields_and_verifies() {
        let aud = "https://123456.suitetalk.api.netsuite.com/services/rest/auth/oauth2/v1/token";
        let jwt = assertion(KEY, "cert-1", "client-1", aud, 1_700_000_000).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: serde_json::Value = serde_json::from_slice(&b64(parts[0])).unwrap();
        assert_eq!(
            header,
            json!({"typ": "JWT", "alg": "ES256", "kid": "cert-1"})
        );
        let claims: serde_json::Value = serde_json::from_slice(&b64(parts[1])).unwrap();
        assert_eq!(claims["iss"], "client-1");
        assert_eq!(claims["scope"], "rest_webservices");
        assert_eq!(claims["aud"], aud);
        assert_eq!(claims["iat"], 1_700_000_000);
        let ttl = claims["exp"].as_u64().unwrap() - 1_700_000_000;
        assert!(ttl > 0 && ttl < 3600, "exp < iat + 60 min");
        let sig = b64(parts[2]);
        assert_eq!(sig.len(), 64, "raw R‖S, not DER");
        let der = pkcs8_der(KEY).unwrap();
        let pair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &SystemRandom::new())
                .unwrap();
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, pair.public_key().as_ref())
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
            .expect("own signature verifies");
    }

    #[test]
    fn sec1_and_garbage_keys_are_refused() {
        let sec1 = "-----BEGIN EC PRIVATE KEY-----\nAAAA\n-----END EC PRIVATE KEY-----";
        let e = assertion(sec1, "k", "i", "a", 0).unwrap_err();
        assert_eq!(e.class, ErrorClass::Auth);
        assert!(e.detail.contains("pkcs8"));
        let bad = "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----";
        assert_eq!(
            assertion(bad, "k", "i", "a", 0).unwrap_err().class,
            ErrorClass::Auth
        );
        assert!(assertion("hello", "k", "i", "a", 0).is_err());
    }
}
