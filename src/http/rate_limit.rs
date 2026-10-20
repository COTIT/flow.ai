//! Per-IP rate limit for public auth routes (signup, login, register, invite status).

use std::{
    net::{IpAddr, Ipv6Addr},
    time::Duration,
};

use axum::{
    Json, Router,
    body::Body,
    http::{HeaderValue, Request, Response, StatusCode, header::RETRY_AFTER},
    response::IntoResponse,
};
use serde_json::json;
use tower_governor::{
    GovernorError, GovernorLayer,
    governor::GovernorConfigBuilder,
    key_extractor::{KeyExtractor, PeerIpKeyExtractor, SmartIpKeyExtractor},
};

use crate::{
    AppState,
    error::{AppError, TOO_MANY},
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

/// Wrap `router` in the limit. Spawns a cleanup task, so call inside a tokio runtime.
pub fn apply(router: Router<AppState>, limit: RateLimit) -> Router<AppState> {
    // Clamped: governor rejects zero burst/period, so finish() below can't return None.
    let config = GovernorConfigBuilder::default()
        .period(limit.replenish.max(Duration::from_millis(1)))
        .burst_size(limit.burst.max(1))
        .key_extractor(ClientIp {
            trust_forwarded: limit.trust_forwarded,
        })
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
