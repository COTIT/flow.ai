//! Rate limits: per client IP on public auth routes (signup, login, register, invite status),
//! per session (bearer token) on authenticated routes.

use std::{
    net::{IpAddr, Ipv6Addr},
    time::Duration,
};

use axum::{
    Json, Router,
    body::Body,
    extract::FromRequestParts,
    http::{HeaderValue, Request, Response, StatusCode, header::RETRY_AFTER, request::Parts},
    response::IntoResponse,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tower_governor::{
    GovernorError, GovernorLayer,
    governor::GovernorConfigBuilder,
    key_extractor::{KeyExtractor, PeerIpKeyExtractor, SmartIpKeyExtractor},
};

use crate::{
    AppState,
    error::{AppError, TOO_MANY},
    http::{app_key, auth::bearer},
};

/// How often stale per-IP buckets are dropped, so limiter memory can't grow without bound.
const CLEANUP_EVERY: Duration = Duration::from_secs(60);

/// Token bucket per client IP: `burst` requests at once, one more every `replenish`.
#[derive(Debug, Clone, Copy)]
pub struct RateLimit {
    pub burst: u32,
    pub replenish: Duration,
    /// Key on X-Forwarded-For / X-Real-IP / Forwarded. Only true behind a proxy that sets
    /// them (Traefik); otherwise any client could rotate the header to dodge the limit.
    pub trust_forwarded: bool,
}

/// Peer IP, or proxy-reported client IP when trusted.
#[derive(Debug, Clone, Copy)]
struct ClientIp {
    trust_forwarded: bool,
}

impl KeyExtractor for ClientIp {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<IpAddr, GovernorError> {
        let ip = if self.trust_forwarded {
            SmartIpKeyExtractor.extract(req)?
        } else {
            PeerIpKeyExtractor.extract(req)?
        };
        Ok(bucket(ip))
    }
}

/// The client address the public limiter keys on (IPv6: its /64), for audit rows. None when
/// the request carries none (tests without a peer).
pub struct ClientAddr(pub Option<String>);

impl FromRequestParts<AppState> for ClientAddr {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let mut req = Request::new(());
        *req.headers_mut() = parts.headers.clone();
        *req.extensions_mut() = parts.extensions.clone();
        let key = ClientIp {
            trust_forwarded: state.rate_limit.trust_forwarded,
        };
        Ok(Self(key.extract(&req).ok().map(|ip| ip.to_string())))
    }
}

/// Rate-limit key for an address. IPv6: the /64 prefix, since one customer gets a whole /64 and
/// could rotate through it for free. IPv4 (and IPv4-mapped IPv6): the address itself.
fn bucket(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
        v4 => v4,
    }
}

/// Client IP, or one shared bucket when the peer is unknown (server without connect info).
#[derive(Debug, Clone, Copy)]
struct AnyIp(ClientIp);

impl KeyExtractor for AnyIp {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<IpAddr, GovernorError> {
        Ok(self
            .0
            .extract(req)
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)))
    }
}

/// Bucket of an authenticated request: its bearer token (hashed, parsed exactly as auth parses
/// it), so users behind one office NAT don't share a bucket. Malformed or missing token: the
/// client IP.
#[derive(Debug, Clone, Copy)]
struct Session(AnyIp);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum SessionKey {
    Token([u8; 32]),
    Shop(uuid::Uuid),
    Ip(IpAddr),
}

impl KeyExtractor for Session {
    type Key = SessionKey;

    fn extract<T>(&self, req: &Request<T>) -> Result<SessionKey, GovernorError> {
        // Public Shop calls share their managed actor's bucket; rotating headers cannot evade it.
        let path = req
            .uri()
            .path()
            .strip_prefix("/v1")
            .unwrap_or(req.uri().path());
        if let Some(id) = path
            .strip_prefix("/storefront/shops/")
            .and_then(|rest| rest.split('/').next())
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
        {
            return Ok(SessionKey::Shop(id));
        }
        // App keys get their own bucket too: a shop server shouldn't share its IP's.
        Ok(
            match bearer(req.headers()).or_else(|| app_key::bearer(req.headers())) {
                Some(t) => SessionKey::Token(Sha256::digest(t.as_bytes()).into()),
                None => SessionKey::Ip(self.0.extract(req)?),
            },
        )
    }
}

/// Wrap `router` in the per-IP limit. Spawns a cleanup task, so call inside a tokio runtime.
pub fn apply(router: Router<AppState>, limit: RateLimit) -> Router<AppState> {
    let key = ClientIp {
        trust_forwarded: limit.trust_forwarded,
    };
    layer(router, limit, key)
}

/// Wrap authenticated routes in the per-session limit, inside a generous per-IP ceiling: a
/// client rotating made-up tokens gets a fresh session bucket each time, never a fresh IP one.
/// Spawns cleanup tasks.
pub fn apply_private(
    router: Router<AppState>,
    session: RateLimit,
    per_ip: RateLimit,
) -> Router<AppState> {
    let any_ip = |l: RateLimit| {
        AnyIp(ClientIp {
            trust_forwarded: l.trust_forwarded,
        })
    };
    let router = layer(router, session, Session(any_ip(session)));
    layer(router, per_ip, any_ip(per_ip))
}

/// Wrap one route group in its own per-session (bearer or app key) limit; no key: client IP.
pub fn apply_session(router: Router<AppState>, limit: RateLimit) -> Router<AppState> {
    let key = Session(AnyIp(ClientIp {
        trust_forwarded: limit.trust_forwarded,
    }));
    layer(router, limit, key)
}

fn layer<K>(router: Router<AppState>, limit: RateLimit, key: K) -> Router<AppState>
where
    K: KeyExtractor + Send + Sync + 'static,
    K::Key: Send + Sync + 'static,
{
    // Clamped: governor rejects zero burst/period, so finish() below can't return None.
    let config = GovernorConfigBuilder::default()
        .period(limit.replenish.max(Duration::from_millis(1)))
        .burst_size(limit.burst.max(1))
        .key_extractor(key)
        .finish();
    let Some(config) = config else {
        unreachable!("burst and period clamped to >= 1");
    };

    let limiter = config.limiter().clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(CLEANUP_EVERY);
        loop {
            tick.tick().await;
            limiter.retain_recent();
        }
    });

    router.layer(GovernorLayer::new(config).error_handler(too_many))
}

fn too_many(err: GovernorError) -> Response<Body> {
    match err {
        GovernorError::TooManyRequests { wait_time, .. } => {
            let mut res = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "message": TOO_MANY })),
            )
                .into_response();
            res.headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(wait_time.max(1)));
            res
        }
        // No peer address: server not started with connect info. Config bug, not client fault.
        other => AppError::Internal(format!("rate limiter: {other}")).into_response(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn public_shop_bucket_is_bound_to_connection_not_supplied_bearer() {
        let session = super::Session(super::AnyIp(super::ClientIp {
            trust_forwarded: false,
        }));
        let id = uuid::Uuid::now_v7();
        let first = axum::http::Request::builder()
            .uri(format!("/v1/storefront/shops/{id}/products"))
            .header("authorization", format!("Bearer {}", "a".repeat(64)))
            .body(())
            .unwrap();
        let other = axum::http::Request::builder()
            .uri(format!("/v1/storefront/shops/{id}/orders"))
            .header("authorization", format!("Bearer {}", "b".repeat(64)))
            .body(())
            .unwrap();
        use tower_governor::key_extractor::KeyExtractor;
        assert_eq!(
            session.extract(&first).unwrap(),
            super::SessionKey::Shop(id)
        );
        assert_eq!(
            session.extract(&first).unwrap(),
            session.extract(&other).unwrap()
        );
    }

    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn ipv6_addresses_in_one_slash_64_share_a_bucket() {
        assert_eq!(bucket(ip("2001:db8:1:2:aaaa::1")), ip("2001:db8:1:2::"));
        assert_eq!(
            bucket(ip("2001:db8:1:2:ffff:ffff:ffff:ffff")),
            ip("2001:db8:1:2::")
        );
        assert_ne!(bucket(ip("2001:db8:1:3::1")), bucket(ip("2001:db8:1:2::1")));
    }

    #[test]
    fn ipv4_and_ipv4_mapped_stay_per_address() {
        assert_eq!(bucket(ip("203.0.113.7")), ip("203.0.113.7"));
        assert_eq!(bucket(ip("::ffff:203.0.113.7")), ip("203.0.113.7"));
        assert_ne!(bucket(ip("203.0.113.7")), bucket(ip("203.0.113.8")));
    }
}
