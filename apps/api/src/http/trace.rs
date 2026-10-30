//! Request tracing span with secrets stripped: verify tokens and invite codes travel in URL
//! paths, so the span shows them as `:token` instead of the default full URI.

use axum::{body::Body, http::Request};
use tracing::Span;

use crate::modules::auth::token;

/// `make_span_with` for TraceLayer: same debug-level span name as the default, path redacted,
/// query string left out.
pub fn span(req: &Request<Body>) -> Span {
    tracing::debug_span!(
        "request",
        method = %req.method(),
        path = %redact(req.uri().path()),
        version = ?req.version(),
    )
}

/// Replace every segment shaped like one of our secrets (64 hex chars) with `:token`.
pub fn redact(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if token::looks_valid(seg) {
                ":token"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_in_path_redacted() {
        let code = "a".repeat(64);

        assert_eq!(
            redact(&format!("/v1/verify-email/{code}")),
            "/v1/verify-email/:token"
        );
        assert_eq!(
            redact(&format!("/v1/invites/get-invite-status/{code}")),
            "/v1/invites/get-invite-status/:token"
        );
    }

    #[test]
    fn ordinary_paths_untouched() {
        for path in [
            "/v1/login",
            "/v1/workspace/01a159ba-3ddc-7178-966d-36b9c5d12d9c/invites",
            "/healthz",
            "/",
        ] {
            assert_eq!(redact(path), path);
        }
    }
}
