//! Per-route body limits survive the move to documented routers: the big-body routes take
//! more than the app-wide 64 KiB, each up to its own cap, and other routes stay at 64 KiB.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{app, send, signup};
use serde_json::json;
use sqlx::PgPool;

const KIB: usize = 1024;
const MISSING: &str = "00000000-0000-0000-0000-000000000000";

#[sqlx::test]
async fn big_body_routes_keep_their_own_limits(pool: PgPool) {
    let app = app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let at = |rest: &str| format!("/v1/workspace/{ws}{rest}");
    let post = |uri: String, bytes: usize| {
        let (app, token) = (app.clone(), token.clone());
        async move {
            let body = json!({ "pad": "x".repeat(bytes) });
            send(&app, Method::POST, &uri, Some(body), Some(&token))
                .await
                .0
        }
    };
    // (route, its cap): a body past 64 KiB passes the limit, one past the cap is a 413.
    for (uri, cap) in [
        (at(&format!("/invoices/{MISSING}/send")), KIB * KIB),
        (at(&format!("/invoices/{MISSING}/resend")), KIB * KIB),
        (at("/invoices"), 2 * KIB * KIB),
        (at("/calculate-items"), 2 * KIB * KIB),
        (at("/send-message"), KIB * KIB),
        (at("/settings/pdf-templates"), 512 * KIB),
        (at("/files"), 10 * KIB * KIB + 64 * KIB),
    ] {
        let status = post(uri.clone(), 100 * KIB).await;
        assert_ne!(
            status,
            StatusCode::PAYLOAD_TOO_LARGE,
            "{uri}: 100 KiB must pass"
        );
        let status = post(uri.clone(), cap + KIB).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{uri}: past its cap");
    }
    for uri in [
        at("/deals"),
        at("/settings"),
        at("/email-templates"),
        at("/currencies"),
    ] {
        let status = post(uri.clone(), 70 * KIB).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{uri}: keeps 64 KiB");
    }
}
