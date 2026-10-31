//! Fan-out (pending events -> one delivery per subscribed endpoint) and signed delivery, both
//! on the job rail. Signatures follow Standard Webhooks: `webhook-id`, `webhook-timestamp`,
//! `webhook-signature: v1,<base64 HMAC-SHA256(key, "{id}.{ts}.{body}")>`.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AppError,
    jobs::{
        kinds::{Ctx, WEBHOOK_DELIVER},
        repo::{self as jobs, Claimed, NewJob},
        runner::Step,
    },
    modules::webhooks::service::{SECRET_PREFIX, seal_path},
};

/// Pending events one fan-out chunk takes.
const FANOUT_CHUNK: i64 = 500;
/// Tries per delivery. The rail backs off 10 s doubling to 1 h, so about 3.5 h in all.
pub const DELIVER_ATTEMPTS: i32 = 12;
/// Longest one POST may take.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest error text kept on a delivery (column CHECK).
const ERROR_CHARS: usize = 1000;

fn workspace(job: &Claimed) -> Result<Uuid, AppError> {
    job.workspace_id
        .ok_or_else(|| AppError::BadRequest("webhook job without a workspace".into()))
}

/// One chunk: lock the workspace's oldest pending events, write a delivery per subscribed
/// active endpoint (never twice: unique event x endpoint), queue a delivery job for each and
/// mark the events dispatched, in one transaction.
pub async fn fanout(db: &PgPool, job: &Claimed) -> Result<Step, AppError> {
    let ws = workspace(job)?;
    let mut tx = db.begin().await?;
    let out = sqlx::query!(
        r#"WITH ev AS (
               SELECT id, type FROM webhook_events
               WHERE workspace_id = $1 AND dispatched_at IS NULL
               ORDER BY id
               LIMIT $2
               FOR UPDATE SKIP LOCKED
           ), ins AS (
               INSERT INTO webhook_deliveries (workspace_id, event_id, endpoint_id)
               SELECT $1, ev.id, ep.id
               FROM ev
               JOIN webhook_endpoints ep
                 ON ep.workspace_id = $1 AND ep.active AND ev.type = ANY (ep.events)
               ORDER BY ev.id, ep.id
               ON CONFLICT (event_id, endpoint_id) DO NOTHING
               RETURNING id
           ), done AS (
               UPDATE webhook_events SET dispatched_at = now()
               WHERE workspace_id = $1 AND id IN (SELECT id FROM ev)
               RETURNING 1
           )
           SELECT (SELECT count(*) FROM done) AS "events!",
                  coalesce((SELECT array_agg(id ORDER BY id) FROM ins), '{}') AS "deliveries!""#,
        ws,
        FANOUT_CHUNK,
    )
    .fetch_one(&mut *tx)
    .await?;
    for id in out.deliveries {
        let job = NewJob {
            workspace_id: Some(ws),
            payload: json!({ "delivery_id": id }),
            max_attempts: DELIVER_ATTEMPTS,
            dedupe_key: Some(format!("webhook_deliver:{id}")),
            ..NewJob::new(WEBHOOK_DELIVER)
        };
        jobs::enqueue(&mut tx, &job).await?;
    }
    tx.commit().await?;
    Ok(if out.events == FANOUT_CHUNK {
        Step::More
    } else {
        Step::Done
    })
}

/// HMAC-SHA256 of `msg` under `key`.
fn hmac_sha256(key: &[u8], msg: &[u8]) -> Result<[u8; 32], AppError> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key)
        .map_err(|_| AppError::Internal("hmac key".into()))?;
    mac.update(msg);
    Ok(mac.finalize().into_bytes().into())
}

/// `v1,<base64>` signature of one message (Standard Webhooks).
pub fn sign(secret: &str, id: &str, timestamp: i64, body: &str) -> Result<String, AppError> {
    let key = secret
        .strip_prefix(SECRET_PREFIX)
        .and_then(|k| STANDARD.decode(k).ok())
        .ok_or_else(|| AppError::Internal("webhook secret is not whsec_ base64".into()))?;
    let mac = hmac_sha256(&key, format!("{id}.{timestamp}.{body}").as_bytes())?;
    Ok(format!("v1,{}", STANDARD.encode(mac)))
}

/// What one attempt ended as.
enum Outcome {
    Delivered(u16),
    /// Retry later (non-2xx answer or a network error).
    Retry(Option<u16>, String),
    /// Never retried (target refused by the address guard).
    Refused(String),
}

/// POST one delivery. Done when delivered, refused, or nothing left to send (endpoint gone,
/// inactive, already finished); a retryable error otherwise, until the last attempt marks the
/// delivery `failed` (the dead letter).
pub async fn deliver(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let ws = workspace(job)?;
    let id = job
        .payload
        .get("delivery_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest("delivery job without delivery_id".into()))?;
    let Some(d) = sqlx::query!(
        r#"SELECT d.status, d.endpoint_id, e.id AS event_id, e.type AS kind, e.payload,
                  e.created_at, ep.url, ep.active, ep.secret_sealed
           FROM webhook_deliveries d
           JOIN webhook_events e ON e.workspace_id = d.workspace_id AND e.id = d.event_id
           JOIN webhook_endpoints ep ON ep.workspace_id = d.workspace_id AND ep.id = d.endpoint_id
           WHERE d.workspace_id = $1 AND d.id = $2"#,
        ws,
        id,
    )
    .fetch_optional(&ctx.db)
    .await?
    else {
        return Ok(Step::Done);
    };
    if d.status != "pending" {
        return Ok(Step::Done);
    }
    if !d.active {
        record(&ctx.db, ws, id, "failed", None, Some("endpoint inactive")).await?;
        return Ok(Step::Done);
    }
    let key = ctx
        .credentials
        .as_ref()
        .ok_or_else(|| AppError::Internal("CREDENTIALS_KEY not set: can't sign webhooks".into()))?;
    let secret = key.open(ws, &seal_path(d.endpoint_id), &d.secret_sealed)?;
    let event_id = d.event_id.to_string();
    let body = json!({
        "id": event_id, "type": d.kind, "created_at": d.created_at, "data": d.payload,
    })
    .to_string();
    let timestamp = chrono::Utc::now().timestamp();
    let signature = sign(secret.expose(), &event_id, timestamp, &body)?;
    let req = ctx
        .http
        .request(reqwest::Method::POST, &d.url)
        .timeout(SEND_TIMEOUT)
        .header("content-type", "application/json")
        .header("webhook-id", &event_id)
        .header("webhook-timestamp", timestamp.to_string())
        .header("webhook-signature", signature)
        .body(body);
    let outcome = match ctx.http.send(req).await {
        Ok(res) if res.status().is_success() => Outcome::Delivered(res.status().as_u16()),
        Ok(res) => Outcome::Retry(
            Some(res.status().as_u16()),
            format!("endpoint answered {}", res.status()),
        ),
        // `Outbound::send` refuses an inward IP literal before connecting: no retry helps.
        Err(e) if e.contains("is not a public address") => Outcome::Refused(e),
        Err(e) => Outcome::Retry(None, e),
    };
    match outcome {
        Outcome::Delivered(status) => {
            record(&ctx.db, ws, id, "delivered", Some(status), None).await?;
            Ok(Step::Done)
        }
        Outcome::Refused(e) => {
            record(&ctx.db, ws, id, "failed", None, Some(&e)).await?;
            Ok(Step::Done)
        }
        Outcome::Retry(status, e) => {
            let last = job.attempts >= job.max_attempts;
            let state = if last { "failed" } else { "pending" };
            record(&ctx.db, ws, id, state, status, Some(&e)).await?;
            Err(AppError::Internal(format!("webhook delivery: {e}")))
        }
    }
}

/// Count one attempt and its result.
async fn record(
    db: &PgPool,
    workspace_id: Uuid,
    id: Uuid,
    status: &str,
    http_status: Option<u16>,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    let error: Option<String> =
        error.map(|e| e.chars().filter(|c| *c != '\0').take(ERROR_CHARS).collect());
    sqlx::query!(
        "UPDATE webhook_deliveries SET
             attempts = attempts + 1, status = $3, last_status = $4, last_error = $5,
             delivered_at = CASE WHEN $3 = 'delivered' THEN now() END, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        status,
        http_status.map(|s| s as i16),
        error,
    )
    .execute(db)
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicU16, Ordering},
    };

    use axum::{
        Router,
        body::Bytes,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use serde_json::Value;

    use super::*;
    use crate::{CredentialKey, outbound::Outbound};

    const HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    const SECRET: &str = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";

    type Seen = Arc<Mutex<Vec<(String, HeaderMap, String)>>>;

    /// Local receiver: `/hook` answers `status`, `/redirect` sends a 302 to `/hook`.
    async fn receiver(status: u16) -> (String, Seen, Arc<AtomicU16>) {
        let seen: Seen = Arc::default();
        let code = Arc::new(AtomicU16::new(status));
        let (s, c) = (seen.clone(), code.clone());
        let hook = move |headers: HeaderMap, body: Bytes| {
            let (s, c) = (s.clone(), c.clone());
            async move {
                let body = String::from_utf8(body.to_vec()).unwrap();
                s.lock().unwrap().push(("/hook".into(), headers, body));
                StatusCode::from_u16(c.load(Ordering::SeqCst)).unwrap()
            }
        };
        let app = Router::new().route("/hook", post(hook)).route(
            "/redirect",
            post(|| async { (StatusCode::FOUND, [("location", "/hook")]) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), seen, code)
    }

    fn key() -> Arc<CredentialKey> {
        Arc::new(CredentialKey::from_hex(HEX).unwrap())
    }

    fn ctx(db: &PgPool, trusted: &[&str]) -> Ctx {
        let trusted: Vec<String> = trusted.iter().map(|s| (*s).to_owned()).collect();
        Ctx {
            db: db.clone(),
            files: None,
            http: Outbound::new(&trusted).unwrap(),
            credentials: Some(key()),
        }
    }

    fn claimed(kind: &str, ws: Uuid, payload: Value, attempts: i32) -> Claimed {
        Claimed {
            id: Uuid::now_v7(),
            kind: kind.into(),
            workspace_id: Some(ws),
            payload,
            attempts,
            max_attempts: DELIVER_ATTEMPTS,
            created_by: None,
            token: Uuid::now_v7(),
        }
    }

    async fn workspace(db: &PgPool, email: &str) -> Uuid {
        sqlx::query_scalar(
            "WITH u AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ($1::text, 'O', 'W', 'h') RETURNING id)
             INSERT INTO workspaces (name, created_by_user_id) SELECT 'ws-' || split_part($1::text, '@', 1), id FROM u
             RETURNING id",
        )
        .bind(email)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn endpoint(db: &PgPool, ws: Uuid, url: &str, events: &str, active: bool) -> Uuid {
        let id = Uuid::now_v7();
        let sealed = key().seal(ws, &seal_path(id), SECRET).unwrap();
        sqlx::query(
            "INSERT INTO webhook_endpoints
                 (id, workspace_id, url, events, active, secret_sealed, secret_key_id)
             VALUES ($1, $2, $3, $4::text[], $5, $6, 'k')",
        )
        .bind(id)
        .bind(ws)
        .bind(url)
        .bind(events)
        .bind(active)
        .bind(sealed)
        .execute(db)
        .await
        .unwrap();
        id
    }

    async fn event(db: &PgPool, ws: Uuid, kind: &str) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO webhook_events (workspace_id, type, payload)
             VALUES ($1, $2, '{\"product_id\": \"p\"}') RETURNING id",
        )
        .bind(ws)
        .bind(kind)
        .fetch_one(db)
        .await
        .unwrap()
    }

    /// Fan the workspace's events out; the queued delivery ids.
    async fn fan(db: &PgPool, ws: Uuid) -> Vec<Uuid> {
        assert_eq!(
            fanout(db, &claimed("webhook_fanout", ws, json!({}), 1))
                .await
                .unwrap(),
            Step::Done
        );
        sqlx::query_scalar(
            "SELECT (payload->>'delivery_id')::uuid FROM jobs
             WHERE kind = 'webhook_deliver' ORDER BY id",
        )
        .fetch_all(db)
        .await
        .unwrap()
    }

    async fn delivery(db: &PgPool, id: Uuid) -> (String, i32, Option<i16>, Option<String>) {
        sqlx::query_as(
            "SELECT status, attempts, last_status, last_error FROM webhook_deliveries WHERE id = $1",
        )
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap()
    }

    fn job(ws: Uuid, delivery: Uuid, attempts: i32) -> Claimed {
        claimed(
            "webhook_deliver",
            ws,
            json!({ "delivery_id": delivery }),
            attempts,
        )
    }

    #[test]
    fn hmac_matches_rfc_4231_test_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?").unwrap();
        assert_eq!(
            hex::encode(mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn signature_matches_the_standard_webhooks_fixture() {
        let sig = sign(
            SECRET,
            "msg_p5jXN8AQM9LWM0D4loKWxJek",
            1614265330,
            r#"{"test": 2432232314}"#,
        )
        .unwrap();
        assert_eq!(sig, "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=");
        assert!(sign("nope", "i", 1, "b").is_err(), "secret without prefix");
    }

    #[sqlx::test]
    async fn a_delivery_arrives_signed_and_verifies(db: PgPool) {
        let (base, seen, _) = receiver(200).await;
        let ws = workspace(&db, "o@x.io").await;
        endpoint(&db, ws, &format!("{base}/hook"), "{stock.changed}", true).await;
        let ev = event(&db, ws, "stock.changed").await;
        let ids = fan(&db, ws).await;

        let step = deliver(&ctx(&db, &["127.0.0.1"]), &job(ws, ids[0], 1))
            .await
            .unwrap();

        assert_eq!(step, Step::Done);
        let seen = seen.lock().unwrap();
        let (_, headers, body) = &seen[0];
        let h = |n: &str| headers.get(n).unwrap().to_str().unwrap().to_owned();
        assert_eq!(h("webhook-id"), ev.to_string());
        let sig = h("webhook-signature");
        let mac = STANDARD.decode(sig.strip_prefix("v1,").unwrap()).unwrap();
        let mut check =
            <Hmac<Sha256> as KeyInit>::new_from_slice(&STANDARD.decode(&SECRET[6..]).unwrap())
                .unwrap();
        check.update(format!("{}.{}.{body}", h("webhook-id"), h("webhook-timestamp")).as_bytes());
        check
            .verify_slice(&mac)
            .expect("signature verifies (constant time)");
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["type"], "stock.changed");
        assert_eq!(body["data"]["product_id"], "p");
        drop(seen);
        assert_eq!(
            delivery(&db, ids[0]).await,
            ("delivered".into(), 1, Some(200), None)
        );
    }

    #[sqlx::test]
    async fn fanout_writes_one_delivery_per_subscribed_active_endpoint_once(db: PgPool) {
        let ws = workspace(&db, "o@x.io").await;
        let a = endpoint(&db, ws, "https://a.io/h", "{stock.changed}", true).await;
        let b = endpoint(
            &db,
            ws,
            "https://b.io/h",
            "{stock.changed,product.changed}",
            true,
        )
        .await;
        endpoint(&db, ws, "https://c.io/h", "{product.changed}", true).await;
        endpoint(&db, ws, "https://d.io/h", "{stock.changed}", false).await;
        event(&db, ws, "stock.changed").await;

        let first = fan(&db, ws).await;
        let again = fan(&db, ws).await;

        assert_eq!(first.len(), 2);
        assert_eq!(again, first, "second fan-out adds nothing");
        let endpoints: Vec<Uuid> =
            sqlx::query_scalar("SELECT endpoint_id FROM webhook_deliveries ORDER BY endpoint_id")
                .fetch_all(&db)
                .await
                .unwrap();
        let mut want = vec![a, b];
        want.sort();
        assert_eq!(endpoints, want);
        let pending: i64 =
            sqlx::query_scalar("SELECT count(*) FROM webhook_events WHERE dispatched_at IS NULL")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(pending, 0);
    }

    #[sqlx::test]
    async fn a_500_is_retried_and_the_last_attempt_is_the_dead_letter(db: PgPool) {
        let (base, seen, code) = receiver(500).await;
        let ws = workspace(&db, "o@x.io").await;
        endpoint(&db, ws, &format!("{base}/hook"), "{stock.changed}", true).await;
        event(&db, ws, "stock.changed").await;
        let id = fan(&db, ws).await[0];
        let ctx = ctx(&db, &["127.0.0.1"]);

        let err = deliver(&ctx, &job(ws, id, 1)).await.unwrap_err();
        assert!(!err.is_permanent(), "{err}");
        let (status, attempts, last, _) = delivery(&db, id).await;
        assert_eq!((status.as_str(), attempts, last), ("pending", 1, Some(500)));

        deliver(&ctx, &job(ws, id, DELIVER_ATTEMPTS))
            .await
            .unwrap_err();
        assert_eq!(delivery(&db, id).await.0, "failed");
        code.store(200, Ordering::SeqCst);
        let step = deliver(&ctx, &job(ws, id, 1)).await.unwrap();
        assert_eq!(step, Step::Done, "a dead letter is not sent again");
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[sqlx::test]
    async fn redirects_are_not_followed(db: PgPool) {
        let (base, seen, _) = receiver(200).await;
        let ws = workspace(&db, "o@x.io").await;
        endpoint(
            &db,
            ws,
            &format!("{base}/redirect"),
            "{stock.changed}",
            true,
        )
        .await;
        event(&db, ws, "stock.changed").await;
        let id = fan(&db, ws).await[0];

        deliver(&ctx(&db, &["127.0.0.1"]), &job(ws, id, 1))
            .await
            .unwrap_err();

        assert_eq!(delivery(&db, id).await.2, Some(302));
        assert!(
            seen.lock().unwrap().is_empty(),
            "redirect target was called"
        );
    }

    #[sqlx::test]
    async fn inward_targets_are_never_reached(db: PgPool) {
        let (base, seen, _) = receiver(200).await;
        let port = base.rsplit(':').next().unwrap().to_owned();
        let ws = workspace(&db, "o@x.io").await;
        endpoint(&db, ws, &format!("{base}/hook"), "{stock.changed}", true).await;
        endpoint(
            &db,
            ws,
            &format!("http://localhost:{port}/hook"),
            "{stock.changed}",
            true,
        )
        .await;
        event(&db, ws, "stock.changed").await;
        let ids = fan(&db, ws).await;
        let guarded = ctx(&db, &[]);

        // IP literal: refused before connecting, never retried.
        assert_eq!(
            deliver(&guarded, &job(ws, ids[0], 1)).await.unwrap(),
            Step::Done
        );
        // A name resolving inward: the guard drops every address; retried (DNS may change).
        deliver(&guarded, &job(ws, ids[1], 1)).await.unwrap_err();

        assert!(seen.lock().unwrap().is_empty(), "inward target was called");
        let (status, _, _, error) = delivery(&db, ids[0]).await;
        assert_eq!(status, "failed");
        assert!(error.unwrap().contains("not a public address"));
        assert_eq!(delivery(&db, ids[1]).await.0, "pending");
    }

    #[sqlx::test]
    async fn an_inactive_endpoint_gets_nothing(db: PgPool) {
        let (base, seen, _) = receiver(200).await;
        let ws = workspace(&db, "o@x.io").await;
        let ep = endpoint(&db, ws, &format!("{base}/hook"), "{stock.changed}", true).await;
        event(&db, ws, "stock.changed").await;
        let id = fan(&db, ws).await[0];
        sqlx::query("UPDATE webhook_endpoints SET active = false WHERE id = $1")
            .bind(ep)
            .execute(&db)
            .await
            .unwrap();

        deliver(&ctx(&db, &["127.0.0.1"]), &job(ws, id, 1))
            .await
            .unwrap();

        assert!(seen.lock().unwrap().is_empty());
        assert_eq!(delivery(&db, id).await.0, "failed");
    }

    #[sqlx::test]
    async fn a_job_of_another_workspace_sends_nothing(db: PgPool) {
        let (base, seen, _) = receiver(200).await;
        let ws = workspace(&db, "o@x.io").await;
        let other = workspace(&db, "e@x.io").await;
        endpoint(&db, ws, &format!("{base}/hook"), "{stock.changed}", true).await;
        event(&db, ws, "stock.changed").await;
        let id = fan(&db, ws).await[0];

        let step = deliver(&ctx(&db, &["127.0.0.1"]), &job(other, id, 1))
            .await
            .unwrap();

        assert_eq!(step, Step::Done);
        assert!(seen.lock().unwrap().is_empty());
        assert_eq!(delivery(&db, id).await.0, "pending");
    }
}
