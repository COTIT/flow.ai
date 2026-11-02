//! /v1/workspace/{ws}/lots/{lot}/passport: draft, publish and unpublish a lot's passport.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with};
use serde_json::{Value, json};
use sqlx::PgPool;

const PNG_B64: &str = "iVBORw0KGgo="; // PNG signature
const PDF_B64: &str = "JVBERi0xLjQKJcOkw7zDtsOfCg==";

/// Ada's `acme` (lots API) and one lot of a stock-tracked product.
async fn world(pool: PgPool) -> (Api, String) {
    let api = Api::new(pool, "/lots").await;
    let lot = lot_in(&api).await;
    (api, lot)
}

async fn lot_in(api: &Api) -> String {
    let product = api
        .with_base("/products")
        .create(json!({ "name": "Honey 500 g", "price": 10 }), "product_id")
        .await;
    api.create(json!({ "product_id": product, "code": "ILU-1" }), "id")
        .await
}

async fn upload(api: &Api, name: &str, b64: &str) -> String {
    api.with_base("/files")
        .create(
            json!({ "upload": b64, "document_name": name, "public": 1 }),
            "file_id",
        )
        .await
}

fn passport() -> Value {
    json!({
        "title": "Ilundo forest honey",
        "intro": "Raw honey from the miombo woodland.",
        "origin": { "country": "tz", "region": "Iringa", "place": "Ilundo" },
        "map": { "lat": -8.912345, "lng": 35.456789, "label": "Ilundo village" },
        "materials": [{ "name": "Honey", "origin": "Ilundo", "share_bp": 10000 }],
        "certificates": [{ "name": "Organic", "issuer": "TanCert", "valid_until": "2027-12-31" }],
        "story": [
            { "kind": "text", "title": "The forest", "body": "Bees forage on miombo blossom." },
            { "kind": "facts", "items": [{ "label": "Harvest", "value": "July 2026" }] },
            { "kind": "quote", "text": "The bees know the season.", "by": "Beekeeper" }
        ]
    })
}

async fn put(api: &Api, lot: &str, body: Value) -> (StatusCode, Value) {
    api.call(
        Method::PUT,
        &format!("/{lot}/passport"),
        Some(body),
        &api.owner,
    )
    .await
}

async fn publish(api: &Api, lot: &str) -> (StatusCode, Value) {
    api.call(
        Method::POST,
        &format!("/{lot}/passport/publish"),
        None,
        &api.owner,
    )
    .await
}

async fn unpublish(api: &Api, lot: &str) -> (StatusCode, Value) {
    api.call(
        Method::DELETE,
        &format!("/{lot}/passport/publish"),
        None,
        &api.owner,
    )
    .await
}

async fn show(api: &Api, lot: &str) -> Value {
    let (status, res) = api
        .call(Method::GET, &format!("/{lot}/passport"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

async fn trail(api: &Api, lot: &str) -> Vec<Value> {
    let (_, res) = api
        .call(Method::GET, &format!("/{lot}/events"), None, &api.owner)
        .await;
    res["data"].as_array().unwrap().clone()
}

#[sqlx::test]
async fn draft_publish_and_republish(pool: PgPool) {
    let (api, lot) = world(pool).await;
    let fresh = show(&api, &lot).await;
    assert_eq!(fresh["published"], Value::Null);
    assert_eq!(fresh["version"], 0);
    assert_eq!(fresh["published_at"], Value::Null);

    let (status, res) = put(&api, &lot, passport()).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["draft"]["title"], "Ilundo forest honey");
    assert_eq!(res["data"]["draft"]["origin"]["country"], "TZ");
    assert_eq!(res["data"]["published"], Value::Null);

    let (status, res) = publish(&api, &lot).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["version"], 1);
    assert!(res["data"]["published_at"].is_string());
    assert_eq!(res["data"]["published"], res["data"]["draft"]);

    let mut edited = passport();
    edited["title"] = json!("Ilundo honey, July");
    put(&api, &lot, edited).await;
    let now = show(&api, &lot).await;
    assert_eq!(now["draft"]["title"], "Ilundo honey, July");
    assert_eq!(
        now["published"]["title"], "Ilundo forest honey",
        "public copy waits"
    );

    let (_, res) = publish(&api, &lot).await;
    assert_eq!(res["data"]["version"], 2);
    assert_eq!(res["data"]["published"]["title"], "Ilundo honey, July");
    let kinds: Vec<Value> = trail(&api, &lot)
        .await
        .iter()
        .map(|e| e["kind"].clone())
        .collect();
    assert_eq!(
        kinds,
        [
            json!("passport_published"),
            json!("passport_published"),
            json!("created")
        ]
    );
    assert_eq!(trail(&api, &lot).await[0]["meta"], json!({ "version": 2 }));
}

#[sqlx::test]
async fn map_point_is_rounded_to_two_decimals_on_save(pool: PgPool) {
    let (api, lot) = world(pool).await;
    let (_, res) = put(&api, &lot, passport()).await;
    assert_eq!(res["data"]["draft"]["map"]["lat"], json!(-8.91));

    let stored: (String, String) = sqlx::query_as(
        "SELECT passport_draft->'map'->>'lat', passport_draft->'map'->>'lng' FROM lots",
    )
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(stored, ("-8.91".to_owned(), "35.46".to_owned()));
}

#[sqlx::test]
async fn draft_rejects_unknown_keys_and_bad_values(pool: PgPool) {
    let (api, lot) = world(pool).await;
    for (patch, why) in [
        (
            json!({ "phone": "+255 700 000 000" }),
            "unknown top-level key",
        ),
        (json!({ "farmers": [] }), "no farmer data in the draft"),
        (
            json!({ "map": { "lat": -8.9, "lng": 35.4, "gps": "exact" } }),
            "unknown nested key",
        ),
        (
            json!({ "story": [{ "kind": "video", "url": "x" }] }),
            "unknown block",
        ),
        (json!({ "title": "x".repeat(121) }), "title too long"),
        (json!({ "origin": { "country": "XX" } }), "country not ISO"),
        (
            json!({ "map": { "lat": 91, "lng": 0 } }),
            "latitude out of range",
        ),
        (
            json!({ "materials": [{ "name": "Honey", "share_bp": 10001 }] }),
            "share over 100%",
        ),
        (
            json!({ "story": [{ "kind": "text", "body": " " }] }),
            "blank text",
        ),
        (
            json!({ "story": (0..31).map(|_| json!({ "kind": "quote", "text": "q" })).collect::<Vec<_>>() }),
            "too many blocks",
        ),
        (
            json!({ "certificates": [{ "name": "Organic", "valid_until": "soon" }] }),
            "bad date",
        ),
    ] {
        let (status, res) = put(&api, &lot, patch).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{why}: {res}");
    }
    assert_eq!(
        show(&api, &lot).await["draft"]["title"],
        Value::Null,
        "nothing saved"
    );
}

#[sqlx::test]
async fn publish_needs_a_title_and_unpublish_hides_it(pool: PgPool) {
    let (api, lot) = world(pool).await;
    let (status, res) = publish(&api, &lot).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["title"].is_array(), "{res}");

    put(&api, &lot, passport()).await;
    publish(&api, &lot).await;
    let (status, res) = unpublish(&api, &lot).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["published"], Value::Null);
    assert_eq!(res["data"]["published_at"], Value::Null);
    assert_eq!(res["data"]["version"], 1, "version stays");
    assert_eq!(
        res["data"]["draft"]["title"], "Ilundo forest honey",
        "draft stays"
    );
    let (status, _) = unpublish(&api, &lot).await;
    assert_eq!(status, StatusCode::OK, "idempotent");

    let kinds: Vec<Value> = trail(&api, &lot)
        .await
        .iter()
        .map(|e| e["kind"].clone())
        .collect();
    assert_eq!(
        kinds,
        [
            json!("passport_unpublished"),
            json!("passport_published"),
            json!("created")
        ]
    );
}

#[sqlx::test]
async fn files_must_be_stored_files_of_this_workspace_of_the_right_type(pool: PgPool) {
    let (api, lot) = world(pool).await;
    let png = upload(&api, "hive.png", PNG_B64).await;
    let pdf = upload(&api, "organic.pdf", PDF_B64).await;
    let eve = api.as_new_owner("eve@example.com", "evil").await;
    let foreign = upload(&eve, "eve.png", PNG_B64).await;

    let mut good = passport();
    good["banner_file_id"] = json!(png);
    good["story"] = json!([{ "kind": "image", "file_id": png, "caption": "The hives" }]);
    good["certificates"] = json!([{ "name": "Organic", "file_id": pdf }]);
    let (status, res) = put(&api, &lot, good).await;
    assert_eq!(status, StatusCode::OK, "{res}");

    for (body, field) in [
        (json!({ "banner_file_id": foreign }), "banner_file_id"),
        (json!({ "banner_file_id": pdf }), "banner_file_id"),
        (
            json!({ "story": [{ "kind": "image", "file_id": pdf }] }),
            "story.0.file_id",
        ),
        (
            json!({ "story": [{ "kind": "image", "file_id": foreign }] }),
            "story.0.file_id",
        ),
        (
            json!({ "certificates": [{ "name": "x", "file_id": "01900000-0000-7000-8000-000000000001" }] }),
            "certificates.0.file_id",
        ),
    ] {
        let (status, res) = put(&api, &lot, body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }

    let (status, _) = api
        .with_base("/files")
        .call(Method::DELETE, &format!("/{png}"), None, &api.owner)
        .await;
    assert!(status.is_success(), "{status}");
    let (status, res) = publish(&api, &lot).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "file gone since save: {res}"
    );
    assert!(res["errors"]["banner_file_id"].is_array(), "{res}");
}

#[sqlx::test]
async fn publish_reaches_the_outbox_and_unpublish_sends_nothing(pool: PgPool) {
    let (api, lot) = world(pool).await;
    let hooks = api.with_base("/webhook-endpoints");
    let body = json!({ "url": "https://shop.example.com/hooks", "events": ["passport.published"] });
    let (status, res) = hooks.call(Method::POST, "", Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let got = api.get(&lot).await;

    put(&api, &lot, passport()).await;
    publish(&api, &lot).await;
    unpublish(&api, &lot).await;

    let rows: Vec<(String, Value)> = sqlx::query_as("SELECT type, payload FROM webhook_events")
        .fetch_all(&api.pool)
        .await
        .unwrap();
    assert_eq!(
        rows,
        [(
            "passport.published".to_owned(),
            json!({ "lot_id": lot, "product_id": got["product_id"],
                    "public_code": got["public_code"], "version": 1 })
        )]
    );
}

#[sqlx::test]
async fn members_read_writers_need_product_update_outsiders_get_404(pool: PgPool) {
    let (api, lot) = world(pool).await;
    put(&api, &lot, passport()).await;
    let bob = member_with(&api.app, &api.pool, &api.ws, "bob@example.com", &[]).await;
    let (status, _) = api
        .call(Method::GET, &format!("/{lot}/passport"), None, &bob)
        .await;
    assert_eq!(status, StatusCode::OK);
    for (method, path, body) in [
        (Method::PUT, "/passport", Some(json!({ "title": 5 }))),
        (Method::POST, "/passport/publish", None),
        (Method::DELETE, "/passport/publish", None),
    ] {
        let (status, _) = api
            .call(method.clone(), &format!("/{lot}{path}"), body, &bob)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }

    let eve = api.as_new_owner("eve@example.com", "evil").await;
    for (method, path, body) in [
        (Method::GET, "/passport", None),
        (Method::PUT, "/passport", Some(passport())),
        (Method::POST, "/passport/publish", None),
        (Method::DELETE, "/passport/publish", None),
    ] {
        let (status, _) = eve
            .call(method.clone(), &format!("/{lot}{path}"), body, &eve.owner)
            .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{method} {path} via own workspace"
        );
    }
    assert_eq!(show(&api, &lot).await["version"], 0, "untouched");
}

#[sqlx::test]
async fn passports_with_files_need_file_get_as_well(pool: PgPool) {
    let (api, lot) = world(pool).await;
    let png = upload(&api, "hive.png", PNG_B64).await;
    let with_file = json!({ "title": "t", "banner_file_id": png });
    let carl = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "carl@example.com",
        &["product.update"],
    )
    .await;
    let dora = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "dora@example.com",
        &["product.update", "file.get"],
    )
    .await;
    let call = |method: Method, path: &'static str, body: Option<Value>, token: String| {
        let api = &api;
        let lot = lot.clone();
        async move {
            api.call(method, &format!("/{lot}{path}"), body, &token)
                .await
        }
    };

    let (status, res) = call(
        Method::PUT,
        "/passport",
        Some(with_file.clone()),
        carl.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "save with a file: {res}");
    let (status, res) = call(
        Method::PUT,
        "/passport",
        Some(json!({ "title": "t" })),
        carl.clone(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "no files, no file.get needed: {res}"
    );
    let (status, res) = call(Method::PUT, "/passport", Some(with_file), dora.clone()).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = call(Method::POST, "/passport/publish", None, carl.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "publish with a file: {res}");
    let (status, res) = call(Method::POST, "/passport/publish", None, dora).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(show(&api, &lot).await["version"], 1);
}
