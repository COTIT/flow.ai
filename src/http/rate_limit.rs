//! Per-IP rate limit for public auth routes (signup, login, register, invite status).

use std::{net::IpAddr, time::Duration};

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
        if self.trust_forwarded {
            SmartIpKeyExtractor.extract(req)
        } else {
            PeerIpKeyExtractor.extract(req)
        }
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
