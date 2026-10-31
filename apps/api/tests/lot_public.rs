//! Public lot page: `GET /v1/public/lots/{public_code}` (no auth, IP-limited) and the same
//! reply at `GET /v1/storefront/lots/{public_code}` for an app key of the lot's workspace.

mod common;

use std::net::SocketAddr;

use axum::{
    Router,
    body::{Body, Bytes},
    extract::ConnectInfo,
    http::{HeaderMap, Method, Request, StatusCode},
};
use common::{Api, PEER, send};
use genesy_360_api::{RateLimit, build_app, mail::Mailer};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const PNG_B64: &str = "iVBORw0KGgo=";
const PDF_B64: &str = "JVBERi0xLjQKJcOkw7zDtsOfCg==";

struct World {
    api: Api,
    lot: String,
    code: String,
    key: String,
}

async fn upload(api: &Api, name: &str, b64: &str) -> String {
    api.with_base("/files")
        .create(
            json!({ "upload": b64, "document_name": name, "public": 1 }),
            "file_id",
        )
        .await
}

async fn key_for(api: &Api) -> String {
    let body = json!({ "name": "Web shop", "kind": "shop", "scopes": ["catalog:read"] });
    let (status, res) = api
        .with_base("/app-keys")
        .call(Method::POST, "", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["key"].as_str().unwrap().to_owned()
}

/// Ada's `acme`: a honey product, lot ILU-1 with a published passport (banner, story image,
/// certificate PDF), and a `catalog:read` key.
async fn world(pool: PgPool) -> World {
    let api = Api::new(pool, "/lots").await;
    let product = api
        .with_base("/products")
        .create(json!({ "name": "Honey 500 g", "price": 10 }), "product_id")
        .await;
    let png = upload(&api, "hive.png", PNG_B64).await;
    let pdf = upload(&api, "organic.pdf", PDF_B64).await;
    let (_, res) = api
        .call(
            Method::POST,
            "",
            Some(
                json!({ "product_id": product, "code": "ILU-1", "produced_on": "2026-07-01",
                         "best_before": "2028-07-01", "origin": "Ilundo, Tanzania" }),
            ),
            &api.owner,
        )
        .await;
    let lot = res["data"]["id"].as_str().unwrap().to_owned();
    let code = res["data"]["public_code"].as_str().unwrap().to_owned();
    let passport = json!({
        "title": "Ilundo forest honey",
        "intro": "Raw honey from the miombo woodland.",
        "banner_file_id": png,
        "origin": { "country": "TZ", "region": "Iringa", "place": "Ilundo" },
        "map": { "lat": -8.912345, "lng": 35.456789, "label": "Ilundo village" },
        "materials": [{ "name": "Honey", "share_bp": 10000 }],
        "certificates": [{ "name": "Organic", "file_id": pdf }],
        "story": [
            { "kind": "text", "title": "The forest", "body": "Bees forage on miombo blossom." },
            { "kind": "image", "file_id": png, "caption": "The hives" }
        ]
    });
    let (status, res) = api
        .call(
            Method::PUT,
            &format!("/{lot}/passport"),
            Some(passport),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = api
        .call(
            Method::POST,
            &format!("/{lot}/passport/publish"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let key = key_for(&api).await;
    World {
        api,
        lot,
        code,
        key,
    }
}

async fn public(w: &World, code: &str) -> (StatusCode, Value) {
    send(
        &w.api.app,
        Method::GET,
        &format!("/v1/public/lots/{code}"),
        None,
        None,
    )
    .await
}

async fn storefront(w: &World, code: &str, key: &str) -> (StatusCode, Value) {
    send(
        &w.api.app,
        Method::GET,
        &format!("/v1/storefront/lots/{code}"),
        None,
        Some(key),
    )
    .await
}

/// GET with no auth: status, headers and raw body.
async fn raw_get(app: &Router, uri: &str) -> (StatusCode, HeaderMap, Bytes) {
    let mut req = Request::get(uri).body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(PEER.parse::<SocketAddr>().unwrap()));
    let res = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    (
        parts.status,
        parts.headers,
        body.collect().await.unwrap().to_bytes(),
    )
}

fn keys(v: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    k.sort_unstable();
    k
}

/// The allow-list, both mounts: nothing internal (ids, workspace, draft, quantities).
fn assert_allow_listed(data: &Value) {
    assert_eq!(
        keys(data),
        [
            "best_before",
            "code",
            "expires_on",
            "farmers",
            "origin",
            "passport",
            "produced_on",
            "product",
            "public_code",
            "status"
        ]
    );
    assert_eq!(
        keys(&data["product"]),
        ["brand", "name", "short_description"]
    );
    assert_eq!(
        keys(&data["passport"]),
        [
            "banner",
            "certificates",
            "intro",
            "map",
            "materials",
            "origin",
            "published_at",
            "story",
            "title",
            "version"
        ]
    );
    assert_eq!(keys(&data["passport"]["banner"]), ["expires_at", "url"]);
    assert_eq!(
        keys(&data["passport"]["certificates"][0]),
        ["file", "issuer", "name", "number", "valid_until"]
    );
    assert_eq!(
        keys(&data["passport"]["story"][1]),
        ["caption", "image", "kind"]
    );
    assert_eq!(data["farmers"], json!([]));
    let text = data.to_string();
    for leak in [
        "workspace",
        "draft",
        "file_id",
        "created_by",
        "stock",
        "\"id\"",
    ] {
        assert!(!text.contains(leak), "{leak} leaks: {text}");
    }
}

#[sqlx::test]
async fn scanning_a_jar_shows_its_batch_farm_and_story(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = public(&w, &w.code).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_allow_listed(d);
    assert_eq!(d["code"], "ILU-1");
    assert_eq!(d["public_code"], w.code.as_str());
    assert_eq!(d["status"], "released");
    assert_eq!(d["produced_on"], "2026-07-01");
    assert_eq!(d["origin"], "Ilundo, Tanzania");
    assert_eq!(d["product"]["name"], "Honey 500 g");
    let p = &d["passport"];
    assert_eq!(p["version"], 1);
    assert_eq!(p["title"], "Ilundo forest honey");
    assert_eq!(p["origin"]["place"], "Ilundo");
    assert_eq!(
        p["map"],
        json!({ "lat": -8.91, "lng": 35.46, "label": "Ilundo village" })
    );
    assert_eq!(p["story"][0]["body"], "Bees forage on miombo blossom.");
    assert!(
        p["banner"]["url"].as_str().unwrap().contains("hive.png"),
        "{p}"
    );
    assert!(p["banner"]["expires_at"].is_string());
    assert!(
        p["certificates"][0]["file"]["url"]
            .as_str()
            .unwrap()
            .contains("organic.pdf")
    );
    assert!(p["story"][1]["image"]["url"].is_string());
}

#[sqlx::test]
async fn storefront_mount_gives_the_same_reply_for_its_own_workspace_only(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) = storefront(&w, &w.code, &w.key).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_allow_listed(&res["data"]);
    let (_, public) = public(&w, &w.code).await;
    assert_eq!(
        res["data"]["passport"]["title"],
        public["data"]["passport"]["title"]
    );

    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let eve_key = key_for(&eve).await;
    let (status, body) = storefront(&w, &w.code, &eve_key).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "foreign key: {body}");
    let (status, _) = storefront(&w, &w.code, &w.api.owner).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "user token is not an app key"
    );
    let (status, _) = storefront(&w, &w.code, "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn unknown_unpublished_and_foreign_lots_get_the_same_404(pool: PgPool) {
    let w = world(pool).await;
    let (_, res) = w
        .api
        .call(
            Method::POST,
            "",
            Some(json!({ "product_id": w.api.get(&w.lot).await["product_id"], "code": "ILU-2" })),
            &w.api.owner,
        )
        .await;
    let draft_only = res["data"]["public_code"].as_str().unwrap().to_owned();
    let (_, res) = w
        .api
        .call(
            Method::POST,
            "",
            Some(json!({ "product_id": w.api.get(&w.lot).await["product_id"], "code": "ILU-3" })),
            &w.api.owner,
        )
        .await;
    let hidden = res["data"]["id"].as_str().unwrap().to_owned();
    let hidden_code = res["data"]["public_code"].as_str().unwrap().to_owned();
    w.api
        .call(
            Method::PUT,
            &format!("/{hidden}/passport"),
            Some(json!({ "title": "t" })),
            &w.api.owner,
        )
        .await;
    w.api
        .call(
            Method::POST,
            &format!("/{hidden}/passport/publish"),
            None,
            &w.api.owner,
        )
        .await;
    w.api
        .call(
            Method::DELETE,
            &format!("/{hidden}/passport/publish"),
            None,
            &w.api.owner,
        )
        .await;

    let names = |h: &HeaderMap| {
        let mut n: Vec<String> = h.keys().map(|k| k.to_string()).collect();
        n.sort();
        n
    };
    let (status, headers, body) = raw_get(&w.api.app, "/v1/public/lots/0000000000").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for code in [
        draft_only,
        hidden_code,
        "not%20a%20code".to_owned(),
        "x".repeat(300),
    ] {
        let (s, h, b) = raw_get(&w.api.app, &format!("/v1/public/lots/{code}")).await;
        assert_eq!((s, &b), (status, &body), "{code}");
        assert_eq!(names(&h), names(&headers), "{code}");
    }
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let eve_key = key_for(&eve).await;
    let (s, _, b) = {
        let mut req = Request::get(format!("/v1/storefront/lots/{}", w.code))
            .header("authorization", format!("Bearer {eve_key}"))
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(PEER.parse::<SocketAddr>().unwrap()));
        let res = w.api.app.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        (
            parts.status,
            parts.headers,
            body.collect().await.unwrap().to_bytes(),
        )
    };
    assert_eq!(
        (s, &b),
        (status, &body),
        "foreign key on the storefront mount"
    );
}

#[sqlx::test]
async fn recalled_lot_still_shows_with_its_status(pool: PgPool) {
    let w = world(pool).await;
    w.api
        .call(
            Method::PATCH,
            &format!("/{}", w.lot),
            Some(json!({ "status": "recalled" })),
            &w.api.owner,
        )
        .await;
    let (status, res) = public(&w, &w.code).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "recalled");
}

#[sqlx::test]
async fn draft_edits_stay_private_until_republished(pool: PgPool) {
    let w = world(pool).await;
    w.api
        .call(
            Method::PUT,
            &format!("/{}/passport", w.lot),
            Some(json!({ "title": "Secret draft" })),
            &w.api.owner,
        )
        .await;
    let (_, res) = public(&w, &w.code).await;
    assert_eq!(res["data"]["passport"]["title"], "Ilundo forest honey");
    assert!(!res.to_string().contains("Secret draft"));
}

#[sqlx::test]
async fn deleted_file_drops_its_link_only(pool: PgPool) {
    let w = world(pool).await;
    let files: Vec<(String,)> =
        sqlx::query_as("SELECT id::text FROM files WHERE content_type = 'application/pdf'")
            .fetch_all(&w.api.pool)
            .await
            .unwrap();
    w.api
        .with_base("/files")
        .call(
            Method::DELETE,
            &format!("/{}", files[0].0),
            None,
            &w.api.owner,
        )
        .await;
    let (status, res) = public(&w, &w.code).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"]["passport"]["certificates"][0]["file"],
        Value::Null
    );
    assert_eq!(
        res["data"]["passport"]["certificates"][0]["name"],
        "Organic"
    );
}

#[sqlx::test]
async fn public_route_sits_behind_the_ip_limit(pool: PgPool) {
    let mut state = common::state(pool, Mailer::capture().0);
    state.rate_limit = RateLimit {
        burst: 2,
        replenish: std::time::Duration::from_secs(60),
        trust_forwarded: false,
    };
    let app = build_app(state);
    for _ in 0..2 {
        let (status, _) = send(&app, Method::GET, "/v1/public/lots/0000000000", None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let (status, _) = send(&app, Method::GET, "/v1/public/lots/0000000000", None, None).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
